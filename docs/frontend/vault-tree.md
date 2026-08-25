# 文件树

左侧 Vault 文件树：虚拟 Library / Recycle Bin、魔棒、paper 行操作、多选拖拽。

## 虚拟节点

| 节点 | 路径常量 | 行为 |
|---|---|---|
| Library | `agentero:library` | 打开全库表格；右键导出 BibTeX / 发现引用我的新论文 |
| Recycle Bin | `agentero:trash` | 打开回收站视图；右键清空 |
| 广场 | `agentero:plaza` | 单击切换展开/收起并打开广场首页（与文件夹行相同）；`Globe` 图标。子来源含 Cool Papers、ModelScope 论文、Skill 推荐与 **订阅**；右键来源行可隐藏（`plazaHiddenSources`），右键父节点列出全部来源逐条切换显隐 |

## 建树

- 本地：Host `vault_tree_build` **一次 IPC** 递归（`features/vault/tree.rs`）。
- 远程：`remote_list` 前端递归。
- **全量递归**：`papers/`、`notes/`、`.agents/`；论文内 `source/` **懒加载**（`childrenPending` → `vault_tree_children`）。
- 论文默认仍是叶子。若 `{paper}/attachments/` 内有文件，行上出现 chevron；展开后只列出该目录的子项（不显示 `attachments` 桶本身，也不显示 `source/` / `marks/` / `NOTES.md` 等内部文件）。
- 其它根目录（包括旧 Vault 中可能存在的 `plans/`）只 list 一层，展开再 list。
- **缺失目录**：本地 `read_dir` 失败返回空列表；远程 list 的 `NoSuchFile` 同样按空处理（`isPathMissingError`），避免删除后刷新把整棵树清空。删除成功后会先 `removeTreeNode` 乐观剪枝，再 `refreshTree`。
- 忽略：`.git`、`.venv`、`node_modules` 等（`TREE_IGNORE_NAMES`）。
- 默认只展开 `papers/` 及其一级子目录。
- 虚拟化：`@tanstack/react-virtual` 拍平窗口化；`getItemKey` 用行稳定 id，避免内联新建草稿插入/移除后按索引缓存行高留下空隙。文件/文件夹行固定为 `h-7`，论文资源操作按钮不改变行高。
- 外部工具 / CLI 导入论文时，watcher 会刷新文件树，并在 Catalog 或 `papers/` 结构变更后去抖刷新 Library 元数据；论文行标签因此可在不重开论文库的情况下从目录 ID 更新为标题/作者。

### 论文目录识别

- `papers/` 下目录的直接子项含 `NOTES.md`、`PAPER.md` 或 `source/` / `assets/` / `marks/` 时，可作为论文单元。
- 如果目录自身有索引/概览 `NOTES.md`，但其子树中还包含论文单元，则优先判定为组织目录，继续收集子论文；组织目录不会被折叠成单篇论文。
- `attachments/` 与 `source/` / `assets/` / `marks/` 一样视为论文内部目录：不单独当作论文，也不在其中再找嵌套论文。
- 文件树已经得到非空的论文目录列表后，文件路径的论文归属以该列表为准，不再把组织目录的 `NOTES.md` 通过文件名回退规则猜成论文。

## Paper 行

| 展示 | 说明 |
|---|---|
| 标签 | 默认「标题 · 作者」；`paperTreeLabelMode` 可改（展示用，不改磁盘名） |
| 排序 | `paperTreeSortMode`（标题/作者/年份/添加时间等） |
| Chevron | 仅当 `{paper}/attachments/` 非空时出现。点三角展开/收起附件；点行仍打开论文 |
| Download | 缺 PDF，或既无 TeX 也无 `PAPER.md`（`source/` 为懒壳时按其 `hasTex` 标记判定） |
| Zap | 资源齐且 `is_read === false` → paper-reader |

## 交互

| 操作 | 方式 |
|---|---|
| 新建文件/文件夹 | 右键 → 树内联命名；菜单会按实际尺寸在窗口边缘自动翻转或滚动，不会被窗口下沿截断。**远程 Vault** 的重名预检走 `vaultPathExists`（`remote_list` 父目录），不可用本机 `plugin-fs` `exists`（伪路径 `remote:<id>/…` 不在本地 scope） |
| Finder 显示 | 右键 / `⌥⌘R` |
| 终端打开 | 右键 / `⌥⌘T`（文件夹=自身，文件=父目录） |
| 删除 | 右键 / `⌘⌫` → 回收站（无确认） |
| 编辑元数据 | Paper 行右键（仅本地 Vault；查 `paperMetaByRelPath` 打开与 Library 相同的编辑对话框） |
| 多选拖拽 | ⌘/Shift + 拖到目标文件夹（内部拖动带 `application/x-agentero-vault-paths`，Composer / Library 不抢成图片或 PDF 导入）；拖动时高亮落点夹。论文单元是叶子，拖到论文行 = 落到它的父目录（`dropDirFor`）；两端都在 `papers/` 下走 `paper_move`，否则 `wiki_move` |
| 外部 PDF | 拖到 `papers/` 组织夹，或拖到中间栏 Library 表（[#309](https://github.com/poco-ai/Agentero/issues/309)） |
| 折叠 | `⌘←` 选中夹；`⇧⌘←` 折叠至默认 |
| 定位 | 激活文档变化时展开祖先并 `scrollToIndex`；同一目标只定位一次，导入后台阶段引起的树刷新不再重复滚动 |
| 刷新 | File → Refresh（`⌘R`）；watcher 局部刷新 |

### 拖拽的平台差异

树内拖拽必须**同时**保留 DOM 与 Tauri 原生两条路径（[#353](https://github.com/poco-ai/Agentero/issues/353)）：

- **macOS 只走原生事件**。wry 子类化 WKWebView 并接管 `NSDraggingDestination`（wry `src/wkwebview/drag_drop.rs`），只有当 handler 返回 `false` 时才 `msg_send![super(...)]` 交还给 WebView；而 Tauri 的 handler 恒返回 `true`（tauri-runtime-wry `src/lib.rs`）。于是 `super` 永不调用，页面收不到 DOM `dragover`/`drop`——**即使拖拽是从树里发起的**。落点高亮与移动因此只能由 `onDragDropEvent`（enter/over/leave/drop）+ 命中测试驱动。macOS 版也不区分是否文件拖拽，非文件拖拽只是 `paths` 为空。
- **时序陷阱**：WebKit 在拖拽*源*一侧发的 `dragend` 早于 wry 投递的原生 `drop`。若 `dragend` 直接清空拖拽状态，随后到达的 drop 就找不到路径、静默失败。`use-tree-drag-drop` 因此用 `dragPathsRef` + 到期时间保留路径一小段宽限期。
- **Windows / Linux 只走 DOM 事件**。webview2 的 `DragEnter` 拿不到 `CF_HDROP`（真实文件列表）就直接返回，后续 `DragOver`/`Drop` 全部空转；webkitgtk 的 drop 也需要 URI-list。页面内拖拽在这两个平台不产生 Tauri 事件，由 `handleRowDragOver` / `handleRowDrop` 正常处理。

## 代码

- UI：`src/components/sidebar/file-tree/`（barrel `index.ts`；`file-tree.tsx` 仅装配，行/菜单/输入/选中条/虚拟列表为独立子模块）、AI Elements `FileTree`
- 树内状态：`src/components/sidebar/file-tree/hooks/`（`use-tree-model` 路径索引与扁平行、`use-tree-expansion` 展开与懒加载、`use-tree-selection` 多选、`use-tree-reveal` 虚拟化与定位、`use-tree-drag-drop`、`use-paper-row-actions`、`use-tree-context-menu`、`use-move-picker`）
- 逻辑：`src/lib/vault/`（store、tree、fs-watch、reveal）
- 附件：`src/lib/paper/attachments.ts`、`paths.ts`（`attachments/` 约定）
- 标签/排序：`src/lib/paper/tree-label.ts`、`tree-modes.ts`

## 开发注意

纯浏览器 `pnpm dev` 时本地盘 IO 受限；完整文件树/读写需 `pnpm tauri dev`。
