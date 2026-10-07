# 远端 Node（Headless Agentero Node）

> 状态：**M1 + M2 + 配对鉴权 + 远端 Agent 执行器 + 任务持久化 + 队列化执行 + ACP 原生宿主 已落地** ——
> `crates/agentero-node` 提供身份 / Agent Card / 发现登记表 / 节点服务器、**A2A JSON-RPC + SSE 任务端点**
> （`message/send` / `message/stream` / `tasks/get` / `tasks/cancel` / `tasks/list`）、
> **配对码 + Bearer token 鉴权**、**`agent` 技能**（命令模板或 **ACP 会话**）、**任务落盘 + 重启恢复**，
> 以及 **worker 池 + 队列（`--max-parallel`）**。RunSet 编排与桌面 / CLI 集成为后续（见 §10）。

## 1. 背景

本地聊天、读文献，实验却在远端服务器上跑。要让「本地 UI 驱动远端 Agent」，远端需要一个
**可发现、可寻址、可承载任务**的节点。本 crate 就是那个节点工件。

## 2. 两种节点形态

| | A. SSH-only | B. Headless Node（本 crate） |
|---|---|---|
| 远端需要 | 只要 shell + 一个 CLI Agent | `agentero-node` 二进制 |
| 通信 | ACP over SSH | A2A over 隧道 |
| 能力 | 交互式聊天、单次任务 | 持久任务、队列、断线恢复、多客户端、鉴权 |
| 复杂度 | 零安装 | 一次安装 + 配对 |

形态 B 的地基（身份 / 发现 / A2A 任务 / 鉴权）已完成；形态 A 保持现状，二者不冲突。

## 3. 本 crate 的组成

`crates/agentero-node/`（tauri-free，**不依赖 `agentero-core`**）：

| 模块 | 职责 |
|---|---|
| `identity` | Ed25519 节点身份；`nodeId` = 公钥 SHA-256 指纹（hex），`shortId` = 前 12 位 |
| `probe` | 主机能力快照：hostname / os / arch / cpu / memory / GPU（`nvidia-smi`） |
| `card` | A2A 形状的 Agent Card（v0.3）+ `agentero` 扩展块；`fetch()` 拉取对端 Card |
| `registry` | 本机「已发现节点」登记表 + 已配对 token（JSON，`0600`） |
| `auth` | 配对码（一次性、10 分钟）+ 长期 Bearer token（节点侧**只存哈希**） |
| `task` | A2A 任务模型：Message / Part / Task / Artifact / StreamEvent |
| `store` | 任务存储 + 每任务广播 + 取消令牌；可选落盘（`tasks/<id>.json`），重启恢复 |
| `queue` | 内存作业队列 + 唤醒；`message/send` 入队即返回，worker 池拉取执行 |
| `executor` | `Executor` trait + `echo`（协议冒烟）+ `run`（执行命令）+ `agent`（命令模板） |
| `acp` | ACP 原生执行器：`initialize` → `session/new` → `session/prompt`，流式回消息、权限默认拒绝 |
| `rpc` | JSON-RPC 2.0 方法分发（同时接受 v0.3 `message/send` 与 v1 `SendMessage` 别名） |
| `client` | 极小 A2A 客户端（CLI `call` / `run` / `pair` 用，自动附带 token） |
| `server` | axum 回环服务器：Card + `/healthz` + `/pair` + `/a2a`（JSON-RPC 与 SSE） |
| `paths` | XDG 路径（与 `agentero-core::paths` 同布局） |

**为什么自包含而不复用 `agentero-core`**：节点要能被拷到裸服务器上单独 `cargo build`，
不能拖入桌面依赖树（rusqlite / liteparse / PDFium）。因此显式 pin 依赖版本、自带
`paths` / `error`。`identity` 的 `nodeId` 由 Ed25519 公钥指纹派生，避免碰撞。

## 4. CLI

```text
agentero-node init [--name N]                 # 生成节点身份
agentero-node card [--url URL]                # 打印本机 Agent Card
agentero-node serve [--listen A] [--allow-run] [--require-auth] [--workdir DIR] [--agent-command CMD] [--acp-command CMD] [--acp-approve] [--max-parallel N]
                                              # card + /healthz + /pair + /a2a
agentero-node probe <target>                  # 拉取对端 Agent Card
agentero-node add <target>                    # probe 并登记到本机 registry
agentero-node list                            # 列出已登记节点
agentero-node remove <node-id>                # 忘记节点
agentero-node pair <target> <code> [--name N] # 用配对码换取并保存 token
agentero-node call <target> [--method M] [--params JSON]
agentero-node run <target> <command> [--skill S] [--cwd DIR]
agentero-node tasks <target> [--limit N] [--context ID]
agentero-node batch <target> --template "..." --values "a,b,c" [--skill S] [--context ID] [--wait]
```

## 5. 鉴权（可选，`--require-auth`）

- 节点启动开启鉴权时打印一次性 **配对码**（6 位，10 分钟有效）。
- 客户端 `agentero-node pair <target> <code>` → `POST /pair` 换取长期 **Bearer token**；
  节点侧仅存 token 的 SHA-256 哈希（`tokens.json`，`0600`），客户端把 token 连同节点存入
  本机 registry（`nodes.json`，`0600`）。`call` / `run` 自动附带 token。
- `--require-auth` 关闭时（默认）不做校验，供 SSH 隧道下的本地开发使用。
- `/healthz` 报 `requireAuth` 与 `pairedClients`；`/pair` 仅需配对码；`/a2a` 需 token。
- 后续：token 入 OS keychain、公钥 pinning、token 吊销/轮换、按用途分级。

## 6. A2A 任务端点

`POST /a2a`，JSON-RPC 2.0（同时接受 v0.3 与 v1 方法名）：

| 方法（别名） | 行为 |
|---|---|
| `message/send` / `SendMessage` | 建任务、异步执行、**立即**返回 Task 快照 |
| `message/stream` / `SendStreamingMessage` | 同上，但以 **SSE** 推 `task` → `status-update` / `artifact-update` → 终态后关流 |
| `tasks/get` / `GetTask` | 按 `{id}` / `{taskId}` 取 Task |
| `tasks/cancel` / `CancelTask` | 取消任务（幂等） |
| `tasks/list` / `ListTasks` | 列出任务摘要（按时间倒序） |

- **技能路由**：请求 `metadata.skill`（或 message 的 `data.skill`）选择执行器，缺省 `echo`。
  Card 的 `skills` 由执行器注册表自动生成。
- **`run` 执行**：命令取自 `data` part `{command, cwd}`，回退到 message 文本；用 `sh -c`
  执行，stdout/stderr 合流逐行作为 `working` 状态推流（上限 200 行），最终产出一个
  `output.txt` artifact（含输出与 exit code）。**默认关闭，需 `--allow-run`**。
- **`agent` 技能（命令模板）**：`--agent-command` 提供命令模板（如 `codex exec --skip-git-repo-check
  {prompt}`）；模板中的 `{prompt}` 被 shell 引号包裹后替换，缺省追加。prompt 取自 `data`
  part 的 `prompt`，回退到 message 文本。
- **`agent` 技能（ACP 原生）**：`--acp-command "codex-acp"` 时优先走 ACP：spawn 该 ACP agent，
  `initialize` → `session/new` → `session/prompt`，把 `session/update` 的
  `agent_message_chunk` 流式写入任务状态；权限请求**默认拒绝**（`--acp-approve` 才自动批准
  首个 AllowOnce）；最终回复汇总为 `agent.txt` artifact。用 `agent-client-protocol` crate。
- **队列 + worker 池**：`message/send` / `message/stream` 只**入队即返回**（任务为 `submitted`），
  由 `--max-parallel` 个 worker 拉取执行（`working`）。排队中的任务可被 `tasks/cancel` 取消且不会执行；
  `/healthz` 报 `queued` 深度。默认 `--max-parallel 4`。
- **重启恢复**：`working` / `input-required` 的任务标记 `failed`；`submitted`（从未启动）的任务
  按记录的 skill 重新入队执行（`task.metadata.skill`）。
- **`batch` 扫描**：一个模板 + 多个 value（`--values a,b,c` 或 `--values-file`）→ 每条提交一个任务，
  共享一个 `contextId`（可用 `tasks --context` 按组查询）；`--wait` 轮询到全部终态并汇总。
  这是并行实验的最小原语（更完整的 RunSet 编排见后续）。
- 任务状态为 `kebab-case`（`input-required` 等），任务事件字段为 `camelCase`（对齐 A2A）。

## 7. 路径约定

| 路径 | 内容 |
|---|---|
| `$XDG_CONFIG_HOME/agentero/node/identity.json` | 本机节点身份（私钥 `0600`） |
| `$XDG_CONFIG_HOME/agentero/node/tokens.json` | 已配对客户端的 token 哈希（`0600`） |
| `$XDG_CONFIG_HOME/agentero/node/tasks/<id>.json` | 持久化的 A2A 任务（重启恢复） |
| `$XDG_CONFIG_HOME/agentero/nodes.json` | 已发现节点 + token 登记表（`0600`） |

## 8. 发现 / 安装 / 通信

- **发现**：v1 手动 `add` + 从 `~/.ssh/config` 导入 + 配对码 TOFU；后续 mDNS、中心 registry。
- **安装**：形态 B 只需一个二进制；预编译分发 + 安装脚本 + systemd 为后续。
- **通信**：v1 用 **SSH 端口转发 + A2A over loopback**，复用 SSH 密钥鉴权、无需暴露端口；
  云 / NAT 场景后续走 outbound tunnel。任务在内存队列中执行并落盘，重启后重新入队。

## 9. 验证（实测）

| 主机 | 架构 | 结果 |
|---|---|---|
| 本机 macOS | aarch64 | 20 单测通过；send / stream / run / cancel / 配对 / agent(模板+ACP) / 落盘恢复 / 队列并发 / batch 全通过 |
| `dgx-remote` | aarch64 | `batch` 4 值扫描（`--max-parallel 2`）与 ACP `agent` 均完成；重启后任务仍在、token 仍有效 |
| `a1` | x86_64 | `run` → 4×`A100`；命令模板与 ACP `agent` 均回显 prompt；重启后任务仍在 |

> ACP 端到端以仓库内 `tests/mock-acp.cjs`（一个最小 ACP 服务端）验证；真实
> `codex-acp` / `claude-agent-acp` 的端到端见后续。

桌面「远端会话」面板（`src/components/sessions/remote-sessions-view.tsx`，文件树虚拟节点
`agentero:sessions`）通过 `cargo check` / `clippy -D warnings` / `tsc --noEmit` / `biome`
校验；运行时交互需本地 `pnpm tauri dev` 确认。

本机经 SSH 隧道对两个节点完成：探测 → 配对换 token → 带 token 提交 A2A 任务 → 取回产物。

## 10. 后续

1. **队列增强**：优先级、资源画像（GPU/内存）、自动重跑策略。
2. **编排**：RunSet 模型（父子分组 / 聚合 / 早停 / 晋升）——当前 `batch` 只是最小扫描原语。
3. **集成**：桌面面板已落地（文件树虚拟节点「远端会话」）；后续补通知、未读角标与多节点分组增强。
4. **鉴权增强**：Keychain、公钥 pinning、token 分级/吊销。
5. **真实 ACP Agent 端到端**：用 `codex-acp` / `claude-agent-acp` 跑通（当前以 mock 验证）；
   会话续聊 / 历史 / 权限策略。

## 11. 代码

`crates/agentero-node/`（`src/{main,cli,identity,probe,card,registry,auth,task,store,executor,rpc,client,server,paths,error}.rs`）
