# Agentero TODO

仅列**未完成**项。当前发布 **`0.6.0`**。版本切片见 [`roadmap.md`](roadmap.md)；已实现能力见 [`../frontend/`](../frontend/index.md) · [`../backend/`](../backend/index.md)。

## 0.3 — 入库与 Agent 补强

- [ ] 关键词/描述 → Agent 候选列表确认后入库
- [ ] 本机 Translator sidecar 捆绑（可选）
- [ ] 前端 `afterPaperImport` 策略表统一各入口后置
- [ ] Zotero 迁移走 `paper_commit`；remote 镜像层收敛；统一 `paper:imported` 事件
- [ ] 最近 Vault / UI 偏好与 XDG settings 完全对齐
- [ ] 设置「打开/导出日志文件夹」
- [ ] `catalog:export_papers_md`（Markdown 表）
- [ ] CLI：shell completions
- [ ] CLI：`export papers-md`（随 Host 导出）
- [ ] CLI / Agent：正文句子高亮 / 翻译 mark（pending hydrate）+ Skill 全量（[#170](https://github.com/poco-ai/Agentero/issues/170)，设计：[mark-cli-roadmap.md](mark-cli-roadmap.md)、[mark-locate-lazy.md](mark-locate-lazy.md)、[mark-locate-eager.md](mark-locate-eager.md)）
- [ ] 官方 `Zotero.dotm` → Agentero provider：先做 macOS `:23119` HTTP + Word Automation Go/No-Go，通过后交付 Catalog/CSL/Refresh 闭环；Windows `WM_COPYDATA` + OLE 后置。需完成 AGPL/GPL 与商标审核，不能与 Zotero Desktop 并行（[#167](https://github.com/poco-ai/Agentero/issues/167)，设计：[zotero-word-integration.md](zotero-word-integration.md)）

## 0.4 — Vault 采纳与导入加深

- [ ] Vault 采纳：`vault_inspect` + 安全补脚手架/catalog（不覆盖用户文件）
- [ ] 确认后：散落 PDF → paper 单元 + catalog
- [ ] catalog ↔ 磁盘漂移报告与可选清理
- [ ] Skill `vault-organize`；CLI `vault inspect|adopt`
- [ ] 从 PDF 识别 DOI/arXiv + 元数据确认增强
- [ ] MinerU BYOK 云端解析（可选）

## 0.5 — 广场 Plaza

设计稿：[`plaza.md`](plaza.md)

- [ ] 侧栏虚拟 `agentero:plaza` + Cool Papers WebView / 推荐 v0 / 播客占位
- [ ] 从发现流解析 URL → 魔棒入库（可后置）

## 0.6 — 引用关系

设计稿与实现：[../backend/citation-parsing.md](../backend/citation-parsing.md)

- [ ] 反向联动：hover 引用卡片 → PDF 文中 anchor 高亮（需 anchors bbox）
- [ ] 本地 PDF citation/figure sidecar + Paper Content 侧栏
- [ ] Agent `#` 编号提及 + 引用卡片拖拽（citation-parsing M3/M5）
- [ ] cites/cited_by 持久缓存 + Connected Papers 式布局 / 多跳聚类
- [ ] Agent：Explore citations / Map related work / Ingest neighborhood
- [ ] PDF 正文层检索；搜索历史/过滤；命令注册表 + MRU

## 0.7+ — 体验与平台

- [ ] \#239：翻译 / 版面 / 批注漏斗、UsageProfile、Agent 注入、PostHog 投影
- [ ] Graph 全屏/聚焦、邻居高亮、节点搜索；边级增量索引
- [ ] tab pin、命名工作区会话
- [ ] PDF 无文本层降级；HTML 标注统一模型
- [ ] 翻译：更多 adapter / 消费方 / 词典
- [ ] 更多 Skills（多篇对比、Idea 评估、实验复现清单等）
- [ ] 自动 changelog；多 arch artifact 命名
- [ ] iOS/iPad M3：TestFlight 内测推进、多主机切换、iPad 双栏、wiki backlinks、离线体验打磨
- [ ] Git 集成 / 可选云同步
- [ ] 引用图 deeper（聚类、作者机构图）
- [ ] CLI domain 抽离独立 crate（仅当边界成为问题时）
