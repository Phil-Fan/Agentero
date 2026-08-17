# 生命周期事件系统（Lifecycle Events）

> 状态：设计稿 → 分批落地中。目标：把散落在启动链、导入链、打开链里的硬编码副作用，统一为"语义事件 + 注册式 handler"，并统一全项目事件命名规范。

## 背景与现状

生命周期点在代码里已经存在，但全是硬编码顺序调用：

| 生命周期点 | 现状位置 | 问题 |
|---|---|---|
| 应用启动 | `src/main.tsx` `boot()` + `src/hooks/use-app-bootstrap.ts` | 串行硬编码，加新任务需改源头 |
| 导入论文后处理 | `src/lib/paper/import-actions.ts`（refreshTree → rebuildWiki → refreshLibrary → track → openPaper → enqueue jobs） | 最长的隐式生命周期链 |
| 打开论文 | `src/lib/workspace/actions.ts` `openPaper`/`openTab` | 无挂载点 |
| 事件机制 | 60+ 处 Tauri emit，前端 listen 分散在各 hooks | 只有单向"技术事件"，缺语义完成事件（如 paper:imported） |

## 命名规范

1. **格式**：`domain:event-name`，全 kebab-case，单冒号分隔；域名用单个单词（`job:`，不用 `job-center:`）。
2. **按语义分三类**：
   - **事实事件**（生命周期核心，动词过去式）：`paper:imported`、`vault:opened` —— "某事已发生"，唯一的 hook 挂载点
   - **进度流**：`xxx:progress` —— 仅 UI 反馈，不是 hook 点
   - **请求/指令**：`xxx:request` —— 需要响应方，不属于生命周期
3. **payload**：统一 envelope `{ vaultId, timestamp, ...data }`；paper 相关必带 `paperId`。
4. **Tauri wire 事件名 = lifecycle 事件名**，不做两套映射。
5. **前端 API**：`lifecycle.on('paper:imported', handler)`；事件类型集中定义于 `LifecycleEventMap`（单一事实来源）。

### 历史违规与清理

- ✅ `settings_window_closed` / `feature_window_closed`（snake_case 无前缀）→ `window:closed`，payload `{ kind: "settings" | "feature", view? }`
- ✅ 菜单裸 id 事件（`open_vault`、`toggle_sidebar` 等）→ `menu:invoked`，payload `{ action }`
- `job:changed` 保留为内部状态机事件，对外暴露派生的 `job:completed` / `job:failed`

## 事件清单

⭐ = 第一批（有明确消费者，现有硬编码链可迁移）；○ = 预留，有需求再加。

### app / window

| 事件 | 时机 |
|---|---|
| ⭐ `app:ready` | 前端 bootstrap 完成（store 初始化、vault 校验后）。已预埋 emit、暂无消费者 |
| ○ `app:will-quit` | 退出前（Rust `RunEvent::Exit`） |
| ○ `window:opened` / `window:closed` | 子窗口生命周期（替换 snake_case 旧事件） |

### vault

| 事件 | 时机 |
|---|---|
| ⭐ `vault:opened` | 打开/切换 vault 完成（refreshTree、refreshLibrary、seedVaultSkills、job reconcile 挂载点） |
| ○ `vault:created` | 新建 vault |
| ○ `vault:closed` | 切走/关闭前 |
| 保留 `vault:file-changed` | watcher 文件变更（已符合规范） |

### paper（论文对象）

| 事件 | 时机 |
|---|---|
| ⭐ `paper:imported` | `paper_commit` 成功（catalog 已写入、NOTES 已建）。四条导入路径（魔棒 / 本地 PDF / Zotero / Connector）统一发；`paper_download_assets` 为孤儿文件夹补建 catalog 行时也发 |
| ⭐ `paper:assets-ready` | PDF 下载 / LaTeX 解压 / PAPER.md 生成完成（异步，与 imported 分离） |
| ○ `paper:deleted` / `paper:moved` / `paper:tags-changed` / `paper:metadata-updated` | 对象变更 |

### reader（阅读会话，前端本地事件）

| 事件 | 时机 |
|---|---|
| ⭐ `paper:opened` | openPaper/openTab 完成（layout enqueue、activity 打点挂载点）。已预埋 emit、暂无消费者 |
| ○ `paper:closed` | 关 tab |
| ○ `mark:created` / `mark:deleted` | 标注增删 |
| ○ `translation:completed` | 翻译完成 |

### note / wiki

| 事件 | 时机 |
|---|---|
| ○ `note:saved` | NOTES.md 持久化 |
| ○ `wiki:rebuilt` | 双链索引重建完成 |

### job（横切汇聚点）

| 事件 | 时机 |
|---|---|
| ⭐ `job:completed` / `job:failed` | 由 `job:changed` 状态机单点派生，payload 带 `JobKind`（ParseRefs / ParseBody / LayoutAnalyze / LayoutTranslate / DownloadAssets / PageCount / WikiReindex）。已预埋 emit、暂无消费者 |

### 已符合规范、直接纳入

`settings:changed`、`connector:item-saved`、`agent:completed`、`agent:failed`、`sync:state`（后续可补 `sync:completed`）。

## 架构设计

```text
Rust 关键节点 ──emit──▶ Tauri wire 事件 ──┐
                                          ├──▶ src/lib/lifecycle/（typed bus）──▶ 注册 handler
前端本地动作（openTab 等）──emit──────────┘
```

- **`src/lib/lifecycle/`**：
  - `events.ts`：`LifecycleEventMap` 类型定义（事件名 → payload 类型）
  - `bus.ts`：极简 typed emitter（on/off/emit，无第三方依赖）
  - `tauri-bridge.ts`：集中 `listen()` wire 事件并转发进 bus；在 bootstrap 初始化一次
- **Rust 端不做 observer 抽象**：仅在关键节点（`paper_commit` 后、jobs 状态机单点、assets 完成后）直接 emit 语义事件；当前只有前端一个消费者，不过度设计。
- **顺序约束显式化**：handler 按注册顺序串行 await，不引入优先级系统，靠注册文件内的显式顺序表达。（注：openPaper 直接用导入结果里的绝对路径打开，不依赖 refreshLibrary 完成。）

## 落地批次

1. **前端 lifecycle 模块**：types + bus + bridge，bootstrap 接入
2. **Rust 语义事件**：`paper:imported`、`paper:assets-ready`、`job:completed/failed` 派生
3. **迁移硬编码链**：`vault:opened`（bootstrap）、`paper:imported` 后处理（import-actions）、`paper:opened`（openTab）
4. **命名清理**：`window:closed`、`menu:invoked`
5. 后续（本稿不含）：hook 表用户可配置（settings / `.agentero/`），动作接 JobCenter 执行

## 非目标

- 不做 Rust 内部 plugin/observer trait
- 不做用户可配置 hook（等事件层稳定后另立设计）
- 不改 `xxx:progress` / `xxx:request` 类事件的既有语义
