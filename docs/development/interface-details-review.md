# 界面细节审计（PDF 交互面）· 已闭环

审计日期：2026-08-12 · 范围：图区改为单击批注（`feat(pdf): open visual annotation on click instead of hover dwell`）引入的交互面 + 相邻表面基线。规则编号来自 `.agents/skills/interface-details`（`<章节>-<条号>`）。

**全部 10 条已修复**，实现说明见 [../frontend/pdf.md](../frontend/pdf.md#单击视觉批注)。本文保留两样长期有用的东西：**回归护栏**（哪些表面已达标，别改坏）与**自查清单**（下次做类似改动照着过一遍）。

---

## 修复记录

| 编号 | 规则 | 问题 | 修复 |
|---|---|---|---|
| P1-1 | `interactivity-21` | 命中框绑 `onClick`，浏览器只要 down/up 同元素就派发 click，起手在图区内的选字/平移会误开裁剪卡 | 重建 `click-guard.ts`：6px 位移容差，`detail === 0`（键盘）直接放行；含 6 条单测 |
| P1-2 | `accessibility-1` | 键盘 Tab 到命中框只有半透明 UA 焦点环压在不可预测页面内容上；提示 chip 只挂 `group-hover` | 描边与 chip 同时响应 `group-focus-visible`；公式命中框补 `onFocus`/`onBlur` 使其键盘可达 |
| P1-3 | `typography-2` | chip 无 `max-w`、容器无 `overflow-hidden`，小区域/低缩放下溢出压住邻近内容 | 低于 `LAYOUT_HINT_MIN_REGION_W/H_PX`（120×28）不渲染，另加 `max-w` + `truncate` 兜底 |
| P1-4 | 决策门第 1 条 | 单击按 bbox 精确裁图，但 hover 只有 5% 色块，提交前看不清边界 | hover / focus 画 `border-primary/45` 描边，复用草稿框视觉语言 |
| P1-5 | `copywriting-7` | `visualCropPending` 只用于 gate 框选层，裁剪期间界面完全静止 | `usePdfRegionFraming` 暴露 `visualCropRegion`，页上画描边 + spinner（`role="status"` + `pdfExplain.cropping`）；覆盖单击与框选两条路径 |
| P2-1 | `copywriting-2` | aria 用「标注」，产品术语是「批注」 | 统一为「批注」 |
| P2-2 | `copywriting-1` | 文案罗列「插图、表格或算法」，而 `region.kind` 已知 | aria 改 `批注此{{kind}}`，kind 走既有 `layoutKindI18nKey` |
| P3-1 | `accessibility-7` | 高亮色板命中区 16px，低于桌面 24px 下限 | 按钮扩到 24px flex 容器，视觉圆点仍 16px（hover 缩放移到 `group-hover`） |
| P3-2 | `typography-5` | 全应用无 `::selection` | 加品牌选中色（`--primary` 26%）；EmbedPDF 自绘 PDF 选区 div，不受影响 |
| P3-3 | `accessibility-2` | 全仓库无 `prefers-reduced-motion` | 补全局兜底块，并加 `.animate-spin` 例外——冻结的 spinner 读起来像卡死，而非进度（见 `feat(a11y)` / `fix(a11y)` 两个提交） |

`cursor-pointer` → `cursor-crosshair`（`interactivity-4`：pointer 光标留给会跳转的引用链接）随 P1-4 一并调整。

---

## 回归护栏（已达标，勿改坏）

| 表面 | 规则 | 现状 |
|---|---|---|
| `src/index.css` 滚动条块 | `design-6` | 自动隐藏细滚动条，滚动 / hover 时才浮现 |
| `src/index.css` | `interactivity-15` | 嵌套滚动容器 `overscroll-behavior: contain` |
| `src/index.css` | `accessibility-2` | 全局 reduced-motion 兜底；**`.animate-spin` 是有意的例外，不要一并压掉** |
| `use-pdf-region-framing.ts` | `interactivity-21` | `visualCropPendingRef` 早退，重复点击不重复裁剪 |
| `selection-card.tsx` header | `accessibility-1`、`accessibility-7` | 24px 命中区 + `focus-visible:ring-2` + Tooltip + `aria-label` |
| `use-ime-guard.ts` | `accessibility-13` | 中文输入法组字期间 Enter 不误发 |
| `citation-links.tsx` | `interactivity-4` | 只有会跳转的引用链接用 pointer 光标 |
| `page-layers.tsx` 命中框 | `interactivity-21`、`accessibility-1` | 6px 拖拽容差 + hover/focus 描边 + chip 尺寸阈值 |

---

## 自查清单

做交互改动时逐条过：

1. 动作绑在 `onClick` 上、且铺在**可拖动**表面上吗？→ 需要移动容差（浏览器不替你区分点击和拖拽）。
2. 把鼠标推到屏幕外，只用键盘走一遍。任何一次「不知道焦点在哪」都是 `accessibility-1` 缺陷。
3. 固定字号的标签，塞进**最小**的那个容器还放得下吗？（容器随缩放变，标签不变。）
4. 动作的后果由某个几何范围决定吗？范围在动作**之前**可见吗？
5. 异步超过约 100ms 吗？中间有可见的「已收到、在处理」吗？
6. 文案里的名词，和产品其他地方是同一个词吗？（搜同义词比条数，少的那个是漏改的。）
7. 文案在罗列「A、B 或 C」，而代码其实知道是哪个吗？
8. 命中区到 24px 了吗（移动端 44px）？看起来小、点起来宽容才对。
9. 加了 loading 动画吗？在 reduced-motion 下它会不会被冻成「假死」？
