# 应用设置（Host）

| Command | 说明 |
|---|---|
| `settings_get` | 读全部设置 |
| `settings_set` | 合并写入并广播 `settings:changed` |

- 路径：`$XDG_CONFIG_HOME/agentero/settings.json`（macOS 通常 `~/.config/agentero/`）。
- `telemetryEnabled`：是否把行为事件脱敏投影到 PostHog（见 [telemetry.md](telemetry.md)）。本地 `usage.sqlite` 记录始终开启、无开关。
- `plazaEnabled`：是否显示并加载广场（默认开）。关闭后侧栏不渲染广场节点，已开的广场 tab 关闭，且不挂载 `PlazaView`（含站点代理 iframe / 订阅轮询）。
- 旧 localStorage 键一次性迁移。
- Agent 注册表等同目录管理。

## 耦合契约（schema 无关配置层）

settings 只提供读/写/持久化/广播能力，**不 import 任何域 feature**（出边仅 `core/*`）：

- `settings_set` 只做：proxy 校验 → `store.set`（merge 密钥/normalize/原子落盘）→ 广播 `settings:changed`。
- 域侧反应通过 `AppSettingsStore::subscribe` 在 app 装配（`app/mod.rs` setup）注册，`set` 成功后以 redacted 快照触发（模式同 P2-12 JobCenter runner 注册制）：
  - agent：`set_proxy`（网络代理同步）
  - import：`refresh_parser_config`（正文解析引擎凭据快照，桌面端）
  - connector：`set_port`（端口变更重绑监听）
  - jobs：`apply_layout_backend` + `drain_and_spawn`（layout 并发上限）
- 反序列化期需要的域默认值（如 `DEFAULT_CONNECTOR_PORT`）定义在 settings，由属主域 re-export（方向 `connector → settings`，不成环）。

前端：[../frontend/settings.md](../frontend/settings.md)  
代码：`src-tauri/src/features/settings/`
