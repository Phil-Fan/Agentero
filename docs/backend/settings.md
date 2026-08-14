# 应用设置（Host）

| Command | 说明 |
|---|---|
| `settings_get` | 读全部设置 |
| `settings_set` | 合并写入并广播 `settings:changed` |

- 路径：`$XDG_CONFIG_HOME/agentero/settings.json`（macOS 通常 `~/.config/agentero/`）。
- `usageTrackingEnabled`：是否写入 XDG `usage.sqlite`（见 [usage.md](usage.md)）。与 `telemetryEnabled`（PostHog）独立。
- 旧 localStorage 键一次性迁移。
- Agent 注册表等同目录管理。

前端：[../frontend/settings.md](../frontend/settings.md)  
代码：`src-tauri/src/features/settings/`
