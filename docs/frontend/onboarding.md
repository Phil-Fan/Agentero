# 新手引导（First-run Onboarding）

首次运行的设置向导，Raycast 风格的全窗口多步流程，主窗口渲染。

## 触发时机

- **自动**：主窗口（Tauri 桌面）首次启动，且 `settings.onboardingDone === false`、无已打开 Vault、无最近 Vault 记录时，覆盖层自动打开。老用户升级因已有 Vault/最近记录不会误弹。
- **手动**：设置 → 关于 → 首次运行引导 →「重新打开引导向导」。设置窗口通过 Tauri 事件 `onboarding:request`（`src/lib/onboarding/api.ts`）广播，主窗口 `OnboardingRoot` 监听后强制打开。

完成任一收尾动作（创建 Vault / 从 Zotero 导入 / 完成 / 关闭）都会把 `onboardingDone` 置 `true`（随 `settings.json` 持久化），此后不再自动弹出。

## 步骤

| 步骤 | id | 内容 | 复用 |
|---|---|---|---|
| 欢迎 | `welcome` | 品牌 + 价值主张 + 特性 | — |
| 外观 | `theme` | 明暗模式 + tweakcn 配色主题即时预览 | `patchSettings` + `applyUiTheme` / `next-themes` |
| Agent | `agent` | 扫描本机 ACP Agent、探测、设默认（可跳过） | `scanCatalog` / `probeCatalogAgent` / `ensureCatalogAgent`（`src/lib/agent/api.ts`） |
| 翻译 | `translate` | 直接内嵌 `TranslatePane` | 设置窗口同一组件，配置即落盘 |
| 版面解析 | `layout` | 直接内嵌 `LayoutPane` | 设置窗口同一组件，配置即落盘 |
| 收尾 | `vault` | 创建 Vault / 从 Zotero 导入 / 稍后再说 | `createNewVault()` / `migrateZoteroFromWelcome()`（`src/lib/vault/actions.ts`） |

流程状态机用 **@stepperize/react**（`defineStepper` + `useStepper`），定义见 `src/components/onboarding/flow.ts`；纯线性、无分支跳转。

## 结构

- `src/components/onboarding/flow.ts` — `defineStepper` 步骤定义。
- `src/components/onboarding/onboarding-root.tsx` — 全屏覆盖层（`fixed z-40`，低于 Radix Dialog/Select 的 `z-50`，保证向导内的下拉可弹出）、头部品牌 + 步骤圆点、底部 上一步 / 下一步 / 完成，`motion` 步骤切换动画。
- `src/components/onboarding/steps/*` — 各步骤组件。
- `src/components/onboarding/onboarding-store.ts` — 手动重开的 `forceOpen` 标志（zustand vanilla）。
- `src/lib/onboarding/api.ts` — 跨窗口 `onboarding:request` 事件。
- `src/lib/settings/*` — `AppSettings.onboardingDone` 字段（默认 `false`）。

## i18n

独立命名空间 `onboarding`：`src/i18n/locales/{en,zh-CN}/onboarding.json`。

相关代码：`src/components/onboarding/`、`src/lib/onboarding/api.ts`。
