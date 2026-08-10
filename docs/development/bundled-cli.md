# 桌面 CLI 分发与 PATH 安装

> **状态（2026-08）**：M1–M2（`open` + deep link、`paper move`）与设置 → 关于安装 PATH shim 已实现。  
> **体积策略（[#285](https://github.com/poco-ai/Agentero/issues/285)）**：桌面安装包**不再**嵌入多 MB 的 `agentero-cli`；About → 安装 CLI 从 **同 tag** 的 GitHub Release 下载归档并校验 sha256。`externalBin` 仅保留 stub 以满足 tauri-build。独立 CLI 归档仍由 release CLI 矩阵上传。  
> 平台安装器自动 PATH（NSIS/deb）与 Homebrew Cask link 仍可按渠道加深。实现说明亦见 [../backend/cli.md](../backend/cli.md)。

关联 Issue：[\#165](https://github.com/poco-ai/Agentero/issues/165)（命令行打开 Vault）、[\#166](https://github.com/poco-ai/Agentero/issues/166)（CLI 论文移动）、[\#285](https://github.com/poco-ai/Agentero/issues/285)（安装包体积）。

**命令能力扩展**（翻译 / 高亮 / 批注等阅读标注进同一 `agentero` bin）：[\#170](https://github.com/poco-ai/Agentero/issues/170)，设计见 [mark-cli-roadmap.md](mark-cli-roadmap.md)。本篇只管 **分发与 `open`**；#170 管 **子命令与 marks 契约**。两者共用一个 headless 二进制，可并行开发。

## 背景与结论

桌面 App 与 CLI 共享 `agentero_lib` 中的 Vault / Catalog 领域逻辑；版本不一致会带来发现成本与契约漂移。

**分发目标（修订后）：**

1. 用户只下载安装一个桌面 artifact 即可日常使用 GUI。
2. 需要 `agentero` 时，在 **设置 → 关于** 一键安装与 App **完全同版本** 的 headless CLI（从该次 Release 下载，不拉 `latest`）。
3. 安装包体积不因 CLI 膨胀（#285）；独立 CLI 归档继续服务 headless-only 机器。

不把 GUI 可执行文件直接当作 CLI，也不复制一份 Vault/Catalog 业务逻辑。PATH 策略仍是：显式安装用户级 shim，**不**静默改 shell rc。

安装后的 `agentero` 应包含 **当时 tag 上已实现的全部 headless 子命令**（含既有 `vault` / `paper` / `import` / `wiki`，以及 #170 落地后的 `mark` / `translate` 等）。

## 范围

### 要做

- 将 `agentero-cli` 打入每个 desktop App artifact，版本必须和 App manifest / tag 一致。
- 通过 `agentero open <PATH>` 打开本地 Vault，并支持 `agentero <PATH>` 简写。
- 保持现有 headless 命令、`--json` 输出和退出码契约。
- 让系统安装渠道把 `agentero` 暴露到 `PATH`；DMG 场景提供显式安装入口。
- 覆盖既有 `paper move` 工作流，确保目标父目录不存在时会创建且 Catalog 路径同步。
- Release 验收：内置 CLI 的 `--help` / 关键子命令（含已合并的 `mark` 等）与独立 `agentero-cli` artifact 同源、同版本。

### 不做

- 不在首次启动时静默修改 `.zshrc`、`.bashrc` 或其它 shell 配置。
- 不把远程 Vault handle、SSH 凭据放入 CLI URL。
- 不在 `open` 时覆盖用户 Vault 中的文件。既有 `vault_ensure` 的“仅补缺 / 不覆盖”语义保持不变。
- 不承诺 DMG 拖拽安装后，在所有 shell 中无条件立即可找到 `agentero`；这是 macOS PATH 与权限模型决定的。
- 不在本篇实现阅读标注语义或 PDF 定位（见 [mark-cli-roadmap.md](mark-cli-roadmap.md)）；内置 CLI **不**内嵌 EmbedPDF / BYOA。

## 用户命令契约

```bash
# 显式打开本地目录
agentero open ~/research

# 上述形式的快捷写法
agentero ~/research
agentero .

# 保持既有 headless CLI
agentero --vault ~/research paper list --json
agentero --vault ~/research paper move papers/inbox/demo papers/archive
```

解析优先级：已知子命令（`vault`、`paper`、`import` 等）先按既有 Clap CLI 解析；仅当第一个非 option 参数是单一目录路径时，才转为 `open <PATH>`。路径与命令同名或输入含义不明确时，要求使用 `agentero open <PATH>`。

`agentero` 无参数仍输出 CLI help，不隐式打开“最近 Vault”，避免脚本和终端误触发 GUI。

当前 `paper move` 已存在于 `cli/src/commands/paper.rs`，底层 `catalog::move_paper_under` 会创建目标父目录、拒绝覆盖与路径逃逸，并同步 Catalog；本工作不重新实现它，只补验收测试和文档。

## 运行链路

```text
agentero open <PATH> / agentero <PATH>
  -> 内置 headless CLI：绝对化、检查目录存在
  -> agentero://open?path=<percent-encoded-path>
  -> OS 唤起或转发给已运行的 Agentero
  -> Tauri deep-link handler（启动时和运行时）
  -> Rust 校验本地目录并扩展 fs scope
  -> emit("vault:open-request", canonicalPath)
  -> React 复用按路径打开 Vault 的 action
  -> activateVault + 既有树、Catalog、Wiki 初始化
```

深链只承担 GUI 打开请求；`paper`、`vault`、`tree`、`import`、`export` 等 headless 命令直接在 CLI 进程中执行。桌面端须同时处理：

1. 首次启动携带的 URL；
2. App 已运行时由系统投递的 URL；
3. 直接启动 App 可执行文件时的第二实例参数转发。

第三项采用 `tauri-plugin-single-instance`。现有 deep-link 仅声明移动端 scheme，实施时需增加 desktop 配置与 Rust handler。收到请求后应将主窗口显示、聚焦；解析失败只 Toast / CLI stderr 报错，不能切换当前 Vault。

路径应在 Host 再次验证：非空、本地目录、规范化后无无效编码。前端不得把外部 URL 参数直接用于文件系统访问。打开动作提取为 `openLocalVaultPath(path)`，供最近 Vault、文件选择器和 deep link 共用，以保证 fs scope、状态清理、最近列表和错误提示一致。

## 打包与安装

### Artifact 内容

| 产物 | CLI 策略 |
|---|---|
| 桌面安装包（DMG/MSI/deb/…） | **不**嵌入真实 CLI；`beforeBuildCommand` 仅 `pnpm cli:bundle:stub`（tauri-build 占位） |
| GitHub Release CLI 归档 | 每平台 `agentero-cli-{ver}-{host}.{tar.gz\|zip}` + `.sha256`（CLI 矩阵 job） |
| 设置 → 关于 → 安装 | Host 下载 **同 app 版本** 归档 → 校验 sha256 → 解压到用户 data 目录 → 用户 bin shim |

**仅 desktop**。iOS / Android 是远端客户端，无 headless CLI：

- `tauri.conf.json`：`bundle.externalBin = ["binaries/agentero-cli"]`（stub only），`beforeBuildCommand` 含 `pnpm cli:bundle:stub`
- `tauri.ios.conf.json` / `tauri.android.conf.json`：覆盖 `beforeBuildCommand` 为仅 `pnpm build`，并清空 `externalBin`
- 独立 CLI job / `cargo build -p agentero-cli`：须先 seed stub，否则 `tauri-build` 因 missing resource 失败
- `scripts/prepare-bundled-cli.mjs` 在 mobile 时只写 stub

外部 `agentero` 是指向 **managed** CLI 二进制的轻量 shim（下载缓存，而非 App Bundle 内文件）。App 升级后若 CLI 版本落后，About 提示 **Update** 并重新下载同版本。

### 平台策略

| 平台与安装渠道 | `agentero` 可用方式 | 备注 |
|---|---|---|
| Windows NSIS/MSI | 安装器将 CLI 所在目录加入**用户级** `Path` | 不要求管理员；卸载时恢复安装器写入项。 |
| Linux deb/rpm | 包安装 `/usr/bin/agentero` | 由包管理器维护链接与卸载。 |
| macOS Homebrew Cask | Cask link App 内 CLI 到 Homebrew bin | `brew install --cask` 后可直接使用。 |
| macOS PKG | 安装器创建 `/usr/local/bin/agentero` 链接 | 可在安装时取得授权。 |
| macOS DMG | App 内命令 `Install 'agentero' command in PATH` | 显式请求权限并报告实际安装位置。 |

DMG 的安装操作依次尝试已在 PATH 的用户目录、`~/.local/bin`，最后才请求授权写入系统目录。若目标目录不在 PATH，操作必须明确说明结果和后续动作；不得私自编辑 shell rc 文件。提供相应的卸载命令，并且只删除由 Agentero 创建且仍指向 Agentero 的 shim。

## 实施切片

### M1：CLI 契约与测试（#166 收口）

- 为 `paper move` 增加 CLI 集成测试：目标父目录不存在、Catalog path 更新、目标冲突、越界输入。
- 更新 `docs/backend/cli.md`，说明 `move` 的目录创建和失败行为。
- 将 CLI 入口拆出可复用的参数分发层，为 `open` 预留分支，不改变既有子命令输出。

### M2：桌面接收打开请求（#165）

- 添加 desktop deep-link 与 single-instance plugin。
- Rust 实现 URL 解析、目录校验、fs scope 和窗口聚焦。
- 前端抽取 `openLocalVaultPath`；监听 `vault:open-request`，处理启动竞态和打开失败。
- 测试启动参数、运行中 App、无效目录与 Vault 切换。

### M3：PATH 安装 UX（已实现基线）

- 设置 → 关于：安装 / 卸载 / 状态；文案经 i18n。
- 用户 bin shim；不编辑 shell rc。

### M3b：可选下载分发（#285，已实现）

- 安装包停止 `cli:bundle:release`；仅 stub。
- About 安装：同版本 GitHub 下载 + sha256 + managed 缓存 + shim。
- 版本落后时提供 Update；dev 可用 `pnpm cli:bundle` 离线安装。

### 仍可选加深

- Windows/Linux 安装器自动 PATH、Homebrew Cask link、PKG。

## 验收标准

- 桌面安装包内 **无** 可运行的多 MB `agentero-cli`（仅 stub 或无 payload）。
- 已发布 `vX.Y.Z` 上，About → 安装后 `agentero --version` 为 `X.Y.Z`。
- 安装过程校验 Release `.sha256`；失败不留下半截可执行文件。
- 卸载只删除 Agentero 管理的 shim 与 managed 缓存，不碰用户自建 `agentero`。
- `agentero <existing-directory>` 唤起已运行的 App、聚焦窗口并切换 Vault；无效路径不改变当前 Vault。
- `--json` 与 headless 退出码契约不因 GUI 打开能力而变化。

## 风险与决策

| 风险 | 决策 |
|---|---|
| macOS DMG 无法可靠自动加入 PATH | 使用显式安装命令；提供 PKG / Homebrew 作为零手动渠道。 |
| App 与 CLI 版本漂移 | 只下载 **当前 app 版本** 的 Release 资产；禁止 `latest`。 |
| Draft / 未发布 tag | 404 明确提示；与 updater 一样仅已发布 Release 可用。 |
| 安装包体积 | CLI 不进安装包（#285）；独立归档仍上传。 |
| 外部 deep link 注入任意路径 | Host 规范化并验证目录，前端仅消费 Host 发出的路径。 |
| GUI 启动与前端监听存在竞态 | Host 缓存最近一次 pending open request，前端 ready 后拉取或确认消费。 |
| 目录简写与未来子命令冲突 | 子命令优先；歧义时使用显式 `open`。 |

实现完成后，本文件应移除或改写为 `docs/backend/cli.md`、`docs/frontend/shell.md` 与发布文档中的已实现说明。
