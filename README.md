<p align="center">
  <img src="docs/assets/hero.png" alt="Agentero" width="100%" />
</p>

<p align="center">
  <a href="https://github.com/poco-ai/agentero/stargazers"><img src="https://img.shields.io/github/stars/poco-ai/agentero?style=flat&logo=github" alt="GitHub stars" /></a>
  <a href="https://github.com/poco-ai/agentero/network/members"><img src="https://img.shields.io/github/forks/poco-ai/agentero?style=flat&logo=github" alt="GitHub forks" /></a>
  <a href="https://github.com/poco-ai/agentero/issues"><img src="https://img.shields.io/github/issues/poco-ai/agentero?style=flat" alt="GitHub issues" /></a>
  <a href="https://github.com/poco-ai/agentero/pulls"><img src="https://img.shields.io/github/issues-pr/poco-ai/agentero?style=flat" alt="GitHub pull requests" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT" /></a>
  <a href="https://github.com/poco-ai/agentero/releases"><img src="https://img.shields.io/github/v/release/poco-ai/agentero?include_prereleases&style=flat" alt="Release" /></a>
  <a href="https://agentero-docs.poco-ai.com"><img src="https://img.shields.io/badge/docs-online-5319E7?logo=mkdocs&logoColor=white" alt="Documentation" /></a>
</p>

<p align="center">
  中文 | <a href="README.en.md">English</a>
</p>

如果本项目对你有帮助，请在右上角**给个 star** 吧！

<p align="center">
  <a href="https://github.com/poco-ai/agentero/stargazers"><img src="https://github.com/user-attachments/assets/1d49b049-89ae-4992-a92d-d2411c8053b6" alt="Give me a star" width="480" /></a>
</p>


传统文献管理器对 Agent 并不友好：

- 阅读高亮和笔记被锁在单篇文件里，Agent 很难跨论文复用。
- 每次对话都要重新提供上下文，缺少稳定的本地知识地图。
- PDF 对人友好，但对 Agent 来讲不是最舒服的阅读材料。

**Agentero** 旨在构建 Agent 友好、Agent 原生的文献管理方式，探索人与 Agent 在文献管理中的协作方式。

## 功能

- **BYOA**（Bring Your Own Agent）：通过 ACP 连接本机 Agent，Agentero 不锁定具体 Agent 或模型，工作上下文留在本地 Vault。
- **Agent 原生体验**：支持划词对话、论文导入与 Skill 导入，让 Agent 参与检索、阅读与整理工作流。
- **衔接 Zotero 生态**：兼容 Zotero 生态的导入方式，支持从标识符、链接或浏览器插件保存论文。一键导入 Zotero 书库，保留标签、笔记和附件。随时导出 BibTeX / BibLaTeX，衔接 LaTeX 写作流程。
- **图表公式解析**： 论文中的图、表、公式与算法，都可以解析出来，结合上下文理解。
- **论文翻译**： 全局论文翻译。也支持划词后并排查看原文与译文，结合论文上下文统一术语。
- **双链与知识图谱**：使用 Obsidian 风格的 `[[wikilinks]]` 连接论文、概念和笔记，浏览本地知识图谱。
- **PDF 深度阅读**：支持页码导航、适应宽/整页、大纲、⌘F 查找、平滑划词、高亮、批注、提问与翻译。
- **所见即所得的 Markdown**：支持实时预览和编辑。
- **参考文献管理**：解析论文的参考文献，并支持一键导入。
- **远程文献访问**：通过 SSH 隧道浏览远程知识库，数据保留在用户自己的服务器上。
- **多系统兼容**：Mac、Windows、Linux，快捷键与常用软件保持对齐，不改变使用习惯。
- **多种主题风格**： 支持多种主题风格，满足不同用户需求。

![demo-1](docs/assets/ui-1.png)
![demo-2](docs/assets/ui-2.png)
![demo-3](docs/assets/ui-3.png)
![demo-4](docs/assets/ui-4.png)
![demo-5](docs/assets/ui-5.png)

## Quick Start

### 桌面应用

前往 [Agentero](https://agentero.poco-ai.com) 或 [Releases](https://github.com/poco-ai/agentero/releases) 下载。

Linux 需 Ubuntu **22.04+**（webkit2gtk 4.1）。详见 [安装文档](docs/usage/getting-started.md)。

HomeBrew

```bash
brew tap poco-ai/agentero
brew install --cask agentero
```

### CLI

HomeBrew

```bash
brew tap poco-ai/agentero
brew install agentero
```

## 开发

### 项目结构

```text
agentero/
├── AGENTS.md             # 面向 Agent / 开发者的仓库指南
├── mkdocs.yml            # MkDocs 文档站配置
├── src/                  # React + TypeScript 前端
├── src-tauri/            # Tauri 2 + Rust Host（Vault、Wiki、ACP）
├── cli/                  # headless CLI（bin agentero；见 docs/backend/cli.md）
├── templates/vault/      # Create Vault 脚手架（含 .agents/skills）
├── docs/                 # MkDocs：usage / frontend / backend / development（草稿）
└── package.json
```

### 技术栈

<p align="center">
  <img src="https://img.shields.io/badge/Tauri-2-FFC131?logo=tauri&logoColor=white" alt="Tauri 2" />
  <img src="https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=black" alt="React 19" />
  <img src="https://img.shields.io/badge/TypeScript-5-3178C6?logo=typescript&logoColor=white" alt="TypeScript" />
  <img src="https://img.shields.io/badge/Rust-stable-DEA584?logo=rust&logoColor=black" alt="Rust" />
  <img src="https://img.shields.io/badge/pnpm-workspace-F69220?logo=pnpm&logoColor=white" alt="pnpm" />
</p>

- **桌面壳**：[Tauri 2](https://v2.tauri.app/)
- **前端**：[React](https://react.dev/)、[TypeScript](https://www.typescriptlang.org/)、[Tailwind CSS](https://tailwindcss.com/)、[shadcn/ui](https://ui.shadcn.com/)、[AI Elements](https://elements.ai-sdk.dev/)
- **窗口管理**： Dockview
- **PDF**： Embedded PDF
- **编辑器**：[Plate](https://platejs.org/) / Markdown
- **Agent**：[Agent Client Protocol](https://agentclientprotocol.com/)、BYOA

### 测试

```bash
git clone https://github.com/poco-ai/agentero.git
cd agentero
pnpm install

# 清除前端与 Rust 构建产物
pnpm clean

# 桌面应用（推荐）
pnpm tauri dev

# 仅前端预览（无原生 Vault / Agent 后端）
pnpm dev
```

## 贡献

欢迎提交 Issue 和 PR。

1. Fork 后创建功能分支。
2. 保持改动聚焦，并遵守现有 lint/format 设置（`pnpm lint` / `pnpm format`）。
3. PR 描述清楚改动内容和原因。

较大的想法请先开 issue 对齐范围。

## License

本项目使用 [MIT License](LICENSE)。

## Star History

[![Star History Chart](https://api.star-history.com/chart?repos=poco-ai/agentero&type=date&legend=top-left&sealed_token=dKsoXrNYkG3u-nEL3OLp0_aTrlN-GjDpvVEVJvC3xjH13q3viEwwkkB5m6LYT3iKu6LZXtZpQAXalvBwaFQdYgVTjTA1Dzp6NGe_BUQXA1cMt57wNdrYvA)](https://www.star-history.com/?type=date&repos=poco-ai%2Fagentero)

## 致谢

感谢 [LinuxDo](https://linux.do/) 和 [ModelScope](https://modelscope.cn/) 社区的支持与反馈。
