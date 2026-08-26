# MCP Server

桌面 Host 内嵌的 Streamable HTTP MCP。设置开关打开后在 loopback 监听；关掉即停。作用域是当前打开的**本地** Vault。

远端 Vault 不服务。App 必须开着。

## 开关与地址

设置 → 通用 → **MCP server**。

| 设置 | 默认 | 说明 |
|---|---|---|
| `mcpEnabled` | `false` | 启停 listener |
| `mcpPort` | `8765` | 只绑 `127.0.0.1` |

监听 URL：`http://127.0.0.1:{port}/mcp`。设置页端口旁绿点表示正在听；点击 URL 复制。

Host commands：`mcp_get_status` / `mcp_set_enabled` / `mcp_set_port` / `mcp_set_vault` / `mcp_set_parent_dir`。状态事件 `mcp:status`。

无鉴权。不要把端口绑到非 loopback。

## ChatGPT Secure MCP Tunnel

App 开着且开关打开后：

```bash
tunnel-client init --profile agentero \
  --tunnel-id tunnel_… \
  --mcp-server-url http://127.0.0.1:8765/mcp
tunnel-client run --profile agentero
```

Codex / Inspector 也可直接打该 URL。stdio 子进程不是这条通路。

## Resource

Vault 概况不是 tool，是文档：

| URI | MIME | 内容 |
|---|---|---|
| `agentero://vault` | Markdown | 路径、schemaVersion、papers、unread |

无 Vault 时 resource 仍列出，`resources/read` 返回「未打开 Vault」正文。`initialize` instructions 提示先读这份文档，再 `paper_list` / `paper_get`。

## Tools

`ref` = paper id 或 vault 相对路径（如 `papers/1706.03762`）。禁止 `..`。

| Tool | 作用 |
|---|---|
| `paper_list` | 列表 metadata：`id/path/title/authors/year/tags/doi/arxivId/publication/status/isRead`。`query?`、`tag[]?`、`unread?`、`limit?`（默认 50，封顶 200）。abstract 只在 `paper_get`。 |
| `paper_get` | 单篇完整 catalog 行 |
| `import_id` | 魔棒入库（arxiv / DOI / URL）。`parent?` 默认当前 Library 作用域或 `papers` |
| `paper_notes_get` | 读 `{paper}/NOTES.md`（文件不存在则空字符串） |
| `paper_notes_write` | 写 `NOTES.md`。`mode`: `replace`（默认）或 `append` |
| `paper_tag_add` | 加标签；可用 `topic:blue` 色后缀 |
| `paper_tag_rm` | 删标签 |

`paper_notes_write`：

- 只写 `NOTES.md`，不动 `PAPER.md` / `source/` / `marks/`
- 原子写
- `replace`：新内容没有 YAML frontmatter 时保留原 aliases 头
- `append`：追加正文，保留 frontmatter
- 编辑器有未存改动时走现有 `vault:file-changed` 冲突逻辑

不做：`vault_info` tool、通用读文件、`paper_paths`、delete/trash、mark/layout、shell。

## 代码

`src-tauri/src/features/mcp/`：`McpController` + Streamable HTTP（`rmcp`）+ tools/resource。直接调 `features::{catalog, import, vault}`。

## 安全

- 仅 `127.0.0.1`；默认关
- `ref` / `parent` 消毒
- 不暴露 PDF 二进制、不读 XDG API key
