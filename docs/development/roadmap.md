# Agentero 路线图

**当前发布版本：`0.6.0`**

已实现能力见功能文档（[`../frontend/`](../frontend/index.md) · [`../backend/`](../backend/index.md)），不在此重复勾选。  
可执行 backlog 见 [`todo.md`](todo.md)。

## 原则

- Local-first：笔记与源文件是普通文件；catalog 只权威存论文集合/元数据。
- Agent 采用 **BYOA**（ACP Client），不捆绑模型、不托管 API Key。
- 先做好精确入库与阅读闭环，再扩展发现流与文献引用图。
- 不静默覆盖用户手写 Vault 文件。

## 0.6.0 基线（当前线，摘要）

0.5 工作台之上，本线已落地：文献引用近邻图（`paper_refs_graph` 节点 `role` + 库外 stub，嵌在 References 下方；全库模式可切换）、论文 `attachments/` 在文件树展开、XDG `usage.sqlite`（`track()` + CLI `usage` + 设置页开关/清除）、PDF 全文翻译按阅读顺序分批带上下文、Host 独占 skill 激活语法且翻译不套 envelope。其余 0.5 能力仍在：Dockview、Catalog/Library、魔棒与本地 PDF、Zotero Connector/迁移、PDF 划词、版面分析、视觉批注 v2、BYOA Agent、双链、CLI `layout`/`mark`、远程 Vault MVP、iOS 远程客户端 M2。细节以功能文档为准。

---

## 0.3 — 入库与 Agent 补强

在当前基线上补齐仍缺口的「输入 → 资产 → Agent」路径。

| 主题 | 交付 |
|---|---|
| 智能入库 | 关键词/描述 → Agent 候选列表 → 确认入库 |
| 入库编排 | 前端 `afterPaperImport` 统一后置；迁移/远程走同一 `paper_commit`；可选 `paper:imported` 事件 |
| Agent | workflow 自动注入 Vault `AGENTS.md` |
| 配置 | 最近 Vault / UI 偏好与 XDG settings 完全对齐；设置内打开/导出日志目录 |
| 导出 | `catalog:export_papers_md`（Markdown 表） |
| CLI | shell completions；`export papers-md` 对齐（Doctor 已实现；不要发明 `graph` 子命令，引用图走 Host `paper_refs_graph`） |
| 阅读标注 CLI | `mark` CRUD + 默认惰性文字定位 + Skill（[#170](https://github.com/poco-ai/Agentero/issues/170)，设计：[mark-cli-roadmap.md](mark-cli-roadmap.md)） |
| 桌面分发 | 桌面安装包内置同版本 `agentero` CLI；命令行打开本地 Vault；各平台 PATH 策略 |

可选：本机 Translator sidecar 捆绑。

**验收方向**：自然语言/关键词可确认后入库；各入口入库后置行为一致；Agent 工作流带上 Vault 级 `AGENTS.md` 约束。

---

## 0.4 — Vault 采纳与导入加深

打开「非标准 / 半结构」文件夹时也能用上 Agentero，而不只靠 Create Vault。

| 主题 | 交付 |
|---|---|
| Zotero 双向同步 | 已落地：映射层 `zotero_sync`（拉取元数据/笔记/批注 + NOTES.md 标记块推送回 Zotero，离线直写 + 备份 + 事务；见 [../backend/identifier-lookup.md](../backend/identifier-lookup.md) §17） |
| 发现 | `vault_inspect` 只读报告（结构、catalog、散落 PDF、漂移） |
| 安全整理 | 补缺脚手架与 catalog；种子模板**不覆盖**已有文件 |
| 确认迁移 | 散落 PDF → paper 单元 + catalog；漂移修复可选 |
| Skill / CLI | 可选 `vault-organize`；`vault inspect\|adopt` |
| PDF 元数据 | 从 PDF 识别 DOI/arXiv + 确认面板增强 |
| 解析 | 可选 MinerU BYOK 云端解析 |

**验收方向**：打开已有研究目录可得到可理解报告，确认后安全变成可用 Vault。

---

## 0.5 — 广场（发现）

侧栏虚拟发现入口，与 Library（已收藏）正交。设计：[plaza.md](plaza.md)

| 阶段 | 交付 |
|---|---|
| 壳 | `agentero:plaza` + 子节点；中间栏发现 UI |
| Cool Papers | 内嵌 WebView 浏览 papers.cool |
| 推荐 v0 | 本地启发式（待精读 / 最近入库 / 同标签） |
| 播客 | 占位 |
| 后续 | 从发现流解析 URL → 魔棒入库（本版本可不做） |

**验收方向**：不离开应用即可浏览外部发现流；P0 可不写 Vault。

---

## 0.6 — 引用关系与 Connected Papers

本地引用/插图 → 邻域图 → Agent 引用工作流。实现见 [../backend/citation-parsing.md](../backend/citation-parsing.md)；PDF layout raw cache 已落到 `{paper}/source/layout.json`，最终插图 sidecar 延后。

| 主题 | 交付 |
|---|---|
| 解析 | 本地 PDF/TeX → citation/figure sidecar |
| UI | Paper Content 侧栏；文内引用 hover → Paper Info 已实现；待补卡片到 PDF 的反向高亮 |
| PDF Agent | 视觉批注草稿 → 多模态 Agent 会话与 trace 回跳已实现；待补自动视觉区域检测 |
| 图 | 近邻图（`center` / `reference` / `citedBy` / stub）+ 全库切换已落地；cites/cited_by 持久缓存与 Connected Papers 式布局加深 |
| Agent | Explore citations / Map related work / Ingest neighborhood |
| 检索 | PDF 正文层检索；搜索历史/过滤；命令注册表 + MRU |

与 **双链 Graph**（`[[wikilinks]]`）分层，不共用边语义。

**验收方向**：打开一篇本地 paper 能看到可跳转的引用/插图，并沿邻域打开或入库。

---

## 0.7+ — 体验加深与平台

不绑定单一小版本，按优先级穿插：

| 方向 | 示例 |
|---|---|
| 双链 / Graph | 全屏聚焦、邻居高亮、节点搜索；边级增量索引 |
| 工作区 | tab pin、命名工作区会话 |
| PDF / 翻译 | 无文本层降级；HTML 标注；更多翻译 adapter/消费方 |
| Word 引用 | 优先兼容用户已安装的官方 Zotero Word 插件：macOS `:23119` provider + Word Automation，后续 Windows `WM_COPYDATA` + OLE；本机 Library 检索、CSL 引文/参考文献刷新与文档副本迁移。单 provider、AGPL/GPL/商标审核为发布门槛（设计：[zotero-word-integration.md](zotero-word-integration.md)） |
| Skills | 多篇对比、Idea 评估、实验复现清单 |
| 发布 | 自动 changelog；多 arch artifact 命名 |
| 平台 | **iOS/iPad 纯远程客户端 M2 已提交 TestFlight**（见 [移动端前端与远程架构](../frontend/mobile.md)），M3 打磨中；**Android 端已初始化**（`gen/android`，复用同一移动壳，待发布流水线）；自托管 [`poco-ai/paseo-relay`](https://github.com/poco-ai/paseo-relay) 远程中继、Git 集成、可选云同步 |
| 引用图 deeper | 聚类、作者/机构图 |
| 工程 | CLI domain 抽离独立 crate（仅当边界成为问题时） |

---

## 版本号约定

- 应用 / Tauri / CLI manifest 与 git tag `vX.Y.Z` **一致**（见 [release.md](../test/release.md)）。
- **当前线：`0.6.0`**。下一功能版本从 **`0.7.0`** 起；`0.6.x` 仅用于 0.6 基线的补丁与热修。
- 路线图版本（0.3 / 0.4…）是**产品切片**，落地时再写入 manifest 并打 tag。

## 相关文档

- 未完成清单：[todo.md](todo.md)
- 技术选型：[`../frontend/index.md`](../frontend/index.md) · [`../backend/index.md`](../backend/index.md)
- 功能说明：前端 / 后端 index
