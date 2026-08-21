# 工程实现审计与重构路线图（2026-08）

> 调研方式：6 个并行 agent 从「前端/Rust 重复代码、前端/Rust 耦合度、前端/Rust 质量与简洁性」六个维度全量扫描，所有结论均带 `file:line` 证据。
> 规模基线：前端 113,834 行 TS/TSX（610 文件）+ 后端 71,852 行 Rust。

## 总体结论

代码库纪律性**远高于平均水平**，问题不在"烂代码"，而在三方面：

1. **结构性冗余**：fork 未瘦身（ai-elements）、样板整段复制（jobs 四胞胎、HTTP client ×9）
2. **feature 间依赖方向无约束**：后端 60+ 条横向依赖边、9 组循环；前端存在 lib→components 反向依赖与跨域大环
3. **类型双写漂移**：TS/RS 类型手抄两遍，已产生实际 bug（见 [类型单源](#类型单源)）

卫生指标（无需行动）：

| 指标 | 数值 |
|---|---|
| clippy warning | **0**（无 lint 配置，真实干净） |
| `: any` / `as any` | 1 / 2 |
| `@ts-ignore` / `@ts-expect-error` | 0 |
| 空 `catch {}` | 0 |
| TODO / FIXME | 0 |
| 非测试路径 unwrap+expect | 26 处（无 `.lock().unwrap()`） |
| 跨 await 持锁 | 0 |
| components 层直接 `invoke(` | 0（IPC 全收口 lib 层） |

---

## 一、代码重复

### 1.1 前端

| # | 发现 | 位置 | 程度 |
|---|---|---|---|
| 1 | 错误提取三元 `e instanceof Error ? e.message : String(e)` | lib 层 **68 处**内联；`chat-state.ts:261` 已有导出版，`run-analysis.ts:92` 私有重写 | 完全复制 |
| 2 | PDF ask/translate run-hook 的「runOnce + 三事件监听」整段 | `use-pdf-ask-threads.ts:275-350` vs `use-pdf-selection-translate.ts:295-361` | ~85% 相同，**已漂移**（teardown 判等逻辑不一致） |
| 3 | 路径归一化手写变体 | `core/path.ts` 已有规范实现，但 **35+ 处**手写 `.replace(/\\/g,"/")`；大小写/trim 语义不一致（Windows #181 同款风险） | 完全复制 |
| 4 | 三个同构 useEffect 绕过 `useTauriEvent` | `use-agent-permission-surfaces.ts:102-162`、`use-paper-refs-sidecar.ts:44-60` | 完全复制，且有监听器泄漏隐患 |
| 5 | 拖拽高亮 hook | `use-composer-file-drag.ts` vs `use-library-pdf-drop.ts` | ~80% 相同 |
| 6 | Tauri broadcast/listen 样板 | 6 组 × ~18 行；`core/tauri-events.ts` 有 `listenSafe` 缺对称 `broadcastSafe` | 模式复制 |
| 7 | `VAULT_FILE_CHANGED` 事件匹配 | `use-pdf-layout-run.ts:327`、`use-pdf-marks-io.ts:196`、`use-pdf-layout-hover.ts:159` | 核心逻辑 3 份 |
| 8 | layout 解析三件套 `isObject/isFiniteNumber/parseBbox` | `io.ts:53`、`layout-index.ts:210`、`layout-translate.ts:259` | 一字不差 ×3 |
| 9 | bbox 几何微重复 | 交集六行样板 ×5、union ×3（clamp 语义不一） | 部分重叠 |
| 10 | reading-order 比较器 | 完整三级版 3 处 + 变体 4 处 | 部分重叠 |
| 11 | 手写 debounce ×8 | `papers-library.tsx:507`、`use-pdf-find.ts:28` 等，无共享 util | 同构 |

已排查无问题：各域 api.ts invoke wrapper（`core/ipc.ts` 统一信封）、Paper 类型集中、虚拟化/确认弹窗、debounce/日期工具泛化。

### 1.2 Rust 后端

| # | 发现 | 位置 | 程度 |
|---|---|---|---|
| 1 | HTTP client 构建器 ×9 | translate:312、feeds:118、refs/online.rs:76、zotero_io.rs:468、arxiv_proxy:36、mineru:411、paddle:253、layout_model:334、import/assets.rs:503；UA 字符串 6 种 | 完全复制 |
| 2 | **papers 40 列清单抄写 5+2 处** | `papers.rs:205/244/281/988/1307` + INSERT `:1144` + DDL `schema.rs:25` | **已产生实际 bug**：`find_by_identifier` 的 SELECT 漏 `zotero_item_id, zotero_last_synced` 两列，靠 `row.get(38).ok()` 静默吞掉，该 API 永远返回 None |
| 3 | vault 目录校验样板 ×57 | "vault path is not a directory" + 34 处 `PathBuf::from(args.vault_path.trim())`；`jobs/mod.rs:1265` 的 `validate_job_paper` 已是雏形但只被 jobs 用 | 完全复制 |
| 4 | **时间戳 3 种格式 ×29 处** | `to_rfc3339()` Secs 与 Millis 混用：`papers.rs:665` 用 Millis、`connector/import.rs:150` 用 Secs、`remote/commands.rs:555` 裸 rfc3339。`ORDER BY updated_at` 是字符串比较，`10:00:00Z` > `10:00:00.500Z` 为真 → **排序会错** | 不一致 bug |
| 5 | 原子写入 8 套实现 | wiki/rename:743、sync/engine:447、settings:521、sync/config:140、open_request:270、sync/local:43、catalog/sidecar:24、agent/registry:642（**非原子写**）；temp 命名/Windows 回退/0o600 三维度排列组合，仅一半处理 Windows rename 回退 | 结构相似 |
| 6 | jobs/mod.rs 四胞胎 | `enqueue_*` ×4（:375/424/474/523，各 47 行逐字同）+ `run_*_job` ×4（:718/766/841/911，骨架一致仅业务调用不同）+ command 五连发 ×5 | 完全复制 |
| 7 | SQLite open+PRAGMA+migrate 三连 | catalog/schema:111、usage/schema:89、feeds/schema:62，PRAGMA 批逐字符相同 | 完全复制 |
| 8 | arxiv_proxy vs site_proxy | modelscope 已复用 SiteProxy，唯 arxiv 独立实现（每次请求新建 client） | ~60% 重叠 |
| 9 | vault ignore 名单 ×5 + walker ×4 | watcher:142、wiki/index:23、vault/tree:20、sync/snapshot:32、sync/scheduler:157；"sync ⊇ watcher" 契约靠人工维持 | 部分重叠 |
| 10 | HTTP 错误 snippet 截断 ×8 | `text.chars().take(180)` + format | 完全复制 |
| 11 | terminal 平台分支三胞胎 | macOS osascript ×2、Linux 候选列表 ×2、Windows wt/cmd ×2 | 完全复制 |
| 12 | zotero_db 两条读取管线 | `read_sync_items:765` vs `read_items_conn:826`，同一条 SELECT 逐字符相同 | 结构相似 |
| 13 | `strip_version` ×3 + `bare_arxiv_id` | import/mod:1149、parse.rs:230（**行为已漂移**，缺 `arXiv:` 前缀剥离）、assets.rs:859、coolpapers:104 | 部分重叠 |

原子写入的隐式契约（重构时必须保真）：

- temp 文件必须 `.tmp` 结尾（`sync/snapshot.rs:33` 过滤依赖）
- wiki temp 命名含 `.agentero-rename-`（`watcher/mod.rs:207` 识别依赖）
- `doctor/mod.rs:956` 有故意不用 tmp+rename 的 FSEvents 例外

---

## 二、耦合度

### 2.1 Rust 后端：feature-first 声明与现实的差距

**符合度高**：185 个 command 全部归属 31 个 feature、`app/handlers.rs` 纯注册 201 行、21 个细粒度 managed state 无上帝 state、AppError/VaultFs 路径规范统一（`\\?\` 历史 bug 无复发）。

**符合度中低**：60+ 条横向依赖边、**9 组循环**，根因是 service 层依赖方向无任何约束（无层级声明、无 CI 检查）。

双中心：

- `catalog` 入度 **12**（import/remote/jobs/connector/vault/zotero_sync…）
- `import` 入度 **10** + 出度 **14**（上帝模块）

主要循环（严重度）：

| 环 | 证据 | 严重度 |
|---|---|---|
| import ↔ remote | `remote/import_bridge.rs:8-10` 直接抓 import 内部函数 `batch::preflight_identifier_batch` | 高 |
| import ↔ jobs | `jobs/mod.rs:784,929,934` 硬编码调用 import 业务；反向 import 入队 | 高 |
| refs ↔ jobs | `jobs/mod.rs:734` 调 `refs::parse_paper_refs`；反向 refs 引用 jobs ×7 | 高 |
| jobs ↔ settings | `jobs/mod.rs:956` 读 `AppSettingsStore`；反向 ×3 | 中 |
| settings → connector → translate → settings | 三环 | 中 |
| import ↔ layout_remote | 只为拿 `CANCELLED_MESSAGE` 常量建环 | 低 |

其他方向问题：

- `vault → wiki(3) + catalog(6) + import(1)`：基础设施层向上调用领域层
- `catalog → wiki(7)`：重命名事务主干在 wiki，paper 实体 CRUD 却要懂笔记索引，边界倒置
- `agent/background_tasks.rs`（561 行）是与 JobCenter 平行的**第二套任务系统**，import 里 12 处手动 `background_tasks::finish`
- 大文件结构问题：
  - `acp.rs`（3015 行）：`run_once` 单函数 **668 行 / 22 参数**，probe/connect/stream/persist 混杂
  - `jobs/mod.rs`（2042 行）：既是调度器又是业务执行体
  - `connector/state.rs`（1422 行）：内嵌 SaveSession 状态机（`AttachmentPhase`）被压平在 controller，应拆独立类型

### 2.2 前端：分层纪律好，但有高危违规

1. **lib 反向 import components**（3 处）：
   - `lib/workspace/actions.ts:670` 动态 import `pdf-viewer-registry`
   - `lib/markdown/export/run-export.ts:3` 直接 import React 组件 `MarkdownExportSurface`
   - `lib/plaza/sources.ts:14-15` import 图标组件
   - 造成 2 个跨 3 层的 madge 环（`workspace/actions → viewer/pdf-viewer → hooks → paper/import-actions`）
2. **4 域大环**：`agent→activity→vault→paper→vault(remote)→agent`，关键边 `agent/api.ts:538`、`activity/track.ts:11`、`vault/store.ts:11`、`remote-vault.ts:309`（type-only），靠动态 import 侥幸未成运行时环
3. 中等问题：
   - `core→shell`：`lib/core/file-accept.ts:6` import `shell/vault-file-drag`（底层依赖高层；vault-file-drag 仅 37 行零依赖，移到 core 即可）
   - `settings↔pdf/translate`：settings 硬编码各域 schema，上帝配置倾向早期形态
   - `vault↔wiki` 双向纠缠：rename 编排（搬文件/修双链）天然双向，靠互相调用实现
   - `agent↔pdf`：视觉上下文类型两侧各持一份值级边（`newTraceId`、`nextLineId`），`PdfVisualNormalizedRect` 被 17 处深 import
   - `PaperTag` 类型住在 `lib/ui/tag-colors.ts`（paper 领域类型放 ui 域，方向反了）

健康面（无需动）：components 零直接 invoke、组件横向依赖走公共 index、11 个 zustand store 无上帝 store、lifecycle 总线 scope 化副作用是教科书式做法。

---

## 三、质量与简洁性

### 3.1 前端

死代码与 fork 死重（最大精简空间）：

- **`ai-elements/prompt-input.tsx`（1509 行）**：Vercel AI Elements vendored fork，45 个导出中 **32 个全库 0 使用**（Command/Select/Tabs/HoverCard/ActionMenu/Provider 整族 + 75 行 `captureScreenshot`）。做 fork 瘦身可降至 ~800 行
- lib 全域 1871 个具名导出中 **347 个（18.5%）over-export**；抽查确认 **18 个完全死函数**（`wiki/api.ts` 的 `extractWikilinks`/`resolveDemoWikiReference` 等 4 个、`pdf/highlight/io.ts:22 createHighlight`、`shell/feature-window.ts:119 closeFeatureWindow` 等），合计 ~290 行确认死代码，全量清理可触碰 ~2000 行

巨型组件拆分：

- `pdf-viewer.tsx`（1538 行）：hook 化已好（24 hooks、2 effect、0 useState），剩 ① 6 个"延迟绑定 ref"的隐式事件总线（:521-546，应抽 `useCardChromeBus`）② 75 行内联 `pinsByPage` 纯函数（:749-824）③ 3 个无意义 useCallback 转发（:1172-1184）
- `papers-library.tsx`（1239 行）：单组件 770 行，可拆 `useReadingHeatmaps`（heatmap 五字段缓存 + 2 effect）、`LibraryTableHeader`（215 行 JSX）、`useDeferredCellCopy`
- `agent-pane.tsx`（1481 行）：实为 2 个组件（`AgentPane` 17 useState + `RemoteAgentPane`），重复了 scan/probe 生命周期逻辑

useEffect：312 个总体质量高（27 处 biome-ignore 均附理由），仅 2 处可派生 state 用了 effect（`workspace-host.tsx:154-198` LRU 连锁、`papers-library.tsx:763` 标签过滤修正）。

### 3.2 Rust

- **AppError 类型退化（最大结构问题）**：931 处 `AppError::message(String)` vs 类型化变体显式使用 ~5 处——`error.rs` 的 7 个变体形同虚设，实为 anyhow 风格转字符串，错误不可编程判断
- `.ok()` 丢弃错误 150 处（多为 `lock().ok()` 静默吞中毒）；`let _ =` 378 处（多为 fire-and-forget emit，可接受）
- 锁中毒风格不一致：`catalog/caps.rs:73-97` 用 `.expect("poisoned")`，`settings/mod.rs:376` 用 `map_err`；`acp.rs` 7 处 `if let Ok(g)` 静默吞中毒
- clone/to_string 中等：acp.rs `.clone()` 181 处（多为 DTO 映射本质），真正可省的是 5 处重复 `title: x.clone().unwrap_or_else(|| id.clone())`
- 真死代码仅 3 个 helper（`usage/schema.rs:167 paper_path_of`、`cli_install/download.rs:311 write_all`、`cli_install/mod.rs:581 user_bin_candidates`）
- 超长函数：`run_once` 668 行/22 参数、`save_items`（connector/server.rs:241）208 行、`import_one_local_pdf` 195 行、`elicitation_fields_from_request`（acp.rs:832）三臂同构 128 行

---

## 四、类型单源（TS/RS 双写问题）

### 现状

类型**确实写了两遍**，且项目里 tauri-specta **已以 PoC 形式接入**：

- PoC：`features/translate/commands.rs:73-112` 的 `export_typescript_bindings` 测试——`translate_text` 走 tauri-specta 生成 `src/lib/core/bindings.ts`（47 行），默认 verify-only（`cargo test` 只校验不覆写），`AGENTERO_UPDATE_BINDINGS=1 cargo test -p agentero export_typescript_bindings` 才重新生成。这个模式已是 CI 级质量，可直接推广
- Rust 侧：~136 个 DTO struct（Args/Result/Snapshot），snake_case 序列化，**仅 translate 相关 derive 了 Specta**
- TS 侧手写镜像：`paper/types.ts` 的 `PaperMetadata` ≡ `catalog/papers.rs:99 PaperRecord` 手抄；settings/agent/cli/sync/wiki 各域 api.ts 内联类型；事件 payload（`core/job-center.ts:35 JobOfferPayload`、`vault/fs-watch.ts:7 VaultFileChangedPayload`）手写，17 个文件用 `listen<手写类型>`
- §1.2 的 `find_by_identifier` 丢列 bug 正是双写漂移的实例——类型单源可消灭这一整类问题

### 迁移障碍

1. 136 个 struct 加 `#[derive(specta::Type)]`——机械但面大，按域分批
2. 部分 TS 类型比 Rust 更严：TS `creators?: PaperCreator[]` vs Rust `Option<serde_json::Value>` → 生成后降级为 `any`；需补真 Rust 类型（长期收益）或接受局部降级
3. `invokeApi` 封装语义（`fallback`/`desktopOnly`/`allowVoid` + ApiResult 信封解析）不能丢：薄包生成的 `commands.xxx` 而非直接替换，调用点基本不动
4. 事件侧需迁到 tauri-specta 的 `Events` trait 才能同步生成
5. 版本为 `2.0.0-rc.25`（RC 可能变 API），已锁定在 Cargo.toml，风险可控

### 迁移路径

1. **Phase 1（低风险）**：只生成**共享类型**不换命令——各域 `types.ts` 改为从 bindings re-export（`export type PaperMetadata = PaperRecord`），调用点零改动
2. **Phase 2**：按域把命令注册进 Builder，`invokeApi` 包住生成的函数
3. **Phase 3**：事件迁移（job-center / fs-watch 先行，payload 最稳定）
4. 沿用 verify-only 同步测试，每迁一批就把该域 struct 加进 Builder，漂移即刻被 `cargo test` 拦截

---

## 五、重构路线图（按优先级）

### P0 — 顺带修 bug，立即做（半天级，零风险）

1. `PAPER_COLUMNS` 常量收敛 5 处 SELECT 清单 → **修复 `find_by_identifier` 静默丢列 bug**
2. `now_rfc3339_millis()` 单一入口 → **修复 `updated_at` 字符串排序混排 bug**
3. `resolve_vault()` / `resolve_paper_dir()` 替换 57+34 处样板
4. `errorText` 下沉 `lib/core` 替换 68 处内联；`broadcastSafe` 补进 `core/tauri-events.ts`
5. `use-agent-permission-surfaces` 改用现成 `useTauriEvent`（顺带修监听器泄漏）
6. 删确认死代码：prompt-input 32 个未用导出（~700 行）、lib 18 个死函数、Rust 3 个 helper

### P1 — 消样板，低风险（1-2 周）

7. jobs/mod.rs `enqueue_*`/`run_*_job` 泛型化（净删 ~300 行，新 JobKind 从 47 行变 5 行）
8. HTTP client 工厂 + `http_err`/snippet 共享（9 处构建器 + 8 处 snippet）
9. SQLite ensure/migrate/schema_meta 工具包（3 份逐字相同）；`strip_version` 等 arXiv 工具收敛
10. 手写 debounce ×8 → `useDebouncedValue`；PDF run-hook 三连监听提取 `attachAgentRun`（消除已漂移双副本）
11. 类型单源 Phase 1（bindings re-export 共享类型）

### P2 — 结构性重构（各 2-3 天，需设计投入）

12. **jobs 去业务化**：执行体改 runner 注册制（`JobCenter::register(kind, fn)`）→ 一举解开 jobs↔{import, refs, settings, agent} 4 个环
13. **import 上帝模块拆分**（出度 14→6）：`pdf_parse/`、`zotero_*` 提升为顶层 feature（zotero_sync 已是先例）；`background_tasks` 并入 JobCenter 或走 lifecycle 事件
14. remote↔import 边界重划：import_bridge 只依赖 import 稳定 pub API
15. 前端断开 lib→components 反向依赖：viewer-registry 下沉 `lib/workspace/viewer`、export surface 拆分、plaza 图标上移
16. `acp.rs run_once` 668 行按 probe→connect→stream→finalize 拆分 + 22 参数收敛为 struct；elicitation 三臂提取 helper
17. 原子写入统一到 `core/fs::json_store`（保真 `.tmp` 后缀、`.agentero-rename-` 命名、doctor FSEvents 例外、0o600；registry 顺势升级为原子写）
18. settings 泛化为 schema 无关配置层；vault↔wiki rename 编排上提；agent↔pdf 共享视觉类型下沉
19. 类型单源 Phase 2/3（命令 + 事件按域注册）

### P3 — 防回潮机制

20. 后端声明 feature 分层（L0 叶子：network/translate/terminal/trash/layout_*；L1 存储：catalog/wiki/refs；L2 工作流：import/jobs/remote/sync；L3 端点：connector/bridge/zotero_sync/feeds）+ CI 依赖检查脚本禁止跨层/逆向引用
21. 前端 madge --cyclic 进 CI
22. AppError 类型化渐进改造（热点 catalog/wiki 先做 Io/Sqlite `?` 自动转换，或坦然裁剪 error.rs 到实际使用的 3 变体）

### 明确不动

- `core/fs` 的 Local/Sftp 双实现（架构设计）
- `bridge/` 与 `connector/` 的协议处理（领域差异大于共性）
- acp.rs 的 Replay/Stream 部分（复杂度本质）
- pdf-viewer 的 3 个 slice useMemo（有意的引用稳定，服务于 memo 的 PdfPageLayers）

## 估算总账

- 直接可删 ~1300 行；样板收敛后净减 ~2500-3000 行前端 + ~600 行后端
- 修复 2 个已潜伏 bug（丢列、排序混排）+ 1 个监听器泄漏
- 解开 9 组后端依赖环、2 个前端跨层 madge 环
- 类型单源消除一整类双写漂移 bug
