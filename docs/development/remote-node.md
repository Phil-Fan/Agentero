# 远端 Node（Headless Agentero Node）

> 状态：**M1 + M2 已落地** —— `crates/agentero-node` 提供身份 / Agent Card / 发现登记表 / 节点服务器，
> 以及 **A2A JSON-RPC + SSE 任务端点**（`message/send` / `message/stream` / `tasks/get` / `tasks/cancel`）。
> 配对信任、远端 Agent 拉起、队列 / RunSet 与桌面 / CLI 集成为后续（见 §8）。

## 1. 背景

本地聊天、读文献，实验却在远端服务器上跑。要让「本地 UI 驱动远端 Agent」，远端需要一个
**可发现、可寻址、可承载任务**的节点。本 crate 就是那个节点工件。

## 2. 两种节点形态

| | A. SSH-only | B. Headless Node（本 crate） |
|---|---|---|
| 远端需要 | 只要 shell + 一个 CLI Agent | `agentero-node` 二进制 |
| 通信 | ACP over SSH | A2A over 隧道 |
| 能力 | 交互式聊天、单次任务 | 持久任务、队列、断线恢复、多客户端 |
| 复杂度 | 零安装 | 一次安装 + 配对 |

形态 B 的地基（身份 / 发现 / A2A 任务）已在 M1 + M2 完成；形态 A 保持现状，二者不冲突。

## 3. 本 crate 的组成

`crates/agentero-node/`（tauri-free，**不依赖 `agentero-core`**）：

| 模块 | 职责 |
|---|---|
| `identity` | Ed25519 节点身份；`nodeId` = 公钥 SHA-256 指纹（hex），`shortId` = 前 12 位 |
| `probe` | 主机能力快照：hostname / os / arch / cpu / memory / GPU（`nvidia-smi`） |
| `card` | A2A 形状的 Agent Card（v0.3）+ `agentero` 扩展块；`fetch()` 拉取对端 Card |
| `registry` | 本机「已发现节点」登记表（JSON，保留原始 Card 以向前兼容） |
| `task` | A2A 任务模型：Message / Part / Task / Artifact / StreamEvent |
| `store` | 内存任务存储 + 每任务广播 + 取消令牌；`tasks/get` 与 SSE 共享一致视图 |
| `executor` | `Executor` trait + `echo`（协议冒烟）+ `run`（执行命令、流式输出） |
| `rpc` | JSON-RPC 2.0 方法分发（同时接受 v0.3 `message/send` 与 v1 `SendMessage` 别名） |
| `client` | 极小 A2A 客户端（CLI `call` / `run` 用） |
| `server` | axum 回环服务器：Card + `/healthz` + `/a2a`（JSON-RPC 与 SSE） |
| `paths` | XDG 路径（与 `agentero-core::paths` 同布局） |

**为什么自包含而不复用 `agentero-core`**：节点要能被拷到裸服务器上单独 `cargo build`，
不能拖入桌面依赖树（rusqlite / liteparse / PDFium）。因此显式 pin 依赖版本、自带
`paths` / `error`。`identity` 的 `nodeId` 由 Ed25519 公钥指纹派生，避免碰撞。

## 4. CLI

```text
agentero-node init [--name N]                 # 生成节点身份
agentero-node card [--url URL]                # 打印本机 Agent Card
agentero-node serve [--listen A] [--allow-run] [--workdir DIR]
                                              # 回环提供 card + /healthz + /a2a
agentero-node probe <target>                  # 拉取对端 Agent Card
agentero-node add <target>                    # probe 并登记到本机 registry
agentero-node list                            # 列出已登记节点
agentero-node remove <node-id>                # 忘记节点
agentero-node call <target> [--method M] [--params JSON]
                                              # 发一条 JSON-RPC 请求
agentero-node run <target> <command> [--skill S] [--cwd DIR]
                                              # 在节点上跑命令并流式回报
```

`target` 接受基址（`http://host:8766`）或完整 Card URL。

## 5. A2A 任务端点

`POST /a2a`，JSON-RPC 2.0（同时接受 v0.3 与 v1 方法名）：

| 方法（别名） | 行为 |
|---|---|
| `message/send` / `SendMessage` | 建任务、异步执行、**立即**返回 Task 快照 |
| `message/stream` / `SendStreamingMessage` | 同上，但以 **SSE** 推 `task` → `status-update` / `artifact-update` → 终态 |
| `tasks/get` / `GetTask` | 按 `{id}` / `{taskId}` 取 Task |
| `tasks/cancel` / `CancelTask` | 取消任务（幂等） |

- **技能路由**：请求 `metadata.skill`（或 message 的 `data.skill`）选择执行器，缺省 `echo`。
  Card 的 `skills` 由执行器注册表自动生成。
- **`run` 执行**：命令取自 `data` part `{command, cwd}`，回退到 message 文本；用 `sh -c`
  执行，stdout/stderr 合流逐行作为 `working` 状态推流（上限 200 行），最终产出一个
  `output.txt` artifact（含输出与 exit code）。**默认关闭，需 `--allow-run`**。
- **`/healthz`**：`{status, nodeId, name, version}`。
- 任务状态为 `kebab-case`（`input-required` 等），任务事件字段为 `camelCase`（对齐 A2A）。

## 6. 路径约定

| 路径 | 内容 |
|---|---|
| `$XDG_CONFIG_HOME/agentero/node/identity.json` | 本机节点身份（私钥 `0600`） |
| `$XDG_CONFIG_HOME/agentero/nodes.json` | 已发现节点登记表 |

## 7. 发现 / 安装 / 通信

- **发现**：v1 手动 `add` + 从 `~/.ssh/config` 导入 + 配对码 TOFU；后续 mDNS、中心 registry。
- **安装**：形态 B 只需一个二进制；预编译分发 + 安装脚本 + systemd 为后续。
- **通信**：v1 用 **SSH 端口转发 + A2A over loopback**，复用 SSH 密钥鉴权、无需暴露端口；
  云 / NAT 场景后续走 outbound tunnel。任务队列当前在节点内存中，持久化队列为后续。

## 8. 验证（实测）

| 主机 | 架构 | 结果 |
|---|---|---|
| 本机 macOS | aarch64 | 16 单测通过；send / stream / run / cancel 全通过 |
| `dgx-remote` | aarch64 | 独立构建；`run nvidia-smi -L` → `NVIDIA GB10` |
| `a1` | x86_64 | 裸机装 rustup 后构建；`run nvidia-smi -L` → 4×`A100-PCIE-40GB` |

本机经 SSH 隧道同时向两个节点提交 A2A 任务并取回产物。

## 9. 后续

1. **M3 配对**：配对码 + 长期 Bearer token + 公钥 pinning；token 入 OS keychain；A2A 鉴权头。
2. **M3 远端 Agent**：`run` 之外的 `agent` 技能 —— 节点拉起 CLI Agent（ACP Host），
   把任务落到 Workspace（而非直接 shell）。
3. **M3 持久队列**：任务落盘 + 重启恢复 + worker 拉取（见 [A2A.md](A2A.md)）。
4. **M4 编排**：RunSet / Run 队列 + 并行调度（见并行实验设计）。
5. **集成**：桌面 / `agentero-cli` 加 `node` 子命令与「远端会话」面板。

## 10. 代码

`crates/agentero-node/`（`src/{main,cli,identity,probe,card,registry,task,store,executor,rpc,client,server,paths,error}.rs`）
