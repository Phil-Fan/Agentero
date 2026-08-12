# 翻译

应用级可插拔翻译：免费 MT + 商用 BYOK + BYOA Agent。

## 设置

Settings → **翻译**：

- **默认服务** 下拉：免费 MT 与 Agent 始终可选；商用仅列出已配置者。打开下拉时对免费 MT 与已配置商用并行 probe。
- 目标语言、划词自动翻译。
- **商用 API** 卡片仅填写 key / endpoint / region / model；点「确定」后：
  - 将 API key 写入 Host `settings.json`（Unix 权限 `0600`）；WebView 只保留同长度 `*` 掩码，不再回显明文。
  - Host `settings_get` / `settings:changed` 对 key 按字符 redact 为 `*`；`settings_set` 收到纯 `*` 串时保留原密钥。
  - `translate_text` 在 key 缺省或为 `*` 掩码时从 Host 配置解析真实密钥。
  - 随后做一次连通性 probe。卡片不承担「设为默认」选择。
- 默认服务为 Agent 时展示 Agent / 模型座。

## 消费方

- PDF 划词菜单「翻译」（首要入口）。
  - 结果卡贴合选区锚点（`trackPin`），PDF 滚轮滚动时随页重定位。
  - 翻译完成后若未悬停结果卡 / 原文黄高亮 / 页边针，约 1s 后自动收起；流式输出期间保持可见。隐藏后仍可从页边针重新打开。
- PDF **全文翻译**（工具栏 Languages，在视觉批注旁）：
  - 依赖版面分析 + PDF 文字层；翻译 `text` / `abstract` / `header` / `figure_title`（图题·表题）区域（score ≥ 30%）。
  - **不翻译**：算法框及其内部文字；`reference` / `reference_content` 文献条目；“References / Bibliography / 参考文献” 标题；侧栏 `aside_text`。
  - 按阅读顺序启动，并发 2；**每块完成立刻**在 bbox 上盖译文层（非整页等齐）。
  - 每页纸张右上角外侧常驻窄页签可只翻译本页；页签 hover 不弹出额外文字；本页已有可见译文时，页签切换为隐藏本页译文。隐藏只影响当前 UI 覆盖层，不删除磁盘缓存。
  - 译文按论文写入 `{paper}/source/layout-translate.json`。缓存命中需匹配 provider / 源语言 / 目标语言 / 非密钥服务配置，并逐块校验 region id + 原文；版面或目标语言变化时只复用仍匹配的块。
  - 单页翻译写缓存时按同一 cache key 增量合并，避免只翻译一页时覆盖其它页已经落盘的译文。
  - 运行中再点=停止；有译文再点=清除。实现：`layout-translate.ts` + `layout-translate-overlay.tsx`。
  - 覆盖层按浅色纸面绘制（白底深字）；PDF 暗色模式下套用与页面栅格相同的 invert filter（`PDF_PAGE_RASTER_DARK_CLASS`），使盖住原文的底色与反转后的纸面一致。
- API：`runTranslate(task)`（`src/lib/translate/`）。

## 路径

| 类型 | 路径 |
|---|---|
| 免费 MT | Host `translate_text`（腾讯交互翻译 / 火山 Web / DeepLX / Google gtx） |
| 商用 BYOK | Host `translate_text`（DeepL / Azure / Google Cloud / OpenAI-compatible） |
| Agent | `agent_run_once` + 翻译 prompt；同一篇文献的多次翻译复用同一个 ACP provider session |

结果可写入 `marks/`（划词）。Host 细节：[../backend/translate.md](../backend/translate.md)。
