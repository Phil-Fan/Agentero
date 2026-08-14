# 开发草稿

本目录只放：

1. **路线图 / TODO / 发布**（工程规划）
2. **尚未实现**的功能设计稿

已实现功能的说明在 [`../frontend/`](../frontend/index.md) 与 [`../backend/`](../backend/index.md)，按功能分篇。

## 规划

| 文档 | 说明 |
|---|---|
| [roadmap.md](roadmap.md) | 自 **0.5.0** 起的未来版本切片（无已实现清单） |
| [todo.md](todo.md) | **仅未完成** backlog（按 0.3 / 0.4… 分组） |
| [bug.md](bug.md) | 已知问题语料（精简） |
| [interface-details-review.md](interface-details-review.md) | 界面细节审计（PDF 交互面）**已闭环**：修复记录 + 回归护栏（已达标表面，勿改坏）+ 交互改动自查清单 |

## 未实现草稿

| 文档 | 主题 |
|---|---|
| [paper-pipeline-orchestration.md](paper-pipeline-orchestration.md) | 打开论文的三条时间轴（T0/T1/T2）、`paper_open_bundle` 聚合命令、内存 Job Center 与五条流水线（下载 / liteparse / 引用 / 图谱 / layout）编排；含 Operation/Job 判定、`auto`/`all` 类函数审计、依赖选型。零 schema 变更 |
| [paper-behavior-dag.md](paper-behavior-dag.md) | 导入（identifier / Connector / 本地 PDF / BibTeX 四条入口）与打开论文的行为 DAG、现状不合理之处（`PAPER.md` 生成责任在前端、refs 重复 spawn、PDF/TeX 串行）与目标 DAG |
| [plaza.md](plaza.md) | 广场（Cool Papers / 推荐 / 播客） |
| [usage-analytics.md](usage-analytics.md) | \#239 Activity 总线。P0 存储已落地：XDG `usage.sqlite`（见 [../backend/usage.md](../backend/usage.md)） |
| [zotero-word-integration.md](zotero-word-integration.md) | 官方 Zotero Word 插件 provider 兼容、文档迁移与平台实现评估 |
| [mark-cli-roadmap.md](mark-cli-roadmap.md) | \#170 阅读标注**内置进 CLI**（方案/命令面/边界）+ 基础→上层→Skill；与 [CLI 文档](../backend/cli.md) 分发衔接 |
| [mark-locate-lazy.md](mark-locate-lazy.md) | 文字定位：打开 PDF 再算（惰性，默认主路径） |
| [mark-locate-eager.md](mark-locate-eager.md) | 文字定位：标注时算（即时 B1 viewer / 可选 B2 headless） |

macOS 签名与公证（已实现流程说明）在 [`../bug_fix/macos-signing.md`](../bug_fix/macos-signing.md)。
