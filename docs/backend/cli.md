# CLI（`agentero`）

Headless Vault / Catalog / Wiki 接口；**不含** BYOA / paper-reader。

## 位置

- 目录：`cli/`（crate `agentero-cli`）
- path 依赖 `agentero_lib`：`features::{vault,catalog,import,wiki}` + `core::{error,fs}`
- 可选同版本 CLI 安装（不随桌面安装包打入，减小体积 [#285](https://github.com/poco-ai/Agentero/issues/285)；open/deep-link 仍见 [#165](https://github.com/poco-ai/Agentero/issues/165) / [#166](https://github.com/poco-ai/Agentero/issues/166)）
  - 设置 → 关于：**安装 CLI** 从 GitHub Release 下载与 App **同版本** 的 `agentero-cli-{ver}-{triple}` 归档，校验 `.sha256` 后写入用户目录并创建 PATH shim。POSIX 写 `~/.local/bin/agentero` 软链（不静默改 shell rc）；Windows 写 `agentero-cli.cmd` 并**自动把安装目录加入用户 PATH**（`HKCU\Environment`，广播 `WM_SETTINGCHANGE`），新开终端即可用 `agentero-cli`
  - 独立 CLI 归档仍随每次 Release 发布，供无桌面的 headless 机器使用；macOS 亦可通过 Homebrew tap `poco-ai/agentero` 安装 headless CLI

## 命令组

| 组 | 用途 |
|---|---|
| `open` | 在桌面 App 打开本地目录为 Vault（`agentero open <PATH>`；简写 `agentero <PATH>`） |
| `vault` | create / which / info 等 |
| `tree` | 列树 |
| `paper` | list/get、tag list/set/add/rm、move、download/parse… |
| `trash` | list / restore / purge 本地回收站 |
| `import` | 标识符入库 |
| `export` | 导出 |
| `config` | 配置 |
| `wiki` | 只读双链语义检查 |
| `doctor` | 聚合诊断与显式确认的论文 aliases / 视觉批注格式修复 |
| `layout` | 侧栏同构版面索引：`list` / `get`（figure / table / algorithm / formula） |
| `mark` | 阅读标注：`list` / `get` / `add --region` / `delete`（区域锚点优先） |

稳定 `--json` 输出，供脚本与外部 Agent 组合。

### 版面索引与区域批注（已实现）

侧栏 Figures 同源列表落在 `{paper}/source/layout-index.json`（由桌面版面分析在 merge 后写入；raw 仍为 `source/layout.json`）。

```bash
# 列出图 / 表 / 算法 / 公式（--kind 可重复，OR）
agentero layout list papers/demo --json
agentero layout list papers/demo --kind figure --kind formula --json
agentero layout get  papers/demo figure-3 --json

# 按区域钉批注（geometry=resolved，拷贝 bbox；不写 annotations.json）
agentero mark add papers/demo --region figure-3 --comment "核心图" --json
agentero mark add papers/demo --region formula-p3-… --question "推导？" --json
agentero mark list papers/demo --json
agentero mark delete papers/demo <id> -y --json
```

Mark id 是 nanoid，字母表含 `-`，约 1/64 的 id 以 `-` 开头。`mark get` / `mark delete` 的 id 位置参数按 `allow_hyphen_values` 接收，无需 `--` 分隔。

| `--kind`（layout list） | 含义 |
|---|---|
| `figure` | 侧栏插图分区（image + chart） |
| `image` / `chart` / `table` / `algorithm` / `formula` | 精确 kind |

无 `layout-index.json` 时返回 `layout_index_missing`（提示先在 App 打开论文跑版面分析）。  
正文句子高亮 / `translate` 命令见规划 [#170](https://github.com/poco-ai/Agentero/issues/170) 与 [mark-cli-roadmap.md](../development/mark-cli-roadmap.md)。

```bash
# CLI 以 `default-features = false` 依赖 `agentero_lib`，headless 构建不走 tauri-build，无需 externalBin 占位。
# 但桌面 `pnpm tauri dev` / `pnpm tauri build` 仍要求 src-tauri/binaries 存在，首次/干净树需先占位：
pnpm cli:bundle:stub   # 或 pnpm cli:bundle
cargo build -p agentero-cli
cargo run -p agentero-cli -- vault which --json
cargo run -p agentero-cli -- wiki check papers/demo/NOTES.md --json
cargo run -p agentero-cli -- doctor --json
cargo run -p agentero-cli -- layout list papers/demo --json
cargo test -p agentero-cli
```

## 论文与 Tag

Tag 写入支持桌面端相同的 8 色后缀格式：

```bash
agentero paper tag add papers/demo "survey:blue"
agentero paper tag set papers/demo "nlp:green" "must-read:orange"
```

只有合法颜色后缀会被解析为颜色；例如 `owner:alice` 仍是普通 Tag 名称。

`@zotero:` 是 Connector 内部标签，默认不参与论文列表筛选和 Tag 汇总；需要包含它们时传 `--all`：

```bash
agentero paper list --tag topic
agentero paper list --tag "@zotero:imported" --all
agentero paper tag list --all
```

`paper delete` 默认移入可恢复回收站；明确传 `--files` 才会物理删除。回收站操作：

```bash
agentero trash list
agentero trash restore <batch-id> <stored>
agentero -y trash purge <batch-id> <stored>
agentero -y trash purge
```

论文移动会更新文件夹和 Catalog 路径。目标父目录不存在时会自动创建；目标已存在或路径逃出 `papers/` 时失败且不改 Catalog：

```bash
agentero paper move papers/inbox/demo papers/archive
# 目标父目录可尚未存在：
agentero paper move papers/inbox/demo papers/new-shelf
```

### 从命令行打开桌面 App

```bash
agentero open ~/research
agentero ~/research    # 路径简写（已知子命令名优先）
agentero .             # 当前目录
```

CLI 通过 `agentero://open?path=…` 深链唤起已安装的桌面 App；无参数时仍打印 help，不会隐式打开最近 Vault。

## 双链检查

`agentero wiki check [<source>] --json` 使用桌面端导航、嵌入、反链和重命名事务共用的 `WikiIndex` resolver，不维护第二套正则解析器。

- 不传 `source`：检查整个 Vault。
- 传 Markdown 文件：只检查该文件，适合 paper-reader 写入后的局部验收。
- 传目录：检查该目录下的 Markdown。
- 输入必须是 Vault 相对路径；命令只读，不创建目标或重写来源。
- 派生正文 `PAPER.md` 保留为可链接目标和标题来源，但不作为出链来源参与检查。
- 全部解析成功时退出码为 0；发现 `missing`、`ambiguous`、`invalidFragment` 时返回非零，错误码为 `wikilink_check_failed`，报告位于 `error.details`。
- 批注双链 `[[target@id]]` / `[[target#@id]]`：按 path 解析 target，并校验 id 形态；**不**读取 `marks/` 判断 id 是否仍存在（与桌面 resolve 一致）。

报告包含 `checkedFiles`、四类状态计数，以及每个问题的 `source`、`line`、`targetRaw`、`syntax`、`embed`、`targetPath?`、`candidates` 和 `context?`。指定单文件作用域后，Vault 中其它历史坏链不会影响本次验收。

## Doctor

`agentero doctor` 只读聚合 Vault 结构、Catalog schema、双链语义、Catalog 论文 `NOTES.md` aliases，以及 `papers/**/marks/*.json` 视觉批注格式；任一错误/待修项存在时返回 `doctor_issues` 和非零退出码。诊断会尊重设置页写入的 `.agentero/doctor.json` 别名忽略列表（这些路径不计入别名错误）。

`agentero doctor fix aliases` 在 TTY 中逐篇展示已有 alias，并允许编辑生成的标题 alias / 短 alias，最后进行一次批量确认。`-y` 接受全部安全默认值；`--json` 从不提示，未同时传 `-y` 时返回 `needs_confirmation`。修复会保留已有自定义 aliases，以内容哈希做竞态检查，并作为一个可回滚批次写入。

`agentero doctor fix visual-marks -y` 将旧版 `kind: agent-trace`（扁平 agent 字段）迁移为 `kind: visual` v2（可选嵌套 `agent`），幂等；不改 id 与裁剪图路径。详见 [doctor.md](doctor.md)。

Skill 种子：`templates/vault/.agents/skills/agentero-cli/`，含两个平台变体——`SKILL.md`（POSIX，命令 `agentero`）与 `SKILL-windows.md`（Windows，命令 `agentero-cli`、PowerShell/cmd 语法）。播种时按宿主平台选择变体写入同一个 skill id，skill 目录不随平台变化。
