# 用 MCP 连接外部 Agent

设置里打开 **MCP server** 后，ChatGPT / Codex / MCP Inspector 可以调用当前 Vault 的论文库（列表、入库、写 NOTES），不必把 MCP 暴露到公网。

应用必须开着。远端 Vault 不可用。

应用内 Agent 面板走 ACP（[接入 Agent](agents.md)）；本篇是给 **外部** MCP 客户端用的。

## 1. 打开 Agentero MCP

1. 打开一个**本地** Vault。
2. **Settings → General → MCP server** 打开开关。
3. 默认地址 `http://127.0.0.1:8765/mcp`。绿点表示正在监听；点击地址可复制。

本机 Inspector / 能打 loopback 的 Codex 可以直接用这个 URL。ChatGPT 在云端，需要再装 `tunnel-client`（下一步）。

协议与工具表：[backend/mcp.md](../backend/mcp.md)。

## 2. 安装 tunnel-client

`tunnel-client` 是 OpenAI 的 Secure MCP Tunnel 客户端：在你机器上出站连 OpenAI，把排队的 MCP 请求转给 Agentero，**不开入站端口**。仓库：[openai/tunnel-client](https://github.com/openai/tunnel-client)。

不要装 Homebrew 核心里的 `brew install tunnel`（那是另一个项目）。

### macOS：Homebrew（推荐）

```bash
brew install openai/tools/tunnel-client
tunnel-client --version
tunnel-client help quickstart
```

Formula 会装完整 CLI（`init` / `doctor` / `run`），并带上配套 `cloudflared`。

### 从 Platform 下载

打开 [Platform Tunnels](https://platform.openai.com/settings/organization/tunnels)，用页面上的官方下载。runbook 指向 latest，不要写死版本号。

### 从 GitHub Release 下载

最新稳定版：[github.com/openai/tunnel-client/releases/latest](https://github.com/openai/tunnel-client/releases/latest)。

选 **full client** zip（能 `init` / `doctor` / `run`），不要只含 `run` 的 `runtime` 包。

| 机器 | 资源名片段 |
|---|---|
| macOS Apple Silicon | `darwin-arm64` |
| macOS Intel | `darwin-amd64` |
| Linux | `linux-amd64` 或 `linux-arm64` |
| Windows | `windows-amd64` 或 `windows-arm64` |

解压后把 `tunnel-client` 放到 `PATH`。

### Docker（服务器）

```bash
docker pull ghcr.io/openai/tunnel-client:latest
```

生产 pin 精确 tag 或 digest。镜像是 Linux `amd64` / `arm64`。

### 从源码编译

```bash
git clone https://github.com/openai/tunnel-client.git
cd tunnel-client
make admin-ui
make tunnel-client
./bin/tunnel-client help quickstart
```

## 3. 创建隧道和运行时 Key

在 [Platform Tunnels](https://platform.openai.com/settings/organization/tunnels) 新建隧道，记下 `tunnel_id`（形如 `tunnel_` + 32 位小写十六进制）。

在 [Runtime API keys](https://platform.openai.com/settings/organization/api-keys) 建 **Restricted** key，权限选 Tunnels **Read + Use**。这是 `CONTROL_PLANE_API_KEY`，给长期运行的 daemon 用。不要把 Admin Key 塞给 `tunnel-client run`。

权限是 organization 级：

- 创建 / 编辑隧道：Tunnels **Read + Manage**
- 跑 client、在 ChatGPT 里选隧道：Tunnels **Read + Use**

ChatGPT 开发者模式是另一套 workspace 权限，和 Platform 隧道权限无关。

隧道要关联**目标 ChatGPT workspace**，否则 App 创建页列表里看不到。

## 4. 接到 Agentero

Agentero MCP 开关保持打开，然后：

```bash
export CONTROL_PLANE_API_KEY="sk-..."

tunnel-client init \
  --profile agentero \
  --tunnel-id tunnel_0123456789abcdef0123456789abcdef \
  --mcp-server-url http://127.0.0.1:8765/mcp

tunnel-client doctor --profile agentero --explain
tunnel-client run --profile agentero
```

`tunnel-client run` 是前台进程，**要一直开着**。关掉 Agentero、关掉 MCP 开关、或停掉 `tunnel-client`，ChatGPT 侧的工具调用都会失败。

本机确认：

- `http://127.0.0.1:8080/readyz`（tunnel-client 默认健康口，loopback）
- `http://127.0.0.1:8080/ui`（tunnel-client 仪表盘）
- Agentero 设置页 MCP 地址仍是绿点

## 5. 接到 ChatGPT

1. ChatGPT **Settings → Security and login** 打开 Developer mode。
2. 打开 [ChatGPT Plugins](https://chatgpt.com/plugins)，点 `+` 建开发者模式 App。
3. **Connection** 选 **Tunnel**，从列表选隧道或粘贴 `tunnel_id`。
4. 创建后检查发现到的 tools。外部 Agent 应先读资源 `agentero://vault`。

官方说明：[Secure MCP Tunnel](https://developers.openai.com/api/docs/guides/secure-mcp-tunnels)。

列表里没有隧道时：确认隧道关联了该 ChatGPT workspace、操作者有 Tunnels **Use**、`tunnel-client` 的 `/readyz` 为 200。

## 能做什么

- `paper_list` / `paper_get` — 论文 metadata
- `import_id` — 用 arXiv / DOI / URL 入库
- `paper_notes_get` / `paper_notes_write` — 读写该篇 `NOTES.md`
- `paper_tag_add` / `paper_tag_rm` — 标签

## 常见问题

| 现象 | 处理 |
|---|---|
| Agentero 设置里没有绿点 | 先打开本地 Vault，再开 MCP 开关；端口被占用则换 `mcpPort` |
| `tunnel-client doctor` 缺 Key | 检查 `CONTROL_PLANE_API_KEY`；用 Runtime key，不要用 Admin key |
| ChatGPT 看不到隧道 | 隧道要挂目标 workspace，不只是 Platform org；需要 **Use** 权限 |
| 工具调用失败 | Agentero 和 `tunnel-client run` 都要在跑；先看 `/readyz` 和 Agentero 绿点 |
| Homebrew 装错包 | 必须是 `openai/tools/tunnel-client`，不是 `tunnel` |
