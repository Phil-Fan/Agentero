# 遥测（PostHog）

匿名产品分析：仅上报应用版本与设备级信息，用于版本采用率、平台分布、会话时长统计。代码在 `src-tauri/src/features/telemetry/`，仅桌面端编译。

## 开关语义

三层门控，任一不满足即整体 no-op：

1. **编译期**：构建时环境变量 `AGENTERO_POSTHOG_KEY` 注入 PostHog Project API Key；未设置（或为空）时功能完全禁用——本地 / 开源构建默认不上报。
   - Key 来源（`build.rs` 的 `forward_posthog_key()`）：显式环境变量优先；否则回退读仓库根 `.env`（gitignored）。
   - 官方发布：`release.yml` 的 tauri-action 步骤从 GitHub Secret `POSTHOG_KEY` 注入；secret 缺失时为空串，遥测编译为 no-op。
   - Ingestion host 使用 posthog-rs 默认 `https://us.i.posthog.com`（US 项目）；换 EU / 自建需改用 `ClientOptionsBuilder().host(...)`。
2. **构建类型**：debug 构建（`cfg!(debug_assertions)`，含 `pnpm tauri dev`）不上报，避免开发数据污染。
3. **用户设置**：`AppSettings.telemetry_enabled`（前端 `telemetryEnabled`，默认 `true`），在 设置 → 通用 → 隐私 中关闭，**下次启动生效**。

## 事件与字段

`distinct_id` 为持久化在 XDG 配置目录 `telemetry_id` 文件中的随机 UUID（`install_id()`），不含任何身份信息。

### `app started`（启动时，setup 后 spawn_blocking 发送）

| 属性 | 来源 |
|---|---|
| `app_version` | `env!("CARGO_PKG_VERSION")` |
| `os_name` / `os_version` | `os_info` |
| `arch` | `std::env::consts::ARCH` |
| `device_model` | macOS `sysctl hw.model` / Linux DMI / Windows 注册表（best-effort，可空） |
| `locale` | `AppSettings.locale` |
| `timezone` | 本地 UTC 偏移（如 `+08:00`） |
| `tauri_version` | `tauri::VERSION` |
| `session_id` | 本次运行生成的 UUID |

Person 属性：`$set` → `app_version` / `os_name` / `os_version` / `arch` / `device_model`；`$set_once` → `first_app_version`。

### `app exited`（`RunEvent::Exit` 回调中发送并 flush）

`session_id`、`session_duration_ms`、`app_version`。

## 隐私边界

- 不含 Vault 路径、文件名、论文标题、笔记内容、Agent 配置。
- 上报失败只记日志，绝不影响启动与退出。
- 使用 `posthog-rs` blocking client：`capture()` 非阻塞（后台批量发送），退出回调中同步 `flush()`。
