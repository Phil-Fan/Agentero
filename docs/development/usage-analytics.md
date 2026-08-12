# 使用记录与习惯总结（Usage Analytics）

> 状态：设计草案。关联 [\#239](https://github.com/poco-ai/Agentero/issues/239)。
> 相关：[`../backend/catalog.md`](../backend/catalog.md)、[`../backend/agent.md`](../backend/agent.md)、[`../backend/logging.md`](../backend/logging.md)、[`../frontend/library.md`](../frontend/library.md)、[`plaza.md`](plaza.md)

## 1. 目标与非目标

Issue 三条诉求拆成三层：

| # | 诉求 | 本文对应 |
|---|---|---|
| 1 | 记录浏览 / 下载 / 阅读记录 | §3 事件模型 + §4 存储 |
| 2 | 让 Agent 拿到用户操作习惯 | §5 UsageProfile + §6 Agent 三层接入 |
| 3 | 基于额外 context 做总结、推荐 | §7 上层功能 |

**目标**

- 本地记录**无法从产物反推**的行为事件，形成可查询的使用画像。
- 让 BYOA Agent 以「被动注入 + 主动查询」两种方式获得习惯 context。
- 在画像之上提供**继续阅读 / 周回顾 / 本地推荐**三个可验证功能。

**非目标（首版）**

- 不做 embedding / 向量库 / 语义相似度（推荐用图 + 标签 + 时长，见 §7.3）。
- 不上传任何行为数据。
- 不跨设备同步使用记录（习惯是设备本地事实，见 §4.1）。
- 不重复记录已能从 Vault 产物派生的信号（见 §3.1）。

## 2. 现状盘点

| 能力 | 现状 | 复用方式 |
|---|---|---|
| Catalog SQLite | `features/catalog/schema.rs`：`SCHEMA_VERSION` + `MIGRATE_Vn_TO_Vn+1` + `schema_meta` | **复用迁移范式**，不复用同一个库（§4.1） |
| `papers` 表 | 有 `is_read` / `added_at` / `updated_at`；**无** `opened_at` / 访问计数 | `ATTACH` 后 join 取 title/tags |
| 结构化日志 | `core/log_util.rs` 的 `OpTimer` → `op start\|end <name> k=v` | 沿用字段风格，便于排查；日志不作为数据源 |
| 阅读产物派生 | `src/lib/paper/reading-heatmap/`：从高亮/提问/翻译派生热力图 | **先例**：能派生的不落库（§3.1） |
| 已有 recents | `agent/mention.ts` 的 `pushRecentMentionPath`、`vault/session.ts` 的 `rememberRecentVault`、`pdf/reading-position.ts` | 后续可由 usage 库统一供数，首版并存 |
| 双链图 | `features/wiki/`（`graph_get_graph` / `graph_get_backlinks`） | 推荐候选集来源 |
| 搜索 | `features/search`：walk 式无索引 | 只在此埋 `search.query` 事件 |
| 相似度 / 推荐 | **不存在任何实现** | 从零起，见 §7.3 |

## 3. 事件模型

### 3.1 原则：只记录不可反推的行为

Vault 产物本身已经是最好的行为记录。高亮、批注、翻译、`is_read`、PDF 阅读页码都已落盘，`reading-heatmap` 已证明可从产物派生统计。**重复落一份事件只会引入不一致。**

| 信号 | 来源 | 是否记事件 |
|---|---|---|
| 高亮 / 批注 / 划词提问 / 翻译 | `marks/*.json`、NOTES | ❌ 派生（`reading-heatmap`） |
| 是否已读 | `papers.is_read` | ❌ 直接查 catalog |
| PDF 阅读进度 | `pdf/reading-position.ts` | ❌ 直接读 |
| 打开了什么 / 何时 / 停留多久 | 无痕迹 | ✅ |
| 下载了哪些附件 | 只留文件，无时间线 | ✅ |
| 搜过什么词 | 无痕迹 | ✅ |
| 调用了哪个 Agent workflow | 无痕迹 | ✅ |

### 3.2 事件表

| `kind` | 载荷 | 埋点位置 |
|---|---|---|
| `paper.open` | `path`, `mode`(pdf/html) | `src/lib/workspace/actions.ts` `openTab`（唯一漏斗，会分类 `kind`/`mode`） |
| `note.open` | `path` | 同上 |
| `paper.focus` / `paper.blur` | `path`, `dur_ms` | `actions.ts` `handleActivePanelChange`、`closeTab` |
| `asset.download` | `path`, `asset`(pdf/tex/…) | `src/lib/paper/library-actions.ts` `downloadPaperAssetsAction` / `downloadAllMissingAssets` |
| `paper.import` | `path`, `source`(arxiv/doi/url/zotero) | 入库 action |
| `search.query` | `q`, `hits` | `src/lib/vault/search.ts` |
| `agent.run` | `workflow`, `path?` | `src/lib/agent/api.ts` `runOnce` |

**停留时长**用 focus/blur 配对而非 `open`→`close` 差值：Dockview 多面板下同时打开多篇是常态，只有获得焦点的面板才计时。单次 focus 段设上限（如 30min）以吸收「开着页面去吃饭」。

### 3.3 上报路径

前端**缓冲批量上报**，避免每次点击一次 IPC：

```
埋点 → usage-buffer（内存数组）
      → flush 触发：5s 定时 / window blur / beforeunload / 满 50 条
      → usage_record_events(events[])   单条命令，一次事务
```

同一 `(kind, path)` 在 1s 内去重（Dockview 重挂载会重复触发 open）。

## 4. 存储

### 4.1 为什么另起 `.agentero/usage.sqlite`

不放进 `catalog.sqlite`，两个理由：

1. **远程 Vault 会整文件镜像 catalog**（`features/remote/catalog_mirror.rs`，`CATALOG_REL = ".agentero/catalog.sqlite"`）。高频行为写入会把镜像推送变成持续抖动，甚至让「谁的 catalog 更新」判定失真。
2. **使用记录是设备本地事实**。同一 Vault 在台式机和 iPad 上的阅读节奏不该互相覆盖；合并两台设备的 focus 段也没有语义。

代价是跨库 join，用 `ATTACH catalog.sqlite AS cat` 解决（同目录、同进程，成本可忽略）。

### 4.2 Schema

新建 `src-tauri/src/features/usage/{mod,schema,events,profile,commands}.rs`，`schema.rs` 复用 catalog 的迁移范式（独立的 `schema_meta` + `SCHEMA_VERSION` 从 1 起）。

```sql
-- 原始事件，append-only
CREATE TABLE usage_events (
  id       INTEGER PRIMARY KEY AUTOINCREMENT,
  ts       TEXT    NOT NULL,          -- RFC3339 UTC
  kind     TEXT    NOT NULL,
  path     TEXT,                      -- vault 相对路径，可空（如 search.query）
  mode     TEXT,
  dur_ms   INTEGER,
  extra    TEXT                       -- JSON，低频字段不开新列
);
CREATE INDEX idx_usage_events_ts   ON usage_events(ts);
CREATE INDEX idx_usage_events_path ON usage_events(path, ts);
CREATE INDEX idx_usage_events_kind ON usage_events(kind, ts);

-- 日聚合，画像查询只读这张表
CREATE TABLE usage_daily (
  day    TEXT    NOT NULL,            -- 本地日期 YYYY-MM-DD
  kind   TEXT    NOT NULL,
  path   TEXT    NOT NULL DEFAULT '',
  count  INTEGER NOT NULL DEFAULT 0,
  dur_ms INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (day, kind, path)
);
```

`usage_daily` 在写入事件的同一事务里 upsert（`ON CONFLICT DO UPDATE`），保证聚合与原始一致，画像查询不必扫全表。

同 catalog：WAL、`busy_timeout`、`foreign_keys`。

### 4.3 路径重命名

`paper_move` 改路径时，catalog 的 `papers.path` 会更新；usage 侧需同步。做法：`usage_rename_path(old, new)` 在 `catalog/commands.rs::paper_move` 成功后调用，`UPDATE usage_events / usage_daily SET path = new WHERE path = old OR path LIKE old || '/%'`。失配的历史事件不清理，画像查询 join 不到 title 时降级只显示路径。

### 4.4 需要更新的忽略列表

新增 `.agentero/usage.sqlite` 后必须排除，否则会触发 wiki 重建 / 搜索命中 / watcher 风暴：

- `features/vault/tree.rs`（`.agentero` 已整体隐藏，确认即可）
- `features/wiki/index.rs`
- `features/search/mod.rs`
- `features/watcher/mod.rs`（注意：catalog 是**故意不忽略**以触发刷新；usage 必须忽略）
- `features/remote/catalog_mirror.rs`（确认不会连带镜像）

## 5. UsageProfile

单个命令 `usage_profile_get(window_days)` 输出紧凑 JSON，目标 **≤800 tokens**（要塞进 prompt）。

| 字段 | 含义 | 算法 |
|---|---|---|
| `topPapers` | 最投入的 N 篇 | 时间衰减停留时长 `Σ dur_ms × 0.5^(age_days/14)`，取 Top 8，带 title/tags |
| `continueReading` | 该接着读的 | `is_read=0` 且有 `reading-position` 进度，按最后 focus 时间排序 |
| `stalled` | 卡住的 | 下载后从未 open，或累计 focus < 60s 且 ≥7 天未碰 |
| `tagAffinity` | 兴趣分布 | 按 `cat.papers.tags` 聚合衰减时长，归一化 Top 8 |
| `rhythm` | 节奏 | 活跃时段直方图（3h 桶）、日均时长、本周 vs 上周 |
| `agentUsage` | Agent 习惯 | 各 workflow 调用次数与占比 |

半衰期 14 天：既让「上周在啃的论文」仍占权重，又不让三个月前的旧课题长期霸榜。

## 6. Agent 接入：三层，按需取用

### 6.1 被动注入（默认，轻量）

`src-tauri/src/features/agent/prompts.rs` 的 `build_prompt` 是唯一 prompt 组装点。在现有 `personal_preference_directive()` 旁边加可选块：

```
<user_usage_profile>
近 30 天：日均 42min，活跃 21:00-24:00。
主要方向（时长占比）：diffusion 38% · 3D 22% · RL 11%
在读：[[Flow Matching]]（p12/24，2 天前）· [[DiT]]（p3/18，昨天）
搁置：[[NeRF Survey]]（下载 12 天未打开）
</user_usage_profile>
```

注入策略按 workflow 区分：

| workflow | 注入 | 理由 |
|---|---|---|
| `summary` | ✅ | 摘要可按用户方向调整侧重 |
| `qa` / `free` | ✅ | 通用对话最需要背景 |
| `related_work` | ✅ | 相关工作应贴合已读范围 |
| `paper_reader` | ❌ | 精读要忠于原文，习惯 context 是噪声 |

### 6.2 主动查询（详细）

给 headless CLI 加 `usage` 子命令组（`cli/src/commands/usage.rs`，与 `paper.rs` / `mark.rs` 同构，沿用 `--vault` / `--json`）：

```bash
agentero usage summary --days 30 --json
agentero usage top --days 30 --limit 20 --json
agentero usage timeline --path papers/xxx --json
```

BYOA Agent 本来就有 shell，这条路**不需要新增 MCP 或 Host command**，与既有 `agentero-cli` skill 的用法一致。被动注入给概览，需要细节时 Agent 自己拉。

### 6.3 Skill 编排

新增内置 vault skill `templates/vault/.agents/skills/usage-reviewer/SKILL.md`（frontmatter `version: 1`），约定：

- 何时该跑 `agentero usage summary`，何时只用注入的概览
- 周回顾输出格式（写 `notes/Reviews/YYYY-WW.md`，双链指向论文）
- 推荐输出必须给「因为你在读 X」的理由，不许凭空推荐库外论文
- 不许改写用户手写笔记，只在 `Reviews/` 下新建

同步 `src-tauri/src/features/vault/mod.rs` 的 seed 常量列表（`include_str!` + `ensure_vault` 时补齐）。

## 7. 上层功能

### 7.1 继续阅读（纯本地，零 Agent 成本）

论文库顶部一行卡片：`continueReading` 前 3 + `stalled` 前 2。点击直接 `openPaper` 并跳到 `reading-position` 的页码。这是投入产出比最高的一项，先做。

### 7.2 周回顾

手动触发（论文库工具栏 / 命令面板）→ 以 `usage-reviewer` skill 跑一次 → 产出 `notes/Reviews/YYYY-WW.md`：本周读了什么、时长分布、新增标签、卡住的论文、下周建议。不做自动定时触发（避免无声消耗 Agent 额度）。

### 7.3 推荐（v0，无 embedding）

候选集三路并集，全部来自**本地已有信号**：

1. `topPapers` 在双链图中的邻居（`graph_get_graph`）
2. 与 `tagAffinity` 高分标签同标签的未读论文
3. `topPapers` 参考文献中已入库的条目（`features/refs`）

排序 `score = 衰减时长贡献 × 标签亲和度 × 未读加成`，去掉已读与近期打开过的。每条附理由。与 [`plaza.md`](plaza.md) 的「论文推荐 v0」是同一件事的库内版本——**plaza 面向库外发现，本节面向库内重拾**，两者共用 `tagAffinity`。

## 8. 隐私与开关

- 新增 `AppSettings::usage_tracking_enabled`（`serde(default = "default_true")`）。关闭后**不写库**（不是写了不用）。
- 使用记录只存本地，不经任何网络通道上报。
- 保留期 180 天，`ensure_usage` 时 prune `usage_events`；`usage_daily` 保留 2 年（体积极小，支撑年度回顾）。
- `usage_clear()` 命令 + 设置页「清除使用记录」按钮。
- `search.query` 存原文（否则推荐无从利用），受同一开关与保留期约束，须在设置页文案与文档中明示。
- 使用记录**不随远程 Vault 同步**（§4.1），需在 `docs/backend/remote.md` 说明。

## 9. 分期

| 阶段 | 内容 | 可验证产出 |
|---|---|---|
| **P0** | `features/usage` + schema + 前端埋点 + 批量上报 + 开关 + prune | `agentero usage timeline --json` 能 dump 出真实事件 |
| **P1** | `usage_profile_get` + 论文库「继续阅读」卡片 | UI 上能看到该接着读哪篇并跳对页码 |
| **P2** | `build_prompt` 注入 + CLI `usage summary` / `top` | Agent 回答体现方向偏好；关闭开关后注入消失 |
| **P3** | `usage-reviewer` skill + 周回顾 + 推荐 v0 | 生成 `Reviews/YYYY-WW.md`；推荐列表带理由 |

P0/P1 完全不依赖 Agent，可独立交付价值；P2 起才涉及 prompt 与额度。

## 10. 风险

| 风险 | 缓解 |
|---|---|
| 埋点散落各处，日后漏埋 | 只在 `openTab` 这一个漏斗埋 open；其余 6 类事件集中在 3 个文件 |
| focus 计时把挂机算成阅读 | 单段上限 30min；`window blur` 立即结算 |
| prompt 注入把 context 撑爆 | 画像输出硬上限 ≤800 tokens，超出截断 `topPapers` |
| 用户觉得被监视 | 设置页显式开关 + 一键清除 + 文档写明本地不上传 |
| usage.sqlite 触发 watcher 风暴 | §4.4 忽略列表逐项确认 |
| 与 `plaza.md` 推荐重复实现 | 共用 `tagAffinity`，plaza 管库外、本文管库内 |

## 11. 文档落点

- 本文（草案）：`docs/development/usage-analytics.md`，登记进 `development/index.md` 与 `mkdocs.yml`
- 实现后：`docs/backend/usage.md`（schema / 命令 / CLI）+ `docs/frontend/library.md`（继续阅读卡片）
- 需同步：`docs/backend/data-model.md`（新库与表）、`docs/backend/agent.md`（注入块）、`docs/backend/remote.md`（不同步）、`docs/backend/cli.md`（`usage` 命令组）、`docs/development/roadmap.md` + `todo.md`
