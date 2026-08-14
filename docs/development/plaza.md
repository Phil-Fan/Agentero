# 广场（Plaza）— 外部来源发现

> 范围：侧栏虚拟节点 **广场** 及其子来源（Cool Papers / ModelScope 论文 / 播客 / 论文推荐）；中间栏发现流。  
> 相关：[`../frontend/vault-tree.md`](../frontend/vault-tree.md)、[`../backend/paper-import.md`](../backend/paper-import.md)、[`../backend/index.md`](../backend/index.md)。

## 0. 产品结论（2026-07-25，2026-08-14 修订）

| # | 议题 | 结论 |
|---|---|---|
| Q1 | 树位置 | **Library + Recycle Bin 下方、真实 Vault 根目录上方**（已实现） |
| Q2 | Cool Papers 呈现 | **内嵌 iframe + Host 代理协议** `agentero-coolpapers://`（已实现；见 §3.2） |
| Q3 | 入库 | **已实现**：每行注入 `[入库]`，复用现成魔棒（见 §3.2.1） |
| Q4 | P0 范围 | **已交付：广场壳 + Cool Papers 浏览入库 + Skill 推荐**；播客 / 论文推荐尚未实现 |
| Q5 | ModelScope 论文 | **已实现**：同一代理模式，但站点是 SPA，另有取舍（见 §3.5） |

**已实现落点（2026-08-14）**

| 区域 | 路径 |
|---|---|
| 来源注册表 | `src/lib/plaza/sources.ts`（新增来源 = 一条数组项） |
| 中间栏 | `src/components/plaza/plaza-view.tsx`、`plaza-web-frame.tsx`、`plaza-skills-view.tsx` |
| Skill 精选 | `src/lib/plaza/skill-catalog.ts` |
| 入库 | `src/lib/plaza/import.ts`（论文 + Skill 仓库） |
| 侧栏行 | `src/components/sidebar/file-tree/tree-rows.tsx`（`PlazaRow` / `PlazaSourceRow`） |
| Tab kind | `src/lib/workspace/tabs/types.ts` 的 `"plaza"` + `doc-view.tsx` 分支 |
| 站点代理（共享管道） | `src-tauri/src/features/site_proxy.rs` |
| 站点代理（各站改写 + 注入） | `src-tauri/src/features/coolpapers/proxy.rs`、`src-tauri/src/features/modelscope_proxy.rs` |

> Kimi 解析没有走广场入库，而是作为论文侧的独立能力落在 Markdown 工具栏的
> 「获取 Cool Paper 笔记」按钮上（`paper_coolpapers_notes` → 追加 `NOTES.md`）。

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
2. **ModelScope 论文**（[modelscope.cn/papers](https://modelscope.cn/papers)）— 内嵌站点浏览；魔搭每日读论文带中文摘要与评分。  
3. **Skill 推荐** — 原生面板：按论文阅读 / 写作 / 绘图 / 复现 / 投稿精选 GitHub Skill 仓库；点卡片走魔棒 Skill 导入。  
4. **播客** — 占位，后续。  
5. **论文推荐** — P0 v0：基于本地库的轻量推荐列表（无云端上传）。

## 2. 侧栏信息架构

```
📁 VaultName
├── 📚 Library                 agentero:library
├── 🗑️ Recycle Bin             agentero:trash
├── 🌐 广场                     agentero:plaza              ← 可折叠
│   ├── ✨ Cool Papers         agentero:plaza/cool-papers
│   ├── ✨ ModelScope 论文      agentero:plaza/modelscope
│   ├── ✨ Skill 推荐           agentero:plaza/skills
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
| 父节点 | 单击 → 切换展开/收起并打开广场首页（与文件夹行相同） |
| 子节点 | 单击 → 对应来源 panel（dockview 虚拟 tab） |
| 右键 | Cool Papers：可选「在系统浏览器打开 papers.cool」；无删除/拖拽/Finder |
| 禁用 | 拖入拖出、删除、重命名、终端打开 |

**图标（建议）**

| 节点 | Lucide | en | zh-CN |
|---|---|---|---|
| 广场 | `Globe` | Plaza | 广场 |
| Cool Papers | `Flame` 或自定义标 | Cool Papers | Cool Papers |
| ModelScope 论文 | 自定义标（魔搭 favicon） | ModelScope papers | ModelScope 论文 |
| Skill 推荐 | `Sparkles` | Skill picks | Skill 推荐 |
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
- **同时改写 `window.open`**。仅改 HTML 不够：cool.js 的所有脚本式跳转都走 `window.open`（搜索、`[REL]` 相关论文、排序、feed、导出收藏、arXiv 日历共 7 处），且多数传 `_blank`；sandbox 去掉 `allow-popups` 后这些调用会被**静默丢弃**，表现为「搜索点了没反应」。补丁在 `<head>` 安装，早于 body 末尾的 cool.js。
- 统一的三档跳转规则（链接与脚本共用）：**站内页面** → 原地 `location.assign`；**Atom feed** → 交系统浏览器（面板里渲染裸 XML 没有意义）；**跨源** → 交系统浏览器。
- feed 与站内 handoff 必须**换算回上游 origin** 再交出去——系统浏览器解析不了我们的私有 scheme（消息用 `externalPath`，由面板对 `homeUrl` 求解）。
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

代理注入的桥接脚本给每行论文标题追加 `[入库]`，与站点自带的 `[PDF] [Copy] [Kimi] [REL]` 同排。点击后把 `{ id, branch, url, title }` 交给面板，结果回传给该行显示 `[已入库]`。

**分两条路，因为质量不对等：**

| 行类型 | 路线 | 理由 |
|---|---|---|
| arXiv | 现成魔棒，喂 `arxiv.org/abs/{id}` | 原生 arXiv 路径还能多拿 `arxiv_id` 与 LaTeX 源码 |
| 其余（venue） | `paper_coolpapers_import`，读该行自己的 papers.cool 页面 | 不经 Translator；见下 |

**为什么 venue 不走 Translator。** papers.cool 的论文页自带 Highwire `citation_*`（title / authors / abstract / publisher / date，**以及 `citation_pdf_url`**），实测覆盖它聚合的全部 11 种出版商形态。而把出版商 URL 送去 Translator：

- `openreview.net`（COLM / CoRL / ICLR / ICML / MLSYS / NeurIPS / UAI **共 7 个 venue**）抓到的是 Cloudflare 人机验证页，0 作者；
- `ojs.aaai.org`（AAAI）HTTP 500；
- `www.ecva.net`（ECCV）HTTP 300 多选；
- `papers.miccai.org` / `www.ndss-symposium.org` 退化成 `webpage` / `blogPost`；
- **且 11/11 都不返回 PDF 附件。**

也就是说 Translator 那条路「一半站点坏、还全都缺 PDF、又多一跳依赖用户自建服务」，唯一净胜的只有 DOI（当前不填）。详见 [#333](https://github.com/poco-ai/Agentero/issues/333)。

**catalog id 用 papers.cool 原生 id**（如 `36962@AAAI`、`2026.acl-long.1@ACL`）。`allocate_paper_path` 不清洗 id、直接当目录名，`@` `.` 三平台合法。选它而非默认派生链是因为它全局唯一、去重精确；派生链的 `citekey_fallback`（`{姓}{年}{标题首词}`）会撞，而 `DedupePolicy::ByCatalogId` 撞了会**静默当重复吞掉**。代价是同一篇论文日后从 BibTeX / Zotero 进来 id 不同、会重复——已知取舍，原生 id 另存进 `source_url` 保留可追溯性。

**其它约定**

- 复用共享的 `paper_commit`：catalog 插入、NOTES.md 播种、PDF 下载、去重全部沿用，不新增管线。
- `paper_type` 不是 Zotero itemType，取值只有 `arxiv` / `doi` / `html` / `other`；无 DOI 时为 `other`。
- 元数据解析复用 `map_zotero_item`（先拼一个 Zotero 形状的值），避免第二套字段映射。
- 注入的 `[入库]` **不带 href**，否则会被跨源链接拦截器当外链送去系统浏览器。
- 行是滚动加载的（`loadMorePapers` 追加 `.panel.paper`），除首屏遍历外挂 `MutationObserver`。
- 只做**单条入库**，不提供批量。
- 入库后**不自动打开论文**，否则会把连续浏览的用户拽出面板。
- PDF 没取到时提示「已导入（未取到 PDF）」，不谎报干净成功。

**后续（非 P0）**：批量入库、预览抽屉；DOI 可按需回补（AAAI / IJCAI 的出版商页有 `citation_doi`，`/search` 按 DOI 的元数据质量最高）。

### 3.2.2 Skill 推荐（已实现）

原生面板（不 iframe）。五类：论文阅读 / 论文写作 / 绘图 / 复现 / 投稿。目录写在 `skill-catalog.ts`（静态 star 快照）。点卡片 → `importPlazaSkillRepo` → 魔棒 `lookupSubmit` → 现有 Skill 多选安装框。角上外链单独打开 GitHub。不含 Zotero / 文献库类仓库。

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