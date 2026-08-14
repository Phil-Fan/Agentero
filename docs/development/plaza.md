# 广场（Plaza）— 外部来源发现

> 范围：侧栏虚拟节点 **广场** 及其子来源（Cool Papers / 播客 / 论文推荐）；中间栏发现流。  
> 相关：[`../frontend/vault-tree.md`](../frontend/vault-tree.md)、[`../backend/paper-import.md`](../backend/paper-import.md)、[`../backend/index.md`](../backend/index.md)。

## 0. 产品结论（2026-07-25，2026-08-14 修订）

| # | 议题 | 结论 |
|---|---|---|
| Q1 | 树位置 | **Library + Recycle Bin 下方、真实 Vault 根目录上方**（已实现） |
| Q2 | Cool Papers 呈现 | **内嵌 iframe + Host 代理协议** `agentero-coolpapers://`（已实现；见 §3.2） |
| Q3 | 入库 | **已实现**：每行注入 `[入库]`，复用现成魔棒（见 §3.2.1） |
| Q4 | P0 范围 | **已交付：广场壳 + Cool Papers 完整浏览 + 单条入库**；播客 / 推荐尚未实现 |

**已实现落点（2026-08-14）**

| 区域 | 路径 |
|---|---|
| 来源注册表 | `src/lib/plaza/sources.ts`（新增来源 = 一条数组项） |
| 中间栏 | `src/components/plaza/plaza-view.tsx`、`plaza-web-frame.tsx` |
| 入库 | `src/lib/plaza/import.ts` |
| 侧栏行 | `src/components/sidebar/file-tree/tree-rows.tsx`（`PlazaRow` / `PlazaSourceRow`） |
| Tab kind | `src/lib/workspace/tabs/types.ts` 的 `"plaza"` + `doc-view.tsx` 分支 |
| 站点代理 | `src-tauri/src/features/coolpapers/proxy.rs` |

> Kimi 解析没有走广场入库，而是作为论文侧的独立能力落在 Paper Info 面板的
> 「获取笔记」按钮上（`paper_coolpapers_notes` → 追加 `NOTES.md`）。

## 1. 产品动机

Agentero 已是 **local-first 论文工作台**（Library + 文件树 + PDF\|NOTES）。用户还需要从 **外部发现流** 找新论文。

**广场** = 「发现入口」集合，与 **Library（已收藏）** 正交：

| | Library | 广场 |
|---|---|---|
| 数据权威 | catalog + Vault 文件 | 外部站点 / 本地启发式；**P0 不写 Vault** |
| 侧栏 | `agentero:library` | `agentero:plaza` + 子来源 |
| 中间栏 | 论文库表格 | 来源专属发现 UI |
| 典型动作 | 打开 / 标签 / 导出 | 浏览发现 + 单条入库 |

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

**主内容**：内嵌 iframe，经 Host 代理协议 `agentero-coolpapers://localhost`（Windows 为 `https://agentero-coolpapers.localhost`）加载 papers.cool。

| 区域 | 行为 |
|---|---|
| 主体 | 全高 iframe；站点内导航、分区、搜索均由 papers.cool 负责 |
| 顶条（Agentero chrome） | 后退 / 前进 / 重新载入 / 当前路径（只读）/「系统浏览器打开」 |
| 站内链接 | 在 iframe 内直接跳转 |
| 站外链接 | 交给系统浏览器（arxiv.org 等一律拒绝被嵌套） |
| 加载失败 | 代理返回 502 文案 |
| 入库 | 每行 `[入库]`，见 §3.2.1 |

**为什么要代理（`src-tauri/src/features/coolpapers/proxy.rs`）**

papers.cool 给几乎所有链接都加了 `target="_blank"`（单个分区页实测 238 处）。直接跨源嵌套时：

- 链接要么弹出独立窗口、要么静默失效——**点了没反应**；
- 跨源 iframe 的 `location` / `history` 都读不到，**无法实现后退**。

因此改为在 Host 侧以自有 scheme 转发（沿用 `arxiv_proxy.rs` 的既有模式），同源后即可改写与观测：

- `target="_blank"` → `_self`，站内链接原地跳转；
- 绝对自链接 `https://papers.cool/…` → `/…`，导航不会掉出代理；
- 注入桥接脚本：`postMessage` 上报每次导航路径（前端据此维护 Back/Forward 栈），并拦截跨源链接交给系统浏览器；
- 上游 origin 在 Rust 侧**硬编码**，避免代理退化成任意 URL 中继（SSRF）。
- **只对完整 HTML 文档注入**（首字节是 `<!doctype` / `<html`）。`togglePdf` / `toggleKimi` 用 XHR 取的是 **同样标着 `text/html` 的片段**——Kimi 解析是裸 `<p class="faq-q">`，`POST /star` 是裸计数——cool.js 直接把响应文本塞进 DOM，一旦注入就会把脚本源码和计数当文本显示出来（现象：点 PDF / Kimi 弹出一段 `<script>…</script>0`）。
- **桥接脚本在嵌套 frame 内自我禁用**。pdf.js viewer 也是完整文档，会被一并注入；那里的 `parent` 是 papers.cool 页面而非应用，消息没人收，且点击拦截会把 PDF 内的链接 `preventDefault` 掉。判定方式：读 `parent.location.href`——面板自身的父窗口是跨源的应用会抛异常，嵌套 frame 的父窗口同源可读。

**其它工程注意**

- 前端不用 iframe 自身 history：一旦跳到第三方源就再次不可读；后退改为「按记录的路径重挂载 iframe」，因此也不会污染应用自身的 session history。
- **只有 后退 / 前进 / 重新载入 可以改变 iframe 的 `key`**（用单独的 `epoch` 计数器）。若把 `key` 挂到「观测到的导航」状态上（如 history 长度 / 游标），站内每次点击都会重挂载并重新加载**挂载时那个旧路径**，表现为「点子页面闪回首页」。venue 尤其明显：`<a onclick="listVenueDetail('AAAI')">` 无 href（只做 show/hide + `pushState('/')`），但其中的年份 / 分组是真链接 `href="/venue/AAAI.2026"`。
- sandbox 去掉 `allow-popups`，确保没有链接能逃到新窗口。
- 与 PDF iframe 一样，拖拽期间置 `pointer-events: none`，否则 dockview 收不到 dragover。
- 远程 Vault 会话下同样可用（广场不依赖 vault 文件 IO）。

### 3.2.1 入库（已实现）

代理注入的桥接脚本给每行论文标题追加 `[入库]`，与站点自带的 `[PDF] [Copy] [Kimi] [REL]` 同排。点击后把 `{ id, url, title }` 交给面板，面板调用现成的 `lookupSubmit` → `lookup_import_batch`，结果再回传给该行显示 `[已入库]`。

**关键点：喂给魔棒的是每行 `#N` 序号链接的 href**，也就是上游权威页面。四种分区形态一致：

| 分区 | papers.cool id | `#N` href |
|---|---|---|
| arxiv | `2608.13558` | `arxiv.org/abs/2608.13558` |
| AAAI | `36958@AAAI` | `ojs.aaai.org/…/article/view/36958` |
| NeurIPS | `aVh9KRZdRk@OpenReview` | `openreview.net/forum?id=aVh9KRZdRk` |
| ACL | `2026.acl-long.1@ACL` | `aclanthology.org/2026.acl-long.1/` |

因此**零后端改动**：`extract_primary_identifier` 把任何 http(s) 串判为 `IdentifierKind::Url`，`translator_request` 再路由到 Translator 的 `/web`（arxiv.org 另有原生特判，归一到 abs 页）。这也是 venue 论文唯一可行的入库路径——它们既没有 arXiv id 也没有 DOI，`lookup_import_batch` 直接吃 id 是吃不下的。

- **venue 行硬依赖 Translator**（arXiv 有原生兜底，venue 没有）。Translator 不可用时必须明确报错，不能静默失败。
- 只做**单条入库**，不提供全选批量：`/web` 是逐条抓取出版商页面，批量既慢又容易被对方限流。
- 注入的 `[入库]` **不带 href**，否则会被上面的跨源链接拦截器当成外链送去系统浏览器。
- 行是滚动加载的（`loadMorePapers` 追加 `.panel.paper`），所以除首屏遍历外还挂了 `MutationObserver`。
- `lookupSubmit` 内部是 fire-and-forget 后台任务，异常不会冒泡，因此入库按钮另设超时兜底，避免永远停在 `[入库中]`；重复点击安全（Host 按 arXiv id / DOI / 归一标题去重）。
- 入库后**不自动打开论文**，否则会把正在连续浏览的用户拽出广场面板。

**后续（非 P0）**：批量入库、预览抽屉。

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
| 魔棒 / 入库 | **已复用** `lookup_import_batch`：喂上游 URL，见 §3.2.1 |
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

- 广场 → Vault **批量入库**（单条已实现，见 §3.2.1）。  
- 把 feed 写入 catalog。  
- 播客播放器 / 订阅管理。  
- 云端协同过滤或上传本地库。  
- 注入脚本只做导航上报与 `[入库]`；**不注入任何凭据 / API Key / 登录态**。

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
*修订：2026-08-14 — 改为代理协议嵌入；壳 + Cool Papers 浏览 + 单条入库已落地；推荐 / 播客未实现。*