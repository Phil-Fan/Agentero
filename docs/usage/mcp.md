# 用 MCP 连接外部 Agent

设置里打开 **MCP server** 后，ChatGPT / Codex / MCP Inspector 可以调用当前 Vault 的论文库（列表、入库、写 NOTES），不必把 MCP 暴露到公网。

应用必须开着。远端 Vault 不可用。

## 打开服务器

1. 打开一个**本地** Vault。
2. **Settings → General → MCP server** 打开开关。
3. 默认地址 `http://127.0.0.1:8765/mcp`。绿点表示正在监听；点击地址可复制。

## ChatGPT（Secure MCP Tunnel）

本机再跑 `tunnel-client`，把 loopback MCP 接到 ChatGPT：

```bash
tunnel-client init --profile agentero \
  --tunnel-id tunnel_… \
  --mcp-server-url http://127.0.0.1:8765/mcp
tunnel-client run --profile agentero
```

然后在 ChatGPT 开发者模式 App 的 Connection 选 **Tunnel**。详见 [Secure MCP Tunnel](https://developers.openai.com/api/docs/guides/secure-mcp-tunnels)。

## 能做什么

外部 Agent 应先读资源 `agentero://vault`，再用：

- `paper_list` / `paper_get` — 论文 metadata
- `import_id` — 用 arXiv / DOI / URL 入库
- `paper_notes_get` / `paper_notes_write` — 读写该篇 `NOTES.md`
- `paper_tag_add` / `paper_tag_rm` — 标签

协议细节：[backend/mcp.md](../backend/mcp.md)。
