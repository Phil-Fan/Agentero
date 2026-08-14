# 使用记录（XDG usage.sqlite）

设备本地活动日志：打开论文、下载、入库、搜索、Agent、翻译等。**不在 Vault 内**，不随远程 catalog 镜像，也不进文件监听。

## 位置

`$XDG_DATA_HOME/agentero/usage.sqlite`

| 平台 | 未设 env 时的默认 |
|---|---|
| macOS / Linux | `~/.local/share/agentero/usage.sqlite` |
| Windows | `%APPDATA%\agentero\usage.sqlite`（`dirs::data_dir`） |

代码：`src-tauri/src/features/usage/`。前端入口 `src/lib/activity/track.ts`（`track()` 缓冲批量上报）。

## 开关

`AppSettings.usageTrackingEnabled`（设置 → 通用 → 隐私）。关闭后 Host 命令直接返回 0、不写库。与 PostHog `telemetryEnabled` 独立。

设置页可一键清除全部记录。

## Schema

独立 `schema_meta` + `SCHEMA_VERSION`（现为 1）。WAL、`busy_timeout`、`foreign_keys` 与 catalog 相同。

```sql
usage_events  -- append-only
  id, ts, vault, kind, path, mode, dur_ms, extra

usage_daily   -- 同事务 upsert
  PRIMARY KEY (day, vault, kind, path)
```

- `vault`：打开该 Vault 时的绝对路径（一台机器可有多个库）。
- `path`：Vault 相对路径。
- 打开库时 prune：事件 180 天，日聚合 2 年。
- `paper_move` / `wiki_move` 成功后 best-effort 改写 path 前缀。

## 命令

| Command | 说明 |
|---|---|
| `activity_record_events` | 批量写入（前端缓冲后调用） |
| `usage_list` | 按 vault / kind / path / since 倒序列出 |
| `usage_summary` | 按 kind 计数 |
| `usage_clear` | 清空全部或指定 vault |

## CLI

```bash
agentero usage which --json
agentero usage timeline --days 30 --json
agentero usage summary --days 30 --json
agentero usage timeline --kind paper.open --path papers/xxx --json
agentero usage clear -y          # 当前 --vault
agentero usage clear --all -y    # 本机全部
```

未加 `--all-vaults` 时 timeline / summary 过滤当前 Vault。

## 前端漏斗

`track(kind, payload)` 是唯一入口。5s / 满 50 条 / 窗口 blur 刷新。同一 `(kind, path, mode)` 1s 去重。

已接线：`paper.open` / `note.open` / `paper.focus|blur|session`、`asset.download`、`paper.import`、`skill.install`、`search.query`、`agent.run`、`paper.tag`、`paper.read`、`vault.open`、`onboarding.complete`。

翻译、版面、批注等其余 kind 已登记，漏斗按 [`../development/usage-analytics.md`](../development/usage-analytics.md) 继续接。

## 隐私

- 本地可含路径与搜索词；不上传。
- PostHog 仍只走 `features/telemetry` 的匿名启动/退出，本库不直接出站。
