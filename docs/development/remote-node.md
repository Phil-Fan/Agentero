# 远端 Node（Headless Agentero Node）

> 状态：**M1 已落地** —— `crates/agentero-node` 提供身份 / Agent Card / 发现登记表 / 节点服务器。
> A2A 任务端点、配对信任、远端 Agent 拉起、队列 / RunSet 与桌面 / CLI 集成为后续（见 §7）。

## 1. 背景

本地聊天、读文献，实验却在远端服务器上跑。要让「本地 UI 驱动远端 Agent」，远端需要一个
**可发现、可寻址、可承载任务**的节点。本 crate 就是那个节点工件。

## 2. 两种节点形态

| | A. SSH-only | B. Headless Node（本 crate） |
|---|---|---|
| 远端需要 | 只要 shell + 一个 CLI Agent | `agentero-node` 二进制 |
| 通信 | ACP over SSH | A2A over 隧道（后续） |
| 能力 | 交互式聊天、单次任务 | 持久任务、队列、断线恢复、多客户端 |
| 复杂度 | 零安装 | 一次安装 + 配对 |

M1 先把形态 B 的**地基**做出来；形态 A 保持现状，二者不冲突。

## 3. 本 crate 的组成

`crates/agentero-node/`（tauri-free，**不依赖 `agentero-core`**）：

| 模块 | 职责 |
|---|---|
| `identity` | Ed25519 节点身份；`nodeId` = 公钥 SHA-256 指纹（hex），`shortId` = 前 12 位 |
| `probe` | 主机能力快照：hostname / os / arch / cpu / memory / GPU（`nvidia-smi`） |
| `card` | A2A 形状的 Agent Card（v0.3）+ `agentero` 扩展块；`fetch()` 拉取对端 Card |
| `registry` | 本机「已发现节点」登记表（JSON，保留原始 Card 以向前兼容） |
| `server` | axum 回环服务器：`/.well-known/agent-card.json` + `/healthz` |
| `paths` | XDG 路径（与 `agentero-core::paths` 同布局） |

**为什么自包含而不复用 `agentero-core`**：节点要能被拷到裸服务器上单独 `cargo build`，
不能拖入桌面依赖树（rusqlite / liteparse / PDFium）。因此显式 pin 依赖版本、自带
`paths` / `error`。`identity` 的 `nodeId` 由 Ed25519 公钥指纹派生，避免碰撞。

### CLI

```text
agentero-node init [--name N]      # 生成节点身份
agentero-node card [--url URL]     # 打印本机 Agent Card
agentero-node serve [--listen A]   # 回环提供 card + /healthz
agentero-node probe <target>       # 拉取对端 Agent Card
agentero-node add <target>         # probe 并登记到本机 registry
agentero-node list                 # 列出已登记节点
agentero-node remove <node-id>     # 忘记节点
```

`target` 接受基址（`http://host:8766`）或完整 Card URL（`…/.well-known/agent-card.json`）。

## 4. 路径约定

| 路径 | 内容 |
|---|---|
| `$XDG_CONFIG_HOME/agentero/node/identity.json` | 本机节点身份（私钥 `0600`） |
| `$XDG_CONFIG_HOME/agentero/nodes.json` | 已发现节点登记表 |

## 5. 发现 / 安装 / 通信（设计）

- **发现**：v1 手动 `add` + 从 `~/.ssh/config` 导入 + 配对码 TOFU；后续 mDNS 局域网发现、中心 registry。
- **安装**：形态 B 只需一个二进制（`cargo build` / 预编译分发 / 安装脚本 + systemd 为后续）。
- **通信**：v1 用 **SSH 端口转发 + A2A over loopback**，复用 SSH 密钥鉴权、无需暴露端口；云 / NAT 场景后续走 outbound tunnel。A2A 端点（JSON-RPC + SSE）为 M2。

链路上的一个要点：**任务队列放在远端节点**（worker 从自己的队列拉取），本地只是提交方，
从而「本地关掉 App，远端任务继续」。

## 6. 验证（实测）

| 主机 | 架构 | 结果 |
|---|---|---|
| 本机 macOS | aarch64 | `init/card/serve/probe/add/list` 全通过；8 单测通过 |
| `dgx-remote` | aarch64 | 独立构建 16s；Card 报告 `spark` / 20 CPU / 119.7 GiB / `NVIDIA GB10` |
| `a1` | x86_64 | 裸机装 rustup 后 2m04s 构建；报告 44 CPU / 4×`A100-PCIE-40GB` |

本机经 SSH 隧道同时探测并登记两个远端，registry 同时含两台节点。

## 7. 后续

1. **M2 A2A**：节点增加 JSON-RPC + SSE 端点（`SendMessage` / `GetTask` / `CancelTask`）。
2. **M2 配对**：配对码 + 长期 Bearer token + 公钥 pinning；token 入 OS keychain。
3. **M2 远端 Agent**：节点拉起 CLI Agent（ACP Host），把任务落到 Workspace。
4. **M3 编排**：RunSet / Run 队列 + worker（见 [A2A.md](A2A.md) 与并行实验设计）。
5. **集成**：桌面 / `agentero-cli` 加 `node` 子命令与「远端会话」面板；安装脚本 + systemd 单元。

## 8. 代码

`crates/agentero-node/`（`src/{main,cli,identity,probe,card,registry,server,paths,error}.rs`）
