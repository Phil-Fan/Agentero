# 开发草稿

本目录只放：

1. **工程规划**（版本切片、发布）
2. **尚未实现**的功能设计稿

已实现功能的说明在 [`../frontend/`](../frontend/index.md) 与 [`../backend/`](../backend/index.md)，按功能分篇。

## 未实现草稿

| 文档 | 主题 |
|---|---|
| [plaza.md](plaza.md) | 广场（Cool Papers / ModelScope 论文 / 推荐 / 播客）。**壳 + 两个站点来源浏览与入库已落地**（`agentero-coolpapers` / `agentero-modelscope` 站点代理）；推荐 / 播客仍为草稿 |
| [plaza-feeds.md](plaza-feeds.md) | 广场订阅 MVP：本地 RSS/Atom 时间线 + 论文入库。**已落地** |
| [usage-analytics.md](usage-analytics.md) | \#239 Activity 总线。P0 存储已落地：XDG `usage.sqlite`（见 [../backend/usage.md](../backend/usage.md)） |
| [zotero-word-integration.md](zotero-word-integration.md) | 官方 Zotero Word 插件 provider 兼容、文档迁移与平台实现评估 |
| [mark-cli-roadmap.md](mark-cli-roadmap.md) | \#170 阅读标注**内置进 CLI**（方案/命令面/边界）+ 基础→上层→Skill；与 [CLI 文档](../backend/cli.md) 分发衔接 |
| [mark-locate-lazy.md](mark-locate-lazy.md) | 文字定位：打开 PDF 再算（惰性，默认主路径） |
| [mark-locate-eager.md](mark-locate-eager.md) | 文字定位：标注时算（即时 B1 viewer / 可选 B2 headless） |

macOS 签名与公证（已实现流程说明）在 [`../bug_fix/macos-signing.md`](../bug_fix/macos-signing.md)。
