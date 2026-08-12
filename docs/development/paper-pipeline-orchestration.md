# 打开论文与后台流水线编排

> 状态：设计草案。零 schema 变更（不新增表 / 列 / migration）。
> 关联：[architecture](../architecture.md) · [catalog](../backend/catalog.md) · [import](../backend/paper-import.md) · [pdf](../frontend/pdf.md)

## 1. 一句话

把「打开论文」的关键路径压到 **1 个聚合 IPC + 1 次 PDF 读取**，其余五条流水线（下载 / liteparse / 引用 / 图谱 / layout）统一交给 Rust 侧一个**内存 Job Center** 按 `focus / normal / idle` 三档优先级调度，用 `job:changed` 事件取代轮询。

```text
T0 交互轴  paper_open_bundle → readFile(PDF) → 骨架屏消失
              ↓ job_focus_paper（fire & forget）
T1 就近轴  Job Center 把当前论文的待办提到 focus lane → job:changed → 局部刷新
T2 idle轴  reconcile 扫描补齐全库缺失产物
```

## 2. 背景与动机

### 2.1 关键路径过长

`loadTabResources`（`src/lib/workspace/tabs/resources.ts:178`）**全程串行**四个 round-trip：

| 步骤 | 位置 | 成本 |
|---|---|---|
| `vault_allow_fs_scope` | `:217` | ~1ms |
| `detectPaperDirectory` 读 NOTES.md | `:220` | ~2ms |
| `paper_get` | `:263` | 5–15ms，每次重开连接 + 重跑 migration 阶梯 |
| `findLocalPdfPath`（`readDir` 递归 4 层） | `src/lib/paper/media.ts:120-210` | 2–30ms |
| `readFile(PDF)` + `Uint8Array` 全量拷贝 | `src/lib/vault/fs.ts:41-60` | 100–400ms |
| `readVaultFile(NOTES.md)` | `:287` | ~2ms |

metadata / PDF 发现 / notes seed 三者互不依赖却串行等待。

### 2.2 触发点散落

| 流水线 | enqueue 点数 | 位置 |
|---|---|---|
| layout | **6** | `import-actions.ts:145,188,302`、`library-actions.ts:171,279`、`resources.ts:89,145`、`use-pdf-layout-run.ts:278` |
| refs | **3** | `paper_import/mod.rs:192`、`import/mod.rs:391`、`import/mod.rs:540` |
| download | **2** | `lookup_import_batch` 内的 `SyncDownload` + `import-actions.ts:158-200` 事后重新 enqueue |

去重全靠 module-level `Set`（`enqueue-paper-layout.ts:19`、`resources.ts:52,55`、`refs.ts:121`），**重启即失忆**，且 `enqueue-paper-layout.ts:120` 在 `finally` 里 `delete`，同一篇会被反复重排。

### 2.3 轮询、全量重写、自回声

- **轮询**：`use-pdf-layout-run.ts:285-380` 每 1.5s 全量读 + `JSON.parse` + 校验 `layout.json`，最长 15 分钟（`:295,322,331`）。而 `vault:file-changed` 事件早已存在（`src/lib/vault/fs-watch.ts:17`）。
- **全量重写**：`layout-translate.json` 每译完一块就整文件 `JSON.stringify(…, null, 2)` 重写（`use-pdf-layout-translate.ts:117` → `layout-translate.ts:396`）。40 页论文 400+ 次全量写，字节数二次增长，无 temp+rename——写一半崩溃即毁缓存。
- **缓存命中仍做真活**：`run-analysis.ts:336` 每次都重跑 `mergeCaptionsIntoHosts`（41KB 几何逻辑），`:359` 每次都重写 `layout-index.json` → **每次打开论文必产生一次无意义磁盘写**，进而触发 watcher。
- **自回声**：`catalog.sqlite*` 未被 watcher 忽略（`src-tauri/src/features/watcher/mod.rs:142-152`）+ `use-vault-file-events.ts:111-119` 判定影响库 → `paper_set_page_counts` 写页数即触发全量 `paper_list`。

### 2.4 连接与锁

- 每个 catalog 命令都走 `ensure_catalog`（`src-tauri/src/features/catalog/schema.rs:109-129`）：`create_dir_all` + `Connection::open` + 4 个 PRAGMA + **5 次 `schema_version` 探测**（`:132-222`）。
- 所有 `wiki_*` 命令串在一把全局 `std::sync::Mutex` 上，连纯读也 `let mut guard`（`wiki/commands.rs:31,52,68,92`）；rebuild 与 `wiki_embed_read` 的文件读也在锁内（`index.rs:708-711`）。poison 只报不恢复。

## 3. 范围

### 要做

- 聚合命令 `paper_open_bundle`，收敛 T0 的多次 IPC。
- Rust 侧内存 `JobCenter`：三档优先级 lane、指纹去重、依赖顺序、并发上限、`job:changed` 事件。
- 渲染进程降级为 layout / translate 的 executor（`job:offer` → `job_report`）。
- enqueue 点从 11 处收敛到 3 个入口。
- 内存 `CapsCache` 能力位缓存，消灭重复目录遍历。
- `CatalogHandle` 进 Tauri `State`，migration 只在 setup 跑一次。
- 统一 sidecar 写入器：debounce + 原子写 + 自写抑制。

### 不做（本篇）

- 不新增表 / 列 / migration。能力位与 job 队列都是**内存派生态**。
- 不把 layout 推理搬去 Rust（见 §4 约束）。
- 不改 sidecar 的 schema 与文件名。
- 不改 PDF 字节的读取方（仍是前端 `plugin-fs`，PDFium 需要 `ArrayBuffer`）。
- 不重写 wiki 解析算法本身（只记录成本，见 §6.4）。

## 4. 硬约束

1. **layout 推理必须留在渲染进程**。引擎是 PP-DocLayoutV3 ONNX，跑在 `@embedpdf/plugin-layout-analysis` + onnxruntime-web（WebGPU/WASM）。Rust 只负责下载模型（`layout_model/mod.rs:29-37`）并通过自定义协议 `agentero-model://` 供给（`app/mod.rs:50`、`src/lib/pdf/layout/model.ts:34`）。因此目标不是「全搬去 Rust」，而是**队列语义全归 Rust，渲染进程只做 executor**。
2. **零 schema 变更**。能力位与 job 状态一律内存态 + 磁盘重建。
3. **local-first 不破**：磁盘上的 Markdown / sidecar 始终可被外部工具读写，任何内存缓存都必须能从磁盘无损重建。

## 5. 三条时间轴（纪律）

| 轴 | 预算 | 允许做的事 | 判据 |
|---|---|---|---|
| **T0 交互轴** | 0–300ms | 只做「没有它就看不到内容」的事 | 阻塞骨架屏消失的都在这 |
| **T1 就近轴** | 0.3s–数秒 | 当前这篇论文的增强 | 用户正看着，值得抢优先级 |
| **T2 idle 轴** | 分钟级 | 全库 backfill | 只在 idle 跑，可随时中断 |

现状问题正是三轴混杂：`loadTabResources` 里塞了 T1 的活（`maybeTriggerDeferredParse`、`loadPaperRefsAuto`），而 T2 的活（页数 backfill）又通过 watcher 自回声打回 T0。

## 6. 现状流水线清单

### 6.1 下载资产（Rust，串行 PDF→TeX）

命令 `lookup_import_batch` / `paper_download_assets` / `paper_import_local_pdf` / `paper_parse_body` / `paper_import`（`app/handlers.rs:66-74`）。唯一事件 `background-task:progress`，phase = `pdf|tex|parse|import`（`import/assets.rs:30-43`），前端监听 `src/lib/core/background-tasks.ts:334-360`。

```text
lookupSubmit (import-actions.ts:69)
 → lookup_import_batch → buffer_unordered(5)        // import/mod.rs:356
   → resolve_metadata(Translator) → enrich_remote_urls
   → paper_commit (paper_import/mod.rs:94)
      ├ allocate_paper_path + mkdir
      ├ 写 NOTES.md（标题 + 摘要 zh-CN 机翻，import/mod.rs:1040-1076）
      ├ papers::upsert_paper                        // sqlite 为事实来源
      ├ ensure_paper_assets  ← 180s 总超时，PDF 完再 TeX，串行
      └ refs::spawn_parse_after_import
```

- **PDF 候选链**（`assets.rs:301`）：`meta.pdf_url` → `arxiv.org/pdf/{id}` → `/{id}.pdf` → `export.arxiv.org` → Crossref links（`:388`）→ Unpaywall `best_oa_location`（`:338`）。落地前校验 `%PDF` 魔数 + 反 HTML（`:472-488`）。**无 per-URL 重试**，候选链本身即兜底。
- **TeX**：仅有 `arxiv_id` 时拉 `arxiv.org/e-print/{id}` → `flate2::GzDecoder` → `tar::Archive` → `extract_tar_safe`（防路径穿越，`:582-638`）。
- **取消**：逐 chunk 轮询 `is_cancelled`。
- **远程 SFTP**：`remote/import_bridge.rs:114` 暂存到 `session.work_root` → 复用同一批本地 helper → `upload_tree` → `catalog.push`。**严格串行且不发进度**（`:90-102`）。

**磁盘布局** `{vault}/papers[/sub]/{id}/`：

```text
NOTES.md              用户笔记（导入时写入标题 + 机翻摘要）
{id}.pdf              规范 PDF —— 在根目录，不在 source/
PAPER.md              liteparse 产物
source/**             解压后的 LaTeX
source/agentero-cite.json    引用 sidecar
source/layout.json           layout 原始 region
source/layout-index.json     layout post-merge 索引
source/layout-translate.json layout 译文缓存
```

> `metadata.json` 只被读取（legacy / rescan，`catalog/commands.rs:242`），本流水线不写它。

### 6.2 liteparse（Rust，子进程隔离）

crates.io `liteparse = "2.5"`（`src-tauri/Cargo.toml:79`），PDFium + OCR，仅桌面（移动端返回「在配对的桌面主机上运行」，`pdf_parse/mod.rs:456`）。

**关键设计**：不在进程内调用，而是 **re-spawn 应用自身二进制**当 worker——`--agentero-internal-pdf-parse-worker <pdf> <response.json>`，`kill_on_drop`，120s 超时，100ms 轮询取消（`pdf_parse/mod.rs:263,304`）。worker 分支在 Tauri init 之前执行。目的是隔离 liteparse 可能的 crash / OOM，**这个设计保留**。

- 守卫（`:150`）：有本地 TeX 就跳过 → `PAPER.md` 已存在且非 `force` 就返回。天然幂等。
- 配置（`:408`）：`ocr_enabled`、`ocr_failure_fatal:false`、Markdown、`ImageMode::Off`、`max_pages:500`、`extract_links`。
- 产物：`PAPER.md` + catalog 的 `body_source`(`pdf|ocr|latex`) / `body_quality`(`medium|low|high`)，经 `update_catalog_body`（`:466`）写入。
- 调用方：`paper_parse_body`（`force:false`）有两个入口——前端 `enqueuePaperPdfParse`（`src/lib/paper/enqueue-paper-pdf-parse.ts:56`）与 CLI（`cli/src/commands/paper.rs:650`）。
- 懒触发：`maybeTriggerDeferredParse`（`resources.ts:58-83`）在「有 PDF、无 TeX、无 `PAPER.md`」时调 `enqueuePaperPdfParse`（`:78`）——已是正确的轻量路径。
- ⚠️ **`PAPER.md` 不由 Rust 的导入/下载命令生成**：`paper_commit`（`paper_import/mod.rs:94-204`）与 `download_paper_assets_with_progress`（`import/mod.rs:458-522`）都只做「资产 + refs spawn」，均无 liteparse 调用。只有 Connector 路径在 Rust 内联生成（`connector/import.rs:174,356,733,797,887,925`、`connector/state.rs:536`）。identifier / 本地 PDF / BibTeX 三条入口**完全依赖前端** `enqueuePaperPdfParse`。详见 [paper-behavior-dag.md](paper-behavior-dag.md) §3.1。

### 6.3 引用解析（Rust，指纹幂等）

`paper_refs_parse` / `paper_refs_list` / `paper_refs_graph`（`refs/commands.rs:39,57,83`）。

- **本地源优先级**（`mod.rs:670`）：`.bbl`（提供编号顺序 → `display:"[n]"`）→ `.tex` 内联 `\begin{thebibliography}` → `.bib`；`.bib` 始终按 cite key 反向补全 `.bbl` 条目（`enrich_from_bib`，`:806`）。**完全不解析 PDF 文本。**
- **幂等**：`fingerprint` = SHA-256(schemaVersion + DOI + arXiv + online 标志 + 每文件 path/len/mtime)（`:640`）。命中且非 `force` 直接返回既有 sidecar（`:136-142`）。
- **在线**：仅当本文有 DOI / arXiv（`:150`）→ Semantic Scholar `references?limit=1000`（`online.rs:86`）→ Crossref `works/{doi}` 兜底（`:142`）。合并顺序 DOI → arXiv → 归一标题（精确后包含）→ year + 首作者姓氏模糊（`mod.rs:834-907`）。mode 标为 `bbl+s2` 这类。
- **关联本地库**：`attach_local_matches`（`:992`）用 `papers::list_all` 建 DOI / arXiv / 标题（≥15 字符）索引 → `LocalMatch{paperPath, matchBy}`。
- **写盘**：故意用 plain `fs::write`（`:594`）而非 temp+rename，为了不触发 watcher 的 "unverified rename" 提示。

> ⚠️ **零限流**：每次 HTTP 新建 `reqwest::Client`（`online.rs:60` 在 `get_json:69` 内），无池化、无 backoff、无 semaphore。`spawn_parse_after_import`（`mod.rs:210`）每篇一个无界 task，批量导入 N 篇 = N 个并发 S2 请求，稳定吃 429。唯一节流是前端 dedupe map（`refs.ts:121-151`）。

**PDF 内引用跳转是完全独立的路径**：`use-pdf-citations.ts` + `layers/citation-links.tsx:107` 靠 PDFium text rects 在 GoTo 目标处就地拼装条目（`mergeBibliographyEntryAtY:149`），按 `page:y` 缓存，**不读 sidecar**。`refs.ts:167,192` 的 marker 匹配函数只有测试在用——sidecar ↔ 正文标记的桥设计了但没接上。

### 6.4 图谱（两套，一套是死的）

| | 引用图 | wiki 图 |
|---|---|---|
| 命令 | `paper_refs_graph`（`refs/commands.rs:83`） | `graph_get_graph` |
| UI | `GraphPanel` 实际渲染的就是这个 | **零调用方，死代码** |
| 成本 | 每次调用读**全库每篇**的 sidecar（`mod.rs:306`），O(catalog) 次文件读 + JSON parse，无内存态 | 走磁盘 `discover_paper_folders`（`index.rs:981,1040`）+ 每节点读 `metadata.json`（`:1105`），全在锁内 |

`graph-panel.tsx:185` 调用时不传 `center`，所以 `selectedPath` prop 白传，`mod.rs:340-385` 的邻域 BFS 分支 UI 不可达。

**wiki index**（`wiki/index.rs:206`）：`edges: Vec<ResolvedLink>` + `reverse: HashMap` + `files` + `documents`，状态 `WikiIndexState{Mutex<WikiIndex>}`（`:1193`，`app/mod.rs:82`）。

构建：全量扫 vault（深度 24，`:161`）→ `fingerprint_files`（size + mtime_ns，`cache.rs:85`）→ 从 `cache/agentero/wiki/{sha256(vault)}.sqlite` 读快照（`:114`）→ 指纹一致即热启（`index.rs:298`），否则 `rebuild_from`（`:343`）增量重读变更文件但**重新 resolve 全部 occurrence**（`:417`）。

重建触发：`ensure_vault` 仅在 vault 路径变化时（`:481`）；显式 `graph_rebuild`（`commands.rs:178`）/ `wiki_cache_rebuild`（`:208`）；重命名与标题重命名事务（`rename.rs:582,591,623`、`heading_rename.rs:361,423,452`）。前端 watcher 900ms trailing debounce + 4s 自写回声抑制（`src/lib/wiki/store.ts:90-119`）。**打开论文不触发重建——这点现状正确，保留。**

成本：`get_backlinks`（`:488-502`）遍历所有 reverse key 做 `eq_ignore_ascii_case`（本应 hash lookup）；`get_outgoing`（`:523`）/ `check_links`（`:564`）O(全部边)；`resolve_document`（`resolve.rs:86-141`）线性扫 documents → 冷启动近似 O(edges × documents)。

### 6.5 layout 分析（渲染进程 ONNX）

```text
enqueuePaperLayoutAnalysis                  // 6 个调用点
 → queuedPapers Set 去重 (enqueue-paper-layout.ts:19,35)
 → readLayoutSidecar 预检 (:41)
 → enqueueBackgroundTask(kind:"parse", concurrency:1) (:56,111)
 → analyzePaperLayoutHeadless (headless-analyze.ts:109)
    ├ 每篇新建 PluginRegistry (:145-246)
    ├ PDFium 模块级 singleton worker engine (:35)，8s ready 超时 → 掉回主线程 direct engine (:78)
    ├ ensureLayoutModel → layout_model_ensure（单飞，model.ts:111-160）
    └ scope.analyzeAllPages (run-analysis.ts:423)
 → mergeCaptionsIntoHosts + NMS dedupe
 → writeLayoutSidecar + writeLayoutIndexFromRaw (run-analysis.ts:517-540)
```

- **region 类型**（`src/lib/pdf/layout/types.ts:33`）：`image/table/algorithm/formula/formula_number/chart/figure_title/header/abstract/text` + `score` + `readingOrder` + `rect`(PDF 点) / `bbox`(归一 0–1)。
- **文字来自 PDFium text layer 而非 OCR**：`scope.getPageTextRuns` 填充 `text`/`title`（`run-analysis.ts:189,250`）。
- `tableStructure: false`（`headless-analyze.ts:154`）→ 不抽表格单元结构。
- **队列**：`enqueueBackgroundTask` 是 zustand store + per-kind `Semaphore`（`background-tasks.ts:384-430`）。`concurrency` 按 kind 计，`"parse"` 与其他 parse 任务共用一条道。
- **消费**：`layoutAnalysisStore`（`store.ts:31`，按 EmbedPDF `documentId` 索引）→ `usePdfLayoutRegions` → 图表侧栏（`figures-panel.tsx:230-384`）、overlay（`page-layers.tsx:265,299-301,316`）、命中测试（`hit-test.ts:107`，最小面积优先，同面积比 score）。**文本选择 / 复制不消费 layout**，仍走 PDFium selection。

**翻译路径**：工具栏 toggle 才启动（`use-pdf-layout-translate.ts:83`）。`listTranslatableLayoutRegions`（`layout-translate.ts:158`）是**整文档粒度**，跳过 algorithm 正文 / `reference` / `aside_text` / "Algorithm N" 标题 / "References" 标题，单块截断 `LAYOUT_TRANSLATE_MAX_CHARS = 2500`（`:39`）。cache key = `{providerId, sourceLang, targetLang, serviceKey}`（`:260`），命中的 item 直接标 `done`（`:378-386`，即 commit `6dcd1b61`）。并发 `LAYOUT_TRANSLATE_CONCURRENCY = 2`（`:42`），Agent provider 强制 1（`:580`）。**一 region 一请求，无 batch、无同文本去重**；Agent provider 绕过 Rust 走 ACP `runOnce`（`:522-556`），其余经 `bindings.ts:8` → `translate_text`（`translate/commands.rs:10`）。

### 6.6 sidecar 一览

| 文件（均在 `{paper}/source/`） | 常量 | ver | 失效条件 |
|---|---|---|---|
| `agentero-cite.json` | `SIDECAR_FILE`（`refs/mod.rs:23`） | — | fingerprint 不符 |
| `layout.json` | `LAYOUT_SIDECAR_FILE`（`io.ts:13`） | 2 | `schemaVersion` 不符；**单个坏 region 整文件作废**（`io.ts:121,128`） |
| `layout-index.json` | `LAYOUT_INDEX_FILE`（`layout-index.ts:23`） | 1 | — |
| `layout-translate.json` | `LAYOUT_TRANSLATE_SIDECAR_FILE`（`layout-translate.ts:45`） | 1 | cacheKey 不符整文件弃（`:346`），再按 `id` + 源文本逐条校验（`:386`） |

**全部为整文件 `JSON.stringify(…, null, 2)` 重写**，经 `writeVaultFile` → `plugin-fs writeTextFile`（`vault/fs.ts:88`），无 temp+rename。

## 7. 目标编排

### 7.1 T0：4 个串行 IPC 压成 1 个

新增聚合命令 **`paper_open_bundle(paperPath)`**：

```rust
struct PaperOpenBundle {
    paper: PaperRow,            // 现有表，不加列
    pdf: Option<PdfLocator>,    // { abs_path, size, page_count } —— 取自 CapsCache，不 walk
    notes: String,              // NOTES.md 内容
    marks: MarksSnapshot,       // asks + translates + visualTraces，一次目录读 + 批量读
    sidecars: SidecarStatus,    // 只回状态位，不回内容
}

enum Freshness { Missing, Stale { fingerprint: String }, Fresh }

struct SidecarStatus {
    refs: Freshness,              // 比 refs fingerprint
    layout: Freshness,            // 比 schemaVersion
    layout_translate: Freshness,  // 比 cacheKey
}
```

`sidecars` 只回「有没有 / 新不新」，让前端立刻知道该不该等，无需读内容。

**替掉**：`resources.ts:217-287` 的 4 个串行 round-trip、`media.ts:120-210` 的 4 层递归、`use-pdf-marks-io.ts:132` 的 3 次 `readDir`（`marks-io.ts:51-82`）。

### 7.2 T0 时间线

```text
t=0ms      同步：placeholder tab + openPanel + 1 次 store 写（现状 5 次）
t=0~15ms   paper_open_bundle(path)                    ← 唯一阻塞 IPC
t=15ms     readFile(pdfPath) → ArrayBuffer            ← 唯一大 IO，100~400ms
           并行 fire&forget：job_focus_paper(path)     ← 不 await
t≈200ms    updateTab 一次落盘 → 骨架屏消失，NOTES 立刻可编辑
t+150ms    PDFium parse → totalPages
t+200ms    并发：outline ∥ 阅读位恢复 ∥ annotations import ∥ pageTextRects
           （marks 已在 bundle 里，不再读盘）
```

PDF 字节读取**仍留在前端**：PDFium 需要 `ArrayBuffer`，走 Rust 反而多一次跨进程拷贝。

### 7.3 Job Center（内存，Rust）

```rust
// src-tauri/src/features/jobs/
pub enum JobKind {
    DownloadAssets, ParseBody, ParseRefs,
    LayoutAnalyze, LayoutTranslate, PageCount, WikiReindex,
}

pub struct Job {
    id: JobId,
    kind: JobKind,
    paper: Option<PathKey>,
    fingerprint: String,
    depends_on: Option<JobId>,   // 取代嵌套 tokio::spawn
    attempts: u8,
    progress: f32,
    phase: String,
}

pub struct JobCenter {
    lanes: Mutex<[VecDeque<Job>; 3]>,                  // focus / normal / idle
    seen: Mutex<HashSet<(JobKind, PathKey, String)>>,  // kind + path + fingerprint 去重
    running: Mutex<HashMap<JobId, RunningJob>>,
    permits: HashMap<JobKind, Arc<Semaphore>>,
}
```

**三条 lane 而非优先级堆**：`job_focus_paper` 切换时只需把 job 从 `normal` lane 挪到 `focus` lane，比堆里改 key 简单。

去重用会话内 `HashSet`，替掉现有的 `queuedPapers`（`enqueue-paper-layout.ts:19`）、`paperParseTried` / `pdfAutoDownloadTried`（`resources.ts:52,55`）、refs in-flight map（`refs.ts:121`）。

#### 执行位置与并发

| kind | 执行位置 | 并发 | 说明 |
|---|---|---|---|
| `DownloadAssets` | Rust | 3 | 现成实现改为从 lane 领活 |
| `ParseBody`（liteparse） | Rust 子进程 | **1** | CPU + 内存重，子进程隔离保留 |
| `ParseRefs` | Rust | 2 + **HTTP semaphore(2)** | 修掉现在 N 并发打 S2 的 429 |
| `LayoutAnalyze` | **渲染进程** | 1 | ONNX，只能在这 |
| `LayoutTranslate` | **渲染进程** | 2 | 需 BYOK provider / ACP |
| `PageCount` | Rust | 4 | 别再在前端起 PDFium 数页数 |
| `WikiReindex` | Rust | 1 | debounce 900ms，沿用现状 |

#### 渲染进程如何领活

```text
Rust      --job:offer {jobId, kind, paperPath}-->   渲染进程
渲染进程   跑 ONNX / 调 translate provider
渲染进程   --job_report(jobId, ok|err)-->            Rust
Rust      --job:changed {jobId, paperPath, kind, state, progress}--> 所有窗口
```

队列语义、幂等、重试、优先级、取消**全在一处**；渲染进程退化为纯 executor。附带收益：多窗口不会各跑一份（现状 `layoutAnalysisStore.ui` 是全局单例，`store.ts:16`，多 tab 进度互串）。

`job:changed` 事件（节流 100ms 批量发）**直接干掉 `use-pdf-layout-run.ts:285-380` 的 1.5s × 15min 轮询**。

#### 重启丢失 in-flight job —— 不需要处理

**每个 job 本身就是幂等的收敛操作**：

| kind | 幂等守卫 |
|---|---|
| `ParseBody` | 见 `PAPER.md` 即返回（`pdf_parse/mod.rs:150`） |
| `ParseRefs` | fingerprint 一致即返回（`refs/mod.rs:136`） |
| `LayoutAnalyze` | `schemaVersion` 匹配即返回（`io.ts:121`） |
| `DownloadAssets` | 有本地 PDF 即跳过（`assets.rs:191`） |

因此启动后跑一次 **`reconcile` 扫描**（`idle` lane、分批、低并发）：遍历 catalog，对能力位缺失的补 enqueue。重扫安全，代价只是一遍 `caps_for`。

> **真相始终在磁盘 sidecar + catalog 里，jobs 只是「待收敛清单」**——一个可以随时从磁盘重算的派生物，本就不该持久化。故不做 `jobs.json`：少一个持久状态就少一个不一致来源。

### 7.4 触发时机：11 处收敛到 3 个入口

#### 入口 ① 导入完成 —— `paper_commit` 尾部，一次建链

```text
DownloadAssets
   ├→ ParseBody      (depends_on)
   │    └→ ParseRefs
   ├→ PageCount
   └→ LayoutAnalyze
```

**只在这一处 enqueue**。删除 `import/mod.rs:391`、`:540` 的重复 spawn；删除 `import-actions.ts:158-200` 那段「导入完成后再对同一批路径 enqueue `paper_download_assets`」的往返。

#### 入口 ② 打开论文 —— `job_focus_paper(paperPath)`

**不产生新 job**，只做两件事：

1. 把该论文的 `queued` job 从 `normal` lane 移到 `focus` lane，并把上一篇降回 `normal`（tab 切换即重排）。
2. 仅对 `CapsCache` 显示**缺失**的产物补 enqueue。

于是删除 `use-pdf-layout-run.ts:278`（每次打开都 enqueue）、`resources.ts:89,145`、`library-actions.ts:171,279`。

#### 入口 ③ watcher 变更 —— debounced，指纹驱动

文件指纹真的变了才 enqueue 对应 kind。同时 watcher **必须忽略** `catalog.sqlite*`（`watcher/mod.rs:142-152` 现状故意不忽略）与自身刚写的 sidecar，掐死 `paper_set_page_counts` → `vault:file-changed` → 全量 `paper_list` 的自回声。

### 7.5 完整时间线

```text
═══ T0 交互轴 ═══════════════════════════════════════════
0ms       placeholder tab + panel（1 次 store 写）
0-15ms    paper_open_bundle                    ← 唯一阻塞 IPC
15ms      readFile(PDF) ∥ job_focus_paper(fire & forget)
~200ms    updateTab → 骨架屏消失，NOTES 可编辑
~350ms    PDFium parse → totalPages
~400ms    outline ∥ 阅读位恢复 ∥ annotations ∥ pageTextRects

═══ T1 就近轴（focus lane 抢占）═════════════════════════
+0.5s     Rust: ParseRefs（缺失/过期时）
            → job:changed → 引用面板局部刷新
+1s       Rust: job:offer(LayoutAnalyze) → 渲染进程 ONNX
            → 进度经 job:changed 推送（无轮询）
+Ns       layout 完成 → 图表侧栏填充
用户点翻译 → LayoutTranslate job（唯一「仅用户显式触发」的 kind）

═══ T2 idle 轴（idle lane）══════════════════════════════
idle      reconcile 扫描（启动后一次）
idle      PageCount backfill（Rust，并发 4）
idle      缺失 PAPER.md 的 ParseBody（并发 1）
idle      refs 指纹过期重解析（HTTP semaphore 2）
idle      WikiReindex（debounce 900ms，沿用现状）
```

**idle 判定**：无 `focus` lane job in-flight，且用户 N 秒无输入。任一条件破裂即暂停 T2，把 CPU 让给 T1。

### 7.6 与既有 background-tasks 的合并

**Rust 侧目前没有任务系统**：`features/agent/background_tasks.rs` 只有 24 行，是个 `CANCELLED: Mutex<HashSet<String>>` 取消标志位，由 worker 轮询 `is_cancelled`。队列完全活在渲染进程的 `src/lib/core/background-tasks.ts`——它实际是「UI 进度台账」+ 外挂的 per-kind Semaphore（`:385-431`）。

现有系统装不下 Job 的三个缺口，正好解释了 layout / pdfParse 为什么各自造轮子：

| 缺口 | 后果 |
|---|---|
| 无指纹去重 | `enqueue-paper-layout.ts:19` 与 `enqueue-paper-pdf-parse.ts:15` 各自维护 `queuedPapers` Set，且都在 `finally` 里 `delete`，重启即失忆 |
| `Promise<T>` 只对 enqueue 方兑现 | 另一 tab / CLI 写完 sidecar 对本 tab 不可见 → 退化成 1.5s 轮询（`use-pdf-layout-run.ts:331`） |
| 无优先级 | `getSemaphore` 是 FIFO，T2 的 idle backfill 会挡在 T1 的 focus 任务前。附带隐患：`sem.setMax(concurrency)`（`:428`）让**最后一个调用者决定并发度** |

#### 判定规则

> 有人 `await` 它的返回值 → **Operation**，留前端。
> 没人等结果、跑两遍无害 → **Job**，进 JobCenter。

| 归属 | 项 | 理由 |
|---|---|---|
| **Operation** | `export` `import` `lookup` `paperRead` `connector` | 调用方要结果、非幂等。`lookup_import_batch` 本身是一个批操作（内部已 `buffer_unordered(5)`），它**产出的**后续任务才归 Job |
| **Job** | `LayoutAnalyze` `ParseBody` `ParseRefs` `PageCount` `WikiReindex` | 全部有幂等守卫（见 §7.3） |
| **Operation** | `resources.ts:132` 的 download | 打开论文缺 PDF 时下载在关键路径上，调用方要 PDF 字节 |
| **Job × N** | `downloadAllMissingAssets` | 见 §10.2，整个函数应消失 |

#### 投影机制已经现成

`startBackgroundTask({ id: 稳定id, kind })` + `updateBackgroundTask` 就是为 Host 驱动任务设计的，`layout-model`（`src/lib/pdf/layout/model.ts:50`，事件 `layout-model:task`）与 `connector`（`use-connector-sync.ts:118`）已经在这么用。JobCenter 只需沿用同一条路：`job:changed` → 同一组 store 函数。**不新增 UI 概念，不重建面板**（`background-tasks-panel.tsx`）。

#### 三个必须同时处理的细节

1. **idle 噪音不能淹没面板**。`PageCount` / `WikiReindex` / `reconcile` 若每项一行，面板会常驻转圈。这类 job 默认**不投影**，仅失败时露出，或聚合成一条「后台索引 N 项」。
2. **取消收口**。现在两条路：前端 `AbortController` map（`:113`）+ Rust `CANCELLED` HashSet。那个 24 行的 HashSet 就是 JobCenter 的雏形，直接由它接管；`background_task_cancel` 命令语义不变。
3. **listener 泄漏**。`attachProgressListener`（`:335`）为每个任务 `listen("background-task:progress")` 一次再按 `taskId` 过滤 → N 个任务 N 个 listener 且全收所有事件。改为一个全局 listener + 按 id 路由。

#### kind 映射

复用现有 `BackgroundTaskKind`，只补一个：

| JobKind | BackgroundTaskKind |
|---|---|
| `DownloadAssets` | `download` |
| `ParseBody` | `pdfParse` |
| `ParseRefs` | `parse` |
| `LayoutAnalyze` | **新增 `layout`** |
| `PageCount` / `WikiReindex` | 不投影（见上） |

新增 `layout` 是因为现在 layout 借用 `parse`（`enqueue-paper-layout.ts:58`），会与 refs 撞同一个 semaphore；i18n key `app:tasks.layoutAnalysis` 已存在。

## 8. 配套基础设施

### 8.1 统一 sidecar 写入器

`debounce(500ms)` + `temp + rename` 原子写 + 写完主动 suppress watcher。

治掉 `layout-translate.json` 每块译完即整文件重写的问题（`use-pdf-layout-translate.ts:117`）。

> 注意：refs 现在故意用 plain `fs::write`（`refs/mod.rs:594`）来避开 watcher 的 rename 告警。加了主动 suppress 之后即可统一改回原子写。

### 8.2 缓存命中就真的什么都不做

`run-analysis.ts:336` 现状每次都重跑 `mergeCaptionsIntoHosts`，`:359` 每次都重写 `layout-index.json`。命中路径应直接从 `layout-index.json` 反序列化，零写盘。同时消除 `:353` 的 `needsText` 回写（它会从 viewer 侧改写 `layout.json`，与 headless writer 抢同一文件）。

### 8.3 `CatalogHandle` 进 State（零新增 SQL）

```rust
// src-tauri/src/features/catalog/handle.rs
pub struct CatalogHandle {
    inner: RwLock<Option<(PathBuf, Connection)>>,   // vault 切换时重开
}
```

setup 时建连接 + 跑一次现有 migration 阶梯；`ensure_catalog`（`schema.rs:109`）退化为 `handle.conn()`。vault 切换时比对 `PathBuf`，不同则重开并重跑 migration（migration 本身幂等）。

> ⚠️ **必须成对做**：单连接会让 `paper_list` 的全表扫描（`papers.rs:417`）阻塞 `paper_get`。所以同一步要把 `paper_list` / `paper_rescan` / `paper_refs_list` / `wiki_*` 从同步 `fn` 改成 `spawn_blocking`，否则可能比现状更卡。

### 8.4 `CapsCache` 内存能力位

```rust
// src-tauri/src/features/catalog/caps.rs
pub struct Caps {
    pdf_path: Option<PathBuf>,     // 已解析的规范 PDF 路径
    has_tex: bool,
    has_paper_md: bool,
    page_count: Option<u32>,
    refs_fingerprint: Option<String>,
    layout_schema: Option<u32>,
    translate_key: Option<String>,
}
pub struct CapsCache { inner: RwLock<HashMap<PathKey, Caps>> }
```

- **懒填**：`caps_for(path)` miss → 一次目录 walk（复用现有 `find_local_pdf`）+ 读 sidecar 头部 → 落缓存。
- **失效**：watcher 事件命中某 paper 目录 → `remove(key)`；vault 切换 → `clear()`。
- `RwLock` 而非 `Mutex`（读远多于写）。

**为什么内存缓存比加 DB 列更对**：能力位是磁盘状态的派生量，塞进 catalog 表就有 stale 风险——而 local-first 恰恰要求文件可被外部工具随意读写。内存缓存 + watcher 失效永远可以从磁盘重建，不存在「两份真相打架」。

收益：一次导入 5+ 次目录遍历（`assets.rs:191,289`、`paper_import/mod.rs:128,240`、`pdf_parse/mod.rs:159`）→ 1 次；打开论文的 4 层递归 `readDir` → 命中即 0 次。重启后首次打开某篇多付一次 walk（几 ms），可忽略。

### 8.5 依赖选型：复用 primitive，不引 job framework

AGENTS.md 要求「尽可能复用能力」，因此先调研了现成框架。结论是**没有可直接复用的**，原因是三处不匹配：

1. **执行体在渲染进程**。`LayoutAnalyze` 必须在 webview 里跑 ONNX。所有 Rust job framework 的 worker 都是 Rust `async fn`，「派活给 JS 客户端并等它回报」不在任何框架的模型里。
2. **去重键来自磁盘状态**。fingerprint = 文件 mtime/size + sidecar `schemaVersion`（refs 已如此，`refs/mod.rs:640`）。框架提供的是「job id 唯一」，不是「输入指纹一致则跳过」。
3. **明确不要持久化**（§4 约束 2）。而所有 OSS job framework 的核心价值恰恰是 storage backend 带来的 durability；去掉它之后框架剩下的就是 Semaphore + 队列。

调研记录（截至 2026-08）：

| 候选 | 版本 | 总下载 | 结论 |
|---|---|---|---|
| [`apalis`](https://github.com/apalis-dev/apalis) | 1.0.0-rc.9 | ~100 万 | 最成熟，但仍 RC；面向 server worker，后端 Redis/SQL；无原生 priority / 依赖链 / 指纹去重 |
| `tauri-queue` | 0.3.0 | **20** | 2026-07-05 首发，crates.io 未填 repository。不能作为核心调度依赖 |
| `agent-queue` | 0.2.0 | **35** | 同上，同作者同日发布 |
| `fang` / `backie` / `underway` | — | — | Postgres 强绑定 |
| `rusty-celery` | — | — | 需 AMQP / Redis broker |

应复用的 primitive（多数已在树里）：

| 需求 | 复用什么 | 状态 |
|---|---|---|
| per-kind 并发上限 | `tokio::sync::Semaphore` | ✅ 已是直接依赖 |
| 批内并发 | `futures_util::stream::buffer_unordered` | ✅ 已用于 `import/mod.rs:356` |
| 结构化取消 | `tokio_util::sync::CancellationToken` | ⚠️ 已在 `Cargo.lock`（传递依赖），提为直接依赖零编译成本；替掉 `features/agent/background_tasks.rs` 的 24 行 HashSet |
| job 状态扇出（取代轮询） | `tokio::sync::broadcast` | ✅ 已有 |
| HTTP 限流（治 §6.3 的 S2 429） | `governor` | ❌ **需新增，是唯一真正值得加的依赖** |
| `CapsCache` | `RwLock<HashMap>` | 标准库够用；`moka` 的 TTL / 容量淘汰在此无用，失效由 watcher 负责 |
| 三档 lane | `[VecDeque<Job>; 3]` | 标准库；`priority-queue` 反而更难实现「focus 切换时重排」 |

量级判断：JobCenter 核心约 **300–400 行 Rust**（lanes + seen + permits + tick loop + `job_offer`/`job_report` 两个命令 + 一个 broadcast 事件）。给任何框架写 adapter 都不会更短，还得绕开其持久化假设。

前端不需要引 `p-queue`：调度移到 Rust 后，`background-tasks.ts:385-431` 那个手写 `Semaphore` 基本可以删除。

值得**读源码而非依赖**的是 apalis 用 `tower` layer 叠加 concurrency / retry / timeout 的组织方式——若将来 `JobKind` 数量显著增长可以借鉴；当前 7 个 kind 不值得引入 `tower` 的抽象成本（尽管它也已在 `Cargo.lock` 中）。

## 9. 落地顺序

全程零 migration。每步一个独立 Conventional Commit。

| # | 改动 | 风险 | 收益 | 状态 |
|---|---|---|---|---|
| 1 | **函数清理**：删 `enqueue-paper-pdf-parse.ts:67-68` 内联刷新、给 `resources.ts:120-135` 补 `enqueuePaperPdfParse` 对齐行为、`refreshTreeQuiet` 合并进 `refreshTree`、`refreshAll` 改并发 | 低 | 一次下载少 2–3 次全量树重建；补上 `PAPER.md` 缺口 | 完成（`c794c19e`、`4fd4b1a7`） |
| 2 | **死代码删除**（§10.4）+ `is_complex()` 去掉 buffer 克隆 | 低 | 减面积；每篇少一次完整 PDF parse | 死代码已删（`8481cfe1`）；`is_complex()` 克隆**无法去除**——liteparse 的 `is_complex` 与 `parse_input` 各自消费 owned `PdfInput::Bytes`，且 `ParseResult` 不暴露 `needs_ocr`，质量标签必须靠独立预检 |
| 3 | sidecar 统一写入器（debounce + 原子写 + 自写抑制） | 低 | 治 400+ 次全量写与缓存损坏风险 | 部分：layout-translate debounce 完成（`36987514`）；原子写 + 自写抑制未做（与 watcher rename 行为耦合，需联调） |
| 4 | 缓存命中跳过 `mergeCaptionsIntoHosts` + `layout-index.json` 重写 | 低 | 每次打开省一次无意义写 | 完成（`21863560`，缓存命中零写盘；merge 仍内存执行以保留算法热更新） |
| 5 | watcher 忽略 `catalog.sqlite*` | 低 | 掐死自回声全量 `paper_list` | 完成（`caed3e6f`） |
| 6 | `CatalogHandle` 进 State + 重命令改 `spawn_blocking` | 低 | 每个 catalog 命令省 5 次 `schema_version` 探测 | 暂缓：共享 `Mutex<Connection>` 会串行化 catalog 访问，且 `papers::*` 若嵌套 `ensure_catalog` 会死锁（非重入锁）；会话级「跳过 migrate 探测」缓存又有「db 中途被删重建→误跳过建表」的边缘情况。两者收益（省 open + 探测）有限，需实机验证并发与边界后再做 |
| 7 | `CapsCache` 内存能力位 + 删 `collectPapersNeedingAssetDownload` | 中 | 消灭所有目录 walk | 完成：`CapsCache`（`ff5756be`）+ `job_papers_needing_assets` 查询取代前端树走（`58f5481d`/`9489ff1a`），谓词落入 `PaperCaps::needs_asset_download` 并有单测 |
| 8 | `paper_open_bundle` 聚合命令 | 中 | T0 从 4 IPC → 1 IPC | 完成（`ac17af96`） |
| 9 | `JobCenter` 内存队列 + `job:changed` 事件 + `governor` 限流 | 高 | 轮询消失，队列语义统一，S2 不再 429 | 完成：调度器（`61938b50`）、ParseBody / ParseRefs / LayoutAnalyze executor（`2952c686`）、per-kind 并发上限 + finish 后 drain（`aab07bd4`，§7.3）；在线引用以 `Semaphore(2)` 限流（`online.rs`）替代 `governor` |
| 10 | enqueue 点收敛到 3 入口 + `reconcile` 扫描 + 删 `downloadAllMissingAssets` / `loadPaperRefsAuto` | 高 | 重复触发消失（依赖 9） | 完成：投影(layout/parseRefs/parseBody/download)+取消收口+running 取消、layout/`enqueuePaperPdfParse` 委托、per-paper/vault-wide/open reconcile（含 ParseBody+ParseRefs）、`DownloadAssets` runner；已删 `maybeTriggerDeferredParse`、`collectPapersNeedingAssetDownload`、`loadPaperRefsAuto`（引用面板事件驱动 `03622080`），`downloadAllMissingAssets`/post-import 改 enqueue job |

1–6 互相独立且全为低风险，可先行兑现收益。9 的投入较大（约 300–400 行，见 §8.5），做完 1–8 后再评估。10 必须跟在 9 之后。

> 已额外兑现：§7.6 细节 3（`background-task:progress` 单一全局 listener 按 taskId 路由，`a0149774`）、§8.2（缓存命中只读——viewer 不再回写 `layout.json`，`fc943551`；配合索引未变更跳过，缓存命中零写盘）、§11「layout sidecar 轮询 → 0」（`use-pdf-layout-run.ts` 已改为 watcher 事件驱动）。
>
> 剩余未竟项：**6（`CatalogHandle` + `spawn_blocking`）** 与 **10（enqueue 收敛 + reconcile driver + `job:changed`→任务面板投影 + 取消收口 + 删前端 wrapper）**。§7.3 的 per-kind 并发已落入 JobCenter（`aab07bd4`），前端 wrapper 的 `{concurrency}` 门因此可安全移除，但仍须与任务面板投影 + 取消收口一并切换，避免进度/取消 UX 回退。3 仅剩「原子写 + 自写抑制」，因 sidecar 读时已校验可重建、缓存命中已零写，边际收益低。上述改动会改变 SQLite 连接线程模型 / 打开论文关键路径 / 任务面板取消语义的运行时行为，单测难以覆盖，建议在 `pnpm tauri dev` 下逐项验证后提交。

## 10. 函数审计与清理

### 10.1 重复的「下载后处理」尾巴（5 处，每处都不一样）

> **勘误**：本节此前断言「三处 `enqueuePaperPdfParse` 是保证空转、应删除」。核实后**结论反转**——Rust 的 `paper_commit` 与 `download_paper_assets_with_progress` 都不生成 `PAPER.md`，这些前端调用是 identifier 路径上唯一的生成环节。完整分析见 [paper-behavior-dag.md](paper-behavior-dag.md) §3.1。

同一段「下载完要做什么」被复制了五遍：

| 位置 | `downloadPaperAssets` | 刷新树 | `refreshLibrary` | `LayoutAnalysis` | `PdfParse` |
|---|---|---|---|---|---|
| `library-actions.ts:174-188`（单篇下载） | ✅ | `refreshTree` | ✅ | `:182` | `:185` |
| `library-actions.ts:289-300`（`downloadAll`） | ✅ | 循环外 `:309` | 循环外 | `:294` | `:297` |
| `import-actions.ts:190-203`（导入尾部） | ✅ | `refreshTree` | ✅ | `:197` | `:200` |
| `resources.ts:120-135`（`resolvePaperPdfSource`） | ✅ | 调用方 `actions.ts:312` 用 `refreshTreeQuiet` | ❌ | `:133` | **❌** |
| `resources.ts:58-83`（`maybeTriggerDeferredParse`） | ❌（只补 parse） | ❌ | ❌ | ❌ | `:78` |

仍然成立的两个问题：

1. **五处互不一致**。`resources.ts:120-135` 缺 `enqueuePaperPdfParse` 是真实缺口——从「打开论文时补下载」这条路进来的论文不会生成 `PAPER.md`，只能等下次打开由 `maybeTriggerDeferredParse` 兜（而它有会话内 `paperParseTried` Set，同一会话内只兜一次）。
2. **刷新风暴**。`enqueue-paper-pdf-parse.ts:67-68` 在任务体内联 `refreshTree` + `refreshLibrary`，而调用方（`library-actions.ts:180-181`、`import-actions.ts:195-196`）自己也刷 → 一次单篇下载触发 3–4 次全量 `vault_tree_build` + `paper_list`。

清理动作（修正后）：

- **不删** `enqueuePaperPdfParse` 调用——先把 `ParseBody` 变成 Rust 侧 DAG 节点（落地清单第 7 项），再一次性删掉全部 5 处前端调用。
- **可立即做**：删除 `enqueue-paper-pdf-parse.ts:67-68` 的内联 `refreshTree` / `refreshLibrary`；刷新交给 watcher。
- **可立即做**：给 `resources.ts:120-135` 补 `enqueuePaperPdfParse`，先把行为对齐（临时措施，第 7 项落地后一并删除）。

落到 §7.4 入口①后，这个尾巴由 `depends_on` 表达（`DownloadPdf` + `DownloadTex` → `ParseBody`），**五处复制归零**。

### 10.2 `downloadAllMissingAssets` 应整个消失

`library-actions.ts:259-315`，唯一触发点是文件树按钮（`file-tree.tsx:1814`）。三个问题：

1. **串行**：`for` 循环逐篇 `await`（`:278`），无并发。而 `import-actions.ts:205` 的同类下载却用了 `concurrency: settings.batchImportConcurrency`——同一件事两种行为。
2. **进度打架**：外层 `setProgress(i/total)`，内层 `downloadPaperAssets` 又用同一个 `progressTaskId: id`（`:292`）往同一个 task 发 `background-task:progress`（单篇字节百分比）。两者交替覆盖同一 `progress` 字段；`updateBackgroundTask` 的 `Math.max` 钳制（`background-tasks.ts:251`）掩盖了数值回退，但 `detail` 文案会闪。
3. **本质是 N 个 Job**，不是一个批操作。

清理动作：函数删除，按钮改为「enqueue N 个 `DownloadAssets` job（`idle` lane）」，UI 由 §7.6 的聚合行呈现。并发上限由 `JobKind` 配置统一决定，不再各处自带一个 `concurrency` 参数。

顺带：`BackgroundTaskKind` 的 `downloadAll`（`background-tasks.ts:16`、`background-tasks-panel.tsx:51`）随之删除，i18n key `app:tasks.downloadAll` 一并清理。

### 10.3 `auto` / `quiet` / `maybe` 类函数逐个审计

| 函数 | 位置 | 判定 | 处理 |
|---|---|---|---|
| `loadPaperRefsAuto` | `refs.ts:132` | 「list → 若 null 则 parse」的读时兜底，正是 Job 的定义 | **删除**。前端只读 `paper_open_bundle` 里的 refs 状态位；补齐交给 `ParseRefs` job。附带消掉 `autoParseKey`（`:123`）与 in-flight Map（`:121`）以及 `references-panel.tsx:125` 的重复触发 |
| `maybeTriggerDeferredParse` | `resources.ts:58-83` | 调的已经是正确的轻量路径 `enqueuePaperPdfParse`（`:78`）。真实问题是它是 `PAPER.md` 的**唯一兜底**，却只在打开论文时触发、且被会话内 `paperParseTried` Set（`:56`）限制为一次 | **保留至第 7 项落地**。`ParseBody` 成为 Rust DAG 节点后整个删除，兜底由 `reconcile` 扫描承担 |
| `refreshAll` | `vault/actions.ts:241-249` | 5 个调用点（命令面板 `palette-commands.ts:70`、两个对话框 `app-dialogs.tsx:51,57`、`App.tsx:320,361`）。是用户显式的「刷新全部」Operation，非死代码 | **保留，改并发**：`await refreshTree` 后 `Promise.all([rebuildWikiAndNotify, refreshLibrary])`。现状三个串行 await 中间卡着全量 wiki 重建，论文库行迟迟不更新。另记：`App.tsx:320` 把它绑给名为 `refreshTree` 的 prop，易误读 |
| `enqueuePaperPdfParse` | `enqueue-paper-pdf-parse.ts:31` | 与 `enqueue-paper-layout.ts` 近乎复制（注释自称 "Mirrors the layout-analysis queue pattern"），各自维护 `queuedPapers` Set | **合并**：两者都退化为「向 JobCenter 提交一个 job」。这对复制品的存在本身就是缺统一抽象的证据 |
| `maybeAutoRunPaperReader` | `reader.ts:300` | Operation（跑 Agent 工作流、用户可见结果），但触发时机藏在 `downloadPaperAssetsAction` 尾部（`library-actions.ts:202`） | **保留函数，移出下载路径**。当前后果：手动点下载会顺带跑 Agent 精读，而打开论文触发的下载不会——同一设置项 `autoPaperReader` 表现不一致。应改为独立触发点 |
| `refreshTreeQuiet` | `vault/store.ts:212` | 与 `refreshTree` 的唯一差别是不设 `treeLoading/busy`、不弹 `notifyError`；仅 `workspace/actions.ts:312` 用一次 | **合并**为 `refreshTree(path, { quiet?: boolean })`。两个近同函数并存容易选错，`:180` 与 `:312` 就选了不同的 |
| `collectPapersNeedingAssetDownload` | `detect.ts:273` | 走前端 `FileNode` 树探测「缺 PDF/TeX」，与 Rust 侧 `has_local_pdf`/`has_local_tex` 是两套并行实现 | **删除**。`CapsCache`（§8.4）已经知道能力位，改为 Rust 查询。它还隐含要求调用方先 `refreshTree`——`import-actions.ts:169` 正是这么做的，属时序耦合 |

### 10.4 死代码

| 目标 | 位置 | 理由 |
|---|---|---|
| `graph_get_graph` / `getGraph` | `wiki/index.rs:850-1190`、`src/lib/wiki/api.ts:1026` | 零调用方；`GraphPanel` 渲染的是引用图 |
| `discover_paper_folders` / `collapse_graph_id` / `graph_node_from_id` / `paper_title_from_metadata` | `wiki/index.rs:981,1040,1105` | 仅被 `get_graph` 使用 |
| `paper_refs_graph` 的邻域 BFS 分支 | `refs/mod.rs:340-385` | `graph-panel.tsx:185` 不传 `center`，UI 不可达 |
| `looksLikeCitationMarker` / `matchCitationByMarker` | `src/lib/paper/refs.ts:167,192` | 仅测试引用；sidecar ↔ 正文标记的桥未接通 |
| `is_complex()` 的 buffer 克隆 | `pdf_parse/mod.rs:434-443` | 克隆整个 PDF 只为打 quality 标签，多一次完整 parse |

`paper_parse_body`（`catalog/commands.rs:147`）**不删**——前端 `enqueuePaperPdfParse` 与 CLI（`cli/src/commands/paper.rs:650`）都在用；清理的是它的**调用位置**（见 §10.1）。

## 11. 验收指标

| 指标 | 现状 | 目标 |
|---|---|---|
| 打开论文的阻塞 IPC 次数 | 4（串行） | 1 |
| 打开论文的 `setTabs` 次数 | 5 | 1–2 |
| 打开论文产生的磁盘写 | ≥1（`layout-index.json`） | 0 |
| 一次导入的目录遍历次数 | 5+ | 1 |
| 一次单篇下载触发的全量树重建 | 3–4 | 1（watcher 驱动） |
| 「下载后处理」重复代码处数 | 5（且互不一致） | 0（`depends_on` 表达） |
| layout sidecar 轮询 | 1.5s × 最长 15min | 0（事件驱动） |
| 翻译一篇 40 页论文的 sidecar 写次数 | 400+ | ≤ 页数（debounce 后） |
| catalog 命令的 `schema_version` 探测 | 每次调用 5 次 | 启动一次 |
| 批量导入 20 篇的并发 S2 请求 | 20 | ≤2 |

观测手段：沿用现有 `OpTimer`（见 `translate/commands.rs:10` 用法）与 `docs/backend/logging.md` 的日志约定，在 `paper_open_bundle` 与各 `JobKind` 上打点。

## 12. 未决问题

- `maybeAutoRunPaperReader`（`reader.ts:300`）移出下载路径后，`autoPaperReader` 设置项应挂在哪个触发点？候选：`DownloadAssets` job 完成事件、还是「打开论文且资产齐备」。前者更接近现状语义，但 Agent 工作流是 Operation 而非 Job，不宜由 JobCenter 直接拉起。
- `downloadAllMissingAssets` 删除后，文件树按钮的进度如何呈现？N 个 job 各占一行会淹没面板（§7.6 细节 1），需要一个「批次」概念把同批 job 聚合成一条——但这又给 JobCenter 引入了 batch 字段，需权衡是否值得。
- `refreshAll`（`vault/actions.ts:241`）与 `refreshTree` + `refreshLibrary` 的关系待查，可能是第三条并行的刷新路径。
- `MarksSnapshot` 塞进 `paper_open_bundle` 后，右侧栏的重复读盘（`right-sidebar.tsx:263-269`）如何复用同一份数据——需要一个 per-paper marks store 作为单一来源。
- `FiguresSidebar` 的 `setInterval(400ms)` handle 探测（`right-sidebar.tsx:154`）应换成 `registerPdfHandle` 的订阅回调，属独立小改动。
