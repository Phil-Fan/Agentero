# 导入与打开论文的行为 DAG

> 状态：现状分析 + 目标设计。所有事实均直接核实自源码（见每条的 `file:line`）。
> 姊妹篇：[流水线编排](paper-pipeline-orchestration.md)（时间轴、Job Center、依赖选型）

## 0. 勘误

本文修正 [paper-pipeline-orchestration.md](paper-pipeline-orchestration.md) 中三条错误结论：

| 错误说法 | 实际 |
|---|---|
| `paper_commit` 内部调 `merge_liteparse` 生成 `PAPER.md` | **`merge_liteparse` 这个函数不存在**。真实函数是 `maybe_generate_paper_md_after_download`（`pdf_parse/mod.rs:104,117`），而 `paper_commit`（`paper_import/mod.rs:94-204`）**从不调用它** |
| `maybeTriggerDeferredParse` 调重量级 `paper_download_assets` | 它调的是 `enqueuePaperPdfParse`（`resources.ts:78`），已经是正确的轻量路径 |
| 下载路径上的 `enqueuePaperPdfParse` 是「保证空转」，应删除 | **恰好相反**：它是 identifier 导入路径上**唯一**生成 `PAPER.md` 的环节，删了就再也不会生成 |

结论反转的根因见 §3.1。

## 1. 一句话

导入与打开这两条链目前**跨 Rust / 渲染进程被切成碎片**，依赖关系靠「调用点的书写顺序」隐式表达；同一件事（生成 `PAPER.md`）在三个入口有三种实现，其中两个入口把责任交给了前端。目标是把依赖显式化为一张 DAG，节点由 Rust 统一持有。

## 2. 现状行为 DAG

### 2.1 导入路径 A：identifier（魔棒 / arXiv / DOI / URL）

```text
[前端] lookupSubmit (import-actions.ts:85, kind:"lookup")
  │
  └→ invoke lookup_import_batch (import/commands.rs:20)
      │
      ├→ 批次 preflight：dedupe / skill 拆分 (import/mod.rs:310)
      │
      ├→ buffer_unordered(5)  每篇并发 (import/mod.rs:356-382)
      │   │
      │   ├→ resolve_metadata (Translator) → enrich_remote_urls
      │   │
      │   └→ paper_commit (paper_import/mod.rs:94)
      │       ├ normalize_parent → id 检查 → dedupe 早退 (:107-136)
      │       ├ allocate_paper_path → mkdir (:138,145)
      │       ├ write_paper_shell_opts → NOTES.md（标题 + 摘要机翻）(:153)
      │       ├ papers::upsert_paper  ← catalog 为事实来源 (:157)
      │       ├ ensure_paper_assets_with_progress (:161)
      │       │    ├ has_local_pdf / has_local_tex          ← 遍历 ×2 (assets.rs:191,192)
      │       │    ├ PDF：候选链 → Crossref → Unpaywall     (assets.rs:194-258)
      │       │    └ TeX：仅 arxiv_id，e-print → tar 解压    (assets.rs:265-286)
      │       │         ⚠ PDF 与 TeX 严格串行
      │       │    └ has_local_pdf / has_local_tex 复查      ← 遍历 ×2 (assets.rs:289,292)
      │       └ spawn_parse_after_import (refs)             ← refs 第 1 次 (:190)
      │
      └→ for r in imported: spawn_parse_after_import        ← refs 第 2 次 (import/mod.rs:391)
  │
[前端] 回到 import-actions.ts
  ├→ for imported:
  │    ├ enqueuePaperLayoutAnalysis  (:146)
  │    └ enqueuePaperPdfParse        (:153)  ← ★ 唯一生成 PAPER.md 的环节
  ├→ refreshTree → collectPapersNeedingAssetDownload (:169)
  └→ for needing: enqueueBackgroundTask(download) (:182)
       └ downloadPaperAssets → refreshTree → refreshLibrary
         ├ enqueuePaperLayoutAnalysis (:197)
         ├ enqueuePaperPdfParse       (:200)
         └ [Rust 内] spawn_parse_after_import ← refs 第 3 次 (import/mod.rs:520)
```

**`PAPER.md` 不由 Rust 生成**。`paper_commit` 与 `download_paper_assets_with_progress`（`import/mod.rs:458-522`）都只做「资产 + refs spawn」，我逐行读完确认无 liteparse 调用。

### 2.2 导入路径 B：Connector（Zotero 扩展）

```text
connector/import.rs
  ├ ensure_paper_assets(...)
  └ maybe_generate_paper_md_after_download(...)   ← PAPER.md 在 Rust 内生成
       await 形式：:174, :887, :925
       spawn 形式：:356, :733, :797
       另有 connector/state.rs:536（spawn）
```

**与路径 A 完全不同**：这里 Rust 自己负责 `PAPER.md`，不依赖前端。

### 2.3 导入路径 C / D：本地 PDF、BibTeX 库导入

| 路径 | 命令 | `PAPER.md` 由谁生成 |
|---|---|---|
| 本地 PDF | `paper_import_local_pdf` → `paper_commit(AssetsPolicy::CopyPdf)` | 前端 `import-actions.ts:322` |
| BibTeX 库导入 | `importLibraryFromFile` | 前端 `library-actions.ts:141` |
| Zotero 迁移 | — | 前端 `zotero-migrate-dialog.tsx:333` |

### 2.4 打开论文路径

```text
openPaper (workspace/actions.ts:522) → openTab (:192)
  ├ tab 已存在 → activatePaperWithNotes (:109) → return
  └ 新建：
      insertPlaceholderTab (:219) → setTabs + setActiveTabId + dock.openPanel
        └ DocView 渲染骨架屏
      async IIFE → loadTabResources (:243 → resources.ts:167)
        │  ⚠ 以下全部串行
        ├ ensureLocalFsScope → invoke vault_allow_fs_scope
        ├ paperDirFromPath / detectPaperDirectory（可能读 NOTES.md）
        ├ loadPaperMetadata → invoke paper_get
        ├ resolvePaperPdfSource (resources.ts:89)
        │   ├ findLocalPdfPath → readDir 递归 4 层 (media.ts:120-210)
        │   ├ localFileToArrayBuffer → readFile 全量 + Uint8Array 拷贝
        │   └ 缺 PDF 时：enqueueBackgroundTask(download)
        │        └ downloadPaperAssets → enqueuePaperLayoutAnalysis (:133)
        ├ maybeTriggerDeferredParse (:58) → enqueuePaperPdfParse (:78)
        ├ loadPaperRefsAuto（fire & forget）
        └ readVaultFile(NOTES.md)
      updateTab (:271)
      createNotesSplitPane (:291) → setTabs → openPanel → activatePanel
      didDownload → refreshTreeQuiet (:312)
  │
  ├ Dockview onDidActivePanelChange → handleActivePanelChange (:132)
  ├ WorkspaceHost effects：setPdfLru / evictPdfBuffers / setTreeSelectedPath
  │   / hydratePlaceholderTabs (:863 → 第二条 loadTabResources :876 → updateTab :899)
  │   / savePersistedTabs
  └ DocView → PdfViewer → EmbedPDF → PDFium parse → totalPages
        ├ outline（getBookmarks）
        ├ 阅读位恢复（readReadingPage → scrollToPage）
        ├ annotations import（可能迁移 + 回写）
        ├ marks：3 次 readDir + N 次 readVaultFile
        ├ pageTextRects（视口 ±2 页）
        ├ enqueuePaperLayoutAnalysis (use-pdf-layout-run.ts:278) ← 每次打开都 enqueue
        └ readLayoutSidecar 轮询 1.5s × 最长 15min
```

## 3. 不合理之处

### 3.1 P0 — `PAPER.md` 的生成责任落在前端

同一件事有三种实现：

| 入口 | 实现 | 后果 |
|---|---|---|
| Connector | Rust 内联 `maybe_generate_paper_md_after_download` | 可靠 |
| identifier / 本地 PDF / BibTeX | 前端 `enqueuePaperPdfParse` → IPC `paper_parse_body` | **前端不调就永远没有** |
| CLI | 直接 `paper_parse_body` | 可靠 |

具体风险：

- 前端在 `lookup_import_batch` 返回后才逐篇 enqueue（`import-actions.ts:153`）。此时用户若切走、刷新、或应用崩溃，这批论文的 `PAPER.md` 永久缺失。
- 唯一的兜底是下次打开该论文时 `maybeTriggerDeferredParse`（`resources.ts:58`）—— 但它有 `paperParseTried` 会话内 Set（`:56`）与三个前置守卫，且只在**打开**时触发。从未被打开的论文永远没有 `PAPER.md`。
- `PAPER.md` 是 Agent 精读、全文搜索的输入。缺失是静默的，没有任何 UI 提示。

**修法**：`ParseBody` 成为 Rust 侧 DAG 的一个节点，由 `CommitPaper` 无条件挂上。Connector 的 7 处内联调用改为 enqueue 同一个 job（顺带解决它 await / spawn 两种写法混用的问题）。前端的 5 处 `enqueuePaperPdfParse` 全部删除。

### 3.2 P0 — refs 解析每篇 spawn 2–3 次且首轮并发打架

三个 spawn 点：`paper_commit:190`、`import/mod.rs:391`（批次循环）、`download_paper_assets:520`。

`refs/mod.rs:138` 的守卫是 `schema_version == SCHEMA_VERSION && source.fingerprint == fp` → 早退。但**守卫在算出指纹之后**，所以每次空转仍要付：

1. `papers::get_by_path` 读 catalog 行
2. `collect_ref_files` 递归遍历（深度 ≤16）收集 `.bib/.bbl/.tex/.ltx`
3. `stat` 每个文件算 SHA-256 指纹

批量导入 20 篇 = 40–60 次目录递归。

更严重的是**首轮**：sidecar 尚不存在时守卫不生效，而三个 spawn 无 in-flight 去重（`spawn_parse_after_import` 是裸 `tokio::spawn`），于是同一篇论文可能有 2–3 个任务同时判定「需要解析」→ 同时发起 Semantic Scholar 请求。20 篇 × 2–3 = 40–60 个并发请求，且每次新建 `reqwest::Client`。**429 是必然的。**

**修法**：`ParseRefs` 作为 DAG 节点，靠 `HashSet<(kind, paper, fingerprint)>` 天然单例。删除三个 spawn 点。

### 3.3 P1 — PDF 与 TeX 串行，把 layout 拖在最后

`assets.rs:194`（PDF）与 `:265`（TeX）严格顺序执行，但它们是指向不同 URL 的独立请求。而 `LayoutAnalyze` **只需要 PDF 文件**，却因为前端在整个 `downloadPaperAssets` 返回后才 enqueue（`import-actions.ts:197`、`library-actions.ts:182`），被迫等 TeX。

以示意时长计：

| 阶段 | 现状（串行） | 目标（并行） |
|---|---|---|
| PDF | 0→2s | 0→2s |
| TeX | 2→3.5s | 0→1.5s |
| ParseBody | 3.5→11.5s | 2→10s |
| LayoutAnalyze | 11.5→41.5s | **2→32s** |
| **总墙钟** | **41.5s** | **32s** |

⚠️ 并行化会破坏 `mapDownloadProgress` 的 `pdf=0–50% / tex=50–100%` 假设（`background-tasks.ts:91-105`），需改为按合并字节总数计算。

### 3.4 P1 — 能力位探测在一次导入中重复 10+ 次

| 位置 | 次数 |
|---|---|
| `ensure_paper_assets`：`:191,192` 前置 + `:289,292` 复查 | 4 |
| `paper_commit`：dedupe 分支 `:127-129`、结果构造经 assets | 2 |
| `parse_paper_body_inner`：`:157` has_local_tex、`:162` has_paper_md、`:168` has_local_pdf、`:173` find_local_pdf | 4 |
| 前端 `collectPapersNeedingAssetDownload`（`detect.ts:273`）走 FileNode 树 | 1（另一套实现） |

**修法**：`CapsCache`（见姊妹篇 §8.4）作为唯一探测入口。

### 3.5 P1 — 打开路径全串行，且有两条 `loadTabResources`

`resources.ts:167` 内 `paper_get` / PDF 发现+读取 / `NOTES.md` 三者互不依赖却串行等待。另外 `hydratePlaceholderTabs`（`actions.ts:863`）是第二条独立的 `loadTabResources` 调用路径（`:876`），两条路径的后续 `updateTab` patch 不完全一致（`:271` vs `:899`），是行为分叉的隐患。

### 3.6 P2 — 三处轮询

| 位置 | 间隔 | 应改为 |
|---|---|---|
| `use-pdf-layout-run.ts` 读 `layout.json` | 1.5s × 15min | `job:changed` 事件 |
| `right-sidebar.tsx:154` 探 PDF handle | 400ms | registry 加订阅（`pdf-viewer-registry.ts:14` 目前是裸 `Map`） |
| `workspace/actions.ts:576` "Retry until the PDF handle is registered" | 重试循环 | 同上 |

### 3.7 P2 — 刷新风暴

`enqueue-paper-pdf-parse.ts:67-68` 在任务体内联 `refreshTree` + `refreshLibrary`，而调用方（`library-actions.ts:180-181`、`import-actions.ts:195-196`）**自己也刷**。一次单篇下载可触发 3–4 次全量 `vault_tree_build` + `paper_list`。

刷新应由 watcher 或 `job:changed` 驱动，不由任务体内联触发。

### 3.8 P2 — 依赖靠书写顺序隐式表达

现在「TeX 下载完才判断要不要跑 liteparse」这条依赖，是靠 `ensure_paper_assets` 内部的语句顺序 + `parse_paper_body_inner:157` 的守卫共同保证的。没有任何地方声明这条依赖，改动顺序就会静默破坏它。

## 4. 目标 DAG

### 4.1 导入

```text
metadata（Translator 解析完成）
    │
    └→ CommitPaper (Rust 同步：mkdir + NOTES.md + upsert_paper)
         │
         ├──→ DownloadPdf ──┬──→ LayoutAnalyze ──→ LayoutTranslate（仅用户触发）
         │                  ├──→ PageCount
         │                  └──┐
         │                     ├──→ ParseBody   [AllSettled]
         └──→ DownloadTex ──┬──┘
                            └──→ ParseRefs      [AllSettled]
```

依据：

- `LayoutAnalyze` / `PageCount` 只需 PDF 文件落盘
- `ParseBody` 需要 PDF **和** TeX 都落定——因为「有本地 TeX 就跳过 liteparse」是它的第一条守卫（`pdf_parse/mod.rs:157`）
- `ParseRefs` 的本地源 `.bib/.bbl` 在 `source/`，但无 TeX 时仍可跑（纯在线分支）

### 4.2 两处对姊妹篇设计的修正

**① `depends_on` 必须是 `Vec<JobId>`，不是 `Option<JobId>`** —— `ParseBody` 有两个前置。

**② 依赖语义必须是「settled」而非「succeeded」**：

```rust
enum DepPolicy { AllSettled, AllSucceeded }
```

`ParseBody` / `ParseRefs` 用 `AllSettled`。因为**大多数论文没有 arXiv TeX**，`DownloadTex` 失败是常态（现状 `assets.rs:280` 就把 `tex failed` 当非致命消息处理）。若用 `AllSucceeded`，非 arXiv 论文将永远不会生成 `PAPER.md`。

### 4.3 打开

```text
T0  paper_open_bundle（1 个 IPC）──→ readFile(PDF) ──→ 骨架屏消失
      └→ job_focus_paper（fire & forget）
           ├ 把该论文已排队的 job 移到 focus lane
           └ 仅对 CapsCache 显示缺失的产物补 enqueue
T1  focus lane 执行：ParseRefs / ParseBody / LayoutAnalyze
      └→ job:changed 事件 → 局部刷新（无轮询）
T2  idle lane：reconcile 扫描补全库缺失
```

打开路径**不再产生新的 enqueue 点**——`use-pdf-layout-run.ts:278`、`resources.ts:78`、`resources.ts:133` 全部删除。

## 5. 修改方案对照

| # | 问题 | 修法 | 依赖 |
|---|---|---|---|
| 1 | §3.7 刷新风暴 | 删 `enqueue-paper-pdf-parse.ts:67-68` 内联刷新 | 无 |
| 2 | §3.6 handle 轮询 ×2 | `pdf-viewer-registry.ts` 加 listener set（约 15 行）+ `useSyncExternalStore` | 无 |
| 3 | §3.3 进度映射 | `mapDownloadProgress` 改按合并字节数 | 需与 5 同批 |
| 4 | §3.4 能力位重复 | `CapsCache` 唯一入口，删 `collectPapersNeedingAssetDownload` | 无 |
| 5 | §3.3 PDF/TeX 串行 | `ensure_paper_assets` 内 `tokio::join!` 两个下载 | 3 |
| 6 | §3.5 打开路径串行 | `paper_open_bundle` 聚合命令；合并两条 `loadTabResources` | 4 |
| 7 | §3.1 `PAPER.md` 责任 | `ParseBody` 成为 DAG 节点；删前端 5 处 `enqueuePaperPdfParse`；Connector 7 处内联改 enqueue | 8 |
| 8 | §3.2 refs 重复 spawn | `ParseRefs` 成为 DAG 节点；删 3 处 `spawn_parse_after_import`；加 `governor` 限流 | 8 |
| 9 | §3.8 隐式依赖 | JobCenter + `depends_on: Vec` + `DepPolicy` | — |
| 10 | §3.6 layout 轮询 | `job:changed` 事件 | 9 |

1–6 不依赖 JobCenter，可独立落地。7、8、10 依赖 9。

## 6. 验收

| 指标 | 现状 | 目标 |
|---|---|---|
| identifier 导入后 `PAPER.md` 生成成功率 | 依赖前端存活 | 100%（Rust 持有） |
| `PAPER.md` 生成实现数 | 3 种（Rust 内联 await / Rust spawn / 前端 IPC） | 1 |
| 每篇 refs 解析 spawn 次数 | 2–3 | 1 |
| 批量导入 20 篇的并发 S2 请求 | 40–60 | ≤2 |
| 一次导入的能力位探测次数 | 10+ | 1 |
| 单篇导入墙钟（示意） | 41.5s | 32s |
| 一次单篇下载的全量树重建 | 3–4 | 1 |
| 轮询循环数 | 3 | 0 |

## 7. 待确认

- `AssetsPolicy::Deferred`（`paper_import/mod.rs:185`）产出 `assets_pending: true`，但谁消费这个标志、何时补下载，尚未追踪完整。
- Connector 的 7 处 `maybe_generate_paper_md_after_download` 中 await / spawn 两种写法的选择依据不明，改为 job 前需确认是否有同步返回 `PAPER.md` 的调用方契约。
- 远程 SFTP 导入（`remote/import_bridge.rs`）的 DAG 与本地是否一致——它把资产暂存到 `work_root` 再 `upload_tree`，`ParseBody` 应在暂存目录还是上传后执行需要定夺。
