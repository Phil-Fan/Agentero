# 广场（Plaza）— 外部来源发现

> 范围：侧栏虚拟节点 **广场** 及其子来源（Cool Papers / 播客 / 论文推荐）；中间栏发现流。  
> 相关：[`../frontend/vault-tree.md`](../frontend/vault-tree.md)、[`../backend/paper-import.md`](../backend/paper-import.md)、[`roadmap.md`](roadmap.md)、[`todo.md`](todo.md)。

## 0. 产品结论（2026-07-25）

| # | 议题 | 结论 |
|---|---|---|
| Q1 | 树位置 | **Library + Recycle Bin 下方、真实 Vault 根目录上方** |
| Q2 | Cool Papers 呈现 | **内嵌 WebView** 打开 [papers.cool](https://papers.cool/) |
| Q3 | 入库 | **P0 不做入库**（不接魔棒 / `lookup_import`；后续迭代再开） |
| Q4 | P0 范围 | **Cool Papers 可用（WebView 完整浏览）+ 推荐 v0**；播客仅占位 |

## 1. 产品动机

Agentero 已是 **local-first 论文工作台**（Library + 文件树 + PDF\|NOTES）。用户还需要从 **外部发现流** 找新论文。

**广场** = 「发现入口」集合，与 **Library（已收藏）** 正交：

| | Library | 广场 |
|---|---|---|
| 数据权威 | catalog + Vault 文件 | 外部站点 / 本地启发式；**P0 不写 Vault** |
| 侧栏 | `agentero:library` | `agentero:plaza` + 子来源 |
| 中间栏 | 论文库表格 | 来源专属发现 UI |
| 典型动作 | 打开 / 标签 / 导出 | 浏览发现（入库后续再做） |

来源：

1. **Cool Papers**（[papers.cool](https://papers.cool/)）— P0：内嵌站点浏览。  
2. **播客** — 占位，后续。  
3. **论文推荐** — P0 v0：基于本地库的轻量推荐列表（无云端上传）。

## 2. 侧栏信息架构

```
📁 VaultName
├── 📚 Library                 agentero:library
├── 🗑️ Recycle Bin             agentero:trash
├── 🌐 广场                     agentero:plaza              ← 可折叠
│   ├── ✨ Cool Papers         agentero:plaza/cool-papers
│   ├── 🎙️ 播客                 agentero:plaza/podcasts      ← 占位
│   └── 🧭 推荐                 agentero:plaza/recommend
├── papers/
├── notes/
└── …
```

| 项 | 约定 |
|---|---|
| 路径 | `agentero:plaza`、`agentero:plaza/<sourceId>`；**永不落盘** |
| 位置 | Library 与 Recycle Bin **之下**，真实根目录 **之上** |
| 父节点 | 展开/折叠；单击 → 广场首页（三来源卡片） |
| 子节点 | 单击 → 对应来源 panel（dockview 虚拟 tab） |
| 右键 | Cool Papers：可选「在系统浏览器打开 papers.cool」；无删除/拖拽/Finder |
| 禁用 | 拖入拖出、删除、重命名、终端打开 |

**图标（建议）**

| 节点 | Lucide | en | zh-CN |
|---|---|---|---|
| 广场 | `Sparkles` | Plaza | 广场 |
| Cool Papers | `Flame` 或自定义标 | Cool Papers | Cool Papers |
| 播客 | `Podcast` | Podcasts | 播客 |
| 推荐 | `Compass` | For You | 推荐 |

i18n：`sidebar:plaza.*`。

## 3. 中间栏呈现

### 3.1 壳：`PlazaView`

- dockview：`kind: "plaza"`，`path` = 虚拟 URI；同一 path 单实例 `activatePanel`。  
- **无**独立应用顶栏（与 Library 一致）；来源工具条做在内容区内。  
- 父路径 `agentero:plaza`：三张来源卡片（Cool Papers 可进；播客「即将推出」；推荐可进 v0）。

```
┌──────────────────────────────────────────────────────┐
│  广场                                                 │
│  ┌────────────┐ ┌────────────┐ ┌────────────┐        │
│  │ Cool Papers│ │ 播客       │ │ 推荐       │        │
│  │ 打开日更流  │ │ 即将推出   │ │ 基于本地库  │        │
│  └────────────┘ └────────────┘ └────────────┘        │
└──────────────────────────────────────────────────────┘
```

### 3.2 Cool Papers（P0，WebView）

**主内容**：内嵌 **WebView / 受控 iframe 等价物**（Tauri 下优先 `webview` 或桌面 WebView 面板）加载 `https://papers.cool/`（或可配置起始分区 URL）。

| 区域 | 行为 |
|---|---|
| 主体 | 全高 WebView；站点内导航、分区、搜索均由 papers.cool 负责 |
| 顶条（Agentero chrome） | 后退 / 前进 / 刷新 / 主页（papers.cool）/「系统浏览器打开」 |
| 地址 | 可选显示当前 URL（只读）；不暴露任意网址栏防滥用 |
| 加载失败 | 空态 + 重试 + 外链打开 |
| 入库 | **P0 不做**（顶条不放「加入 Library」；文档标明后续迭代） |

**工程注意（Tauri）**：

- 使用应用内 webview 面板，而非无约束打开外部浏览器标签（外链为次要出口）。  
- Cookie / 第三方脚本：最小权限；不注入用户 API Key。  
- 与 PDF iframe 类似：若影响 dockview 拖拽，在拖拽期间 `pointer-events: none`（参考 HTML viewer 策略）。  
- 远程 Vault 会话下同样可用（广场不依赖 vault 文件 IO）。

**后续（非 P0）**：从 URL 解析 arXiv id → 预览抽屉 / 批量入库；届时再增加 Agentero 侧列表层或桥接脚本（需合规评估）。

### 3.3 播客（占位）

- 空态文案：订阅源、单集列表与播放将在后续版本提供。  
- 侧栏子节点可点，进入占位页（避免「死链」）。

### 3.4 论文推荐 v0（P0）

**目标**：不依赖外部账号，仅用 **本地 catalog + 轻量信号** 给出可解释的「下一步可读」。

| 信号（v0） | 用法 |
|---|---|
| 最近打开 / 最近入库 | 「继续阅读」「新入库未读」 |
| `is_read === false` 且资源齐全 | 「待精读」 |
| 同 tag 聚类 | 「与标签 X 相关的本地论文」 |
| 阅读热力为空但已打开 | 「打开过但几乎未标注」 |

**呈现**：分组列表（非 WebView）—

```
推荐
├── 待精读（未读且有 PDF/TeX）
├── 最近入库
└── 标签「…」下的其它论文
```

- 行点击 → **`openPaper`**（本地 PDF\|NOTES），因为条目已在库。  
- 无足够本地数据：空态引导「用魔棒或 Cool Papers 发现论文，入库后这里会更有用」（Cool Papers 暂不入库时，引导魔棒 / 手动）。  
- **隐私**：v0 **不上传**库内容到云端；仅本地计算。

> 说明：P0 推荐是「本地库导读」，不是 Cool Papers 式外网发现。外网推荐留待入库打通之后。

## 4. 与其它模块

| 模块 | 关系 |
|---|---|
| Library | 推荐 v0 只读 `paper_list` / 热力；不改 catalog schema |
| 魔棒 / 入库 | **P0 不接**；P1 再复用 `lookup_import` |
| PDF\|NOTES | 推荐打开本地论文时走现有阅读布局 |
| Agent | P0 不强制；P1 可做「解释为何推荐」 |
| 命令面板 | P1：`Plaza: Cool Papers` 等 |

## 5. 虚拟路径与类型草图

```ts
export const PLAZA_VIRTUAL_PATH = "agentero:plaza";

export const PLAZA_SOURCE_PATHS = {
  coolPapers: "agentero:plaza/cool-papers",
  podcasts: "agentero:plaza/podcasts",
  recommend: "agentero:plaza/recommend",
} as const;

export function isPlazaVirtualPath(path: string | null | undefined): boolean {
  return Boolean(path?.startsWith("agentero:plaza"));
}
```

DocTab：`kind: "plaza"`（或 `file` + mode `plaza` + path 虚拟 URI——实现时与 Library/Trash 对齐选一种，**推荐独立 kind** 便于 `DocView` 分支）。

## 6. 分阶段

| 阶段 | 交付 | 验收 |
|---|---|---|
| **P0a 壳** | 侧栏广场 + 三子节点；`PlazaView` 首页 + 路由 | 虚拟 path 不写盘；i18n；折叠位置正确 |
| **P0b Cool Papers** | WebView 浏览 papers.cool + 导航 chrome + 外链 | 可分区浏览站点；失败可恢复 |
| **P0c 推荐 v0** | 本地启发式分组列表 + openPaper | 有库时有分组；无库时空态 |
| **P0d 播客** | 占位页 | 可进入、文案清晰 |
| **P1** | 入库（解析 arXiv / 魔棒管线）、预览抽屉、批量加入 Library | 与魔棒语义一致 |
| **P2** | 播客实体、Agent 推荐、命令面板、@ 广场条目 | — |

## 7. 明确不做（P0）

- 广场 → Vault **入库**（含批量）。  
- 把 feed 写入 catalog。  
- 播客播放器 / 订阅管理。  
- 云端协同过滤或上传本地库。  
- 在 WebView 内注入支付/登录 Agentero 账号。

## 8. 实现落点（编码时）

| 区域 | 路径 |
|---|---|
| 设计 | `docs/development/plaza.md`（本文） |
| UI 规范摘录 | `docs/frontend/shell.md` § 广场 |
| 虚拟 path | `src/lib/paper/api.ts` 或 `src/lib/plaza/` |
| 文件树 | `src/components/sidebar/file-tree/` |
| 中间栏 | `src/components/plaza/*` + `doc-view` |
| Cool Papers WebView | `src/components/plaza/cool-papers-view.tsx`（Tauri webview 封装） |
| 推荐 | `src/lib/plaza/recommend.ts` + `src/components/plaza/recommend-view.tsx` |
| i18n | `sidebar` / 独立 `plaza` ns |
| Roadmap / Todo | 增加「广场 P0」条目 |

## 9. 风险与后续确认点

| 风险 | 缓解 |
|---|---|
| papers.cool 改版 / 禁止嵌入 | 检测 `X-Frame-Options`；失败则全页降级为「系统浏览器打开」 |
| WebView 体积与内存 | 仅在 plaza cool-papers panel 挂载；关 tab 销毁 |
| 推荐过冷启动 | 空态文案；阈值阈值（如 &lt; 3 篇不估标签组） |

**仍可再确认（非阻塞 P0a）**：

- Cool Papers 默认落地 URL（首页 vs 某默认分区如 `cs.AI`）。  
- 推荐 v0 是否显示「打开过的非 paper 笔记」——默认 **否**，仅 paper 单元。

---

*修订：2026-07-25 — 采纳 WebView、不做入库、P0 含推荐 v0、树位置在 Library/Trash 下。*