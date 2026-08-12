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
  <a href="README.md">中文</a> | English
</p>

If this project helps you, please **give it a star** on GitHub!
<p align="center">
  <a href="https://github.com/poco-ai/agentero/stargazers"><img src="https://github.com/user-attachments/assets/1d49b049-89ae-4992-a92d-d2411c8053b6" alt="Give me a star" width="480" /></a>
</p>

Traditional reference managers are not agent-friendly:

- Reading highlights and notes are locked inside individual paper files, making them hard for agents to reuse across papers.
- Every conversation requires re-supplying context — there is no stable local knowledge map.
- PDFs are human-friendly, but they are not the most comfortable reading material for an agent.

**Agentero** aims to build an agent-friendly, agent-native way of managing references, and to explore how humans and agents collaborate in literature management.

## Features

- **BYOA** (Bring Your Own Agent): connect your local agent via ACP. Agentero is not locked to any specific agent or model, and your working context stays in the local Vault.
- **Agent-native experience**: selection-based chat, paper import, and Skill import let agents take part in search, reading, and organization workflows.
- **Zotero ecosystem bridge**: compatible import paths from the Zotero ecosystem — save papers from identifiers, links, or the browser extension. Import a Zotero library in one click, keeping tags, notes, and attachments. Export BibTeX / BibLaTeX anytime to plug into your LaTeX writing flow.
- **Figure, table & formula parsing**: figures, tables, formulas, and algorithms in papers are all parsed and understood in context.
- **Paper translation**: full-paper translation, plus side-by-side original/translated views after selecting text, with terminology kept consistent using paper context.
- **Wikilinks & knowledge graph**: connect papers, concepts, and notes with Obsidian-style `[[wikilinks]]` and browse your local knowledge graph.
- **Deep PDF reading**: page navigation, fit-width/fit-page, outline, ⌘F search, smooth text selection, highlights, annotations, Q&A, and translation.
- **WYSIWYG Markdown**: live preview and editing.
- **Reference management**: parse a paper's references and import them with one click.
- **Remote library access**: browse remote knowledge bases over an SSH tunnel — data stays on your own server.
- **Cross-platform**: Mac, Windows, and Linux, with shortcuts aligned with common software so your habits carry over.
- **Multiple themes**: several theme styles to match different preferences.

![demo-1](docs/assets/ui-1.png)
![demo-2](docs/assets/ui-2.png)
![demo-3](docs/assets/ui-3.png)
![demo-4](docs/assets/ui-4.png)
![demo-5](docs/assets/ui-5.png)

## Quick Start

### Desktop app

Download from [Agentero](https://agentero.poco-ai.com) or [Releases](https://github.com/poco-ai/agentero/releases).

Linux requires Ubuntu **22.04+** (webkit2gtk 4.1). See the [installation docs](docs/usage/getting-started.md).

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

## Development

### Project structure

```text
agentero/
├── AGENTS.md             # Repo guide for agents / developers
├── mkdocs.yml            # MkDocs site config
├── src/                  # React + TypeScript frontend
├── src-tauri/            # Tauri 2 + Rust host (Vault, Wiki, ACP)
├── cli/                  # Headless CLI (bin agentero; see docs/backend/cli.md)
├── templates/vault/      # Create Vault scaffold (incl. .agents/skills)
├── docs/                 # MkDocs: usage / frontend / backend / development (drafts)
└── package.json
```

### Tech stack

<p align="center">
  <img src="https://img.shields.io/badge/Tauri-2-FFC131?logo=tauri&logoColor=white" alt="Tauri 2" />
  <img src="https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=black" alt="React 19" />
  <img src="https://img.shields.io/badge/TypeScript-5-3178C6?logo=typescript&logoColor=white" alt="TypeScript" />
  <img src="https://img.shields.io/badge/Rust-stable-DEA584?logo=rust&logoColor=black" alt="Rust" />
  <img src="https://img.shields.io/badge/pnpm-workspace-F69220?logo=pnpm&logoColor=white" alt="pnpm" />
</p>

- **Desktop shell**: [Tauri 2](https://v2.tauri.app/)
- **Frontend**: [React](https://react.dev/), [TypeScript](https://www.typescriptlang.org/), [Tailwind CSS](https://tailwindcss.com/), [shadcn/ui](https://ui.shadcn.com/), [AI Elements](https://elements.ai-sdk.dev/)
- **Window management**: Dockview
- **PDF**: Embedded PDF
- **Editor**: [Plate](https://platejs.org/) / Markdown
- **Agent**: [Agent Client Protocol](https://agentclientprotocol.com/), BYOA

### Getting started

```bash
git clone https://github.com/poco-ai/agentero.git
cd agentero
pnpm install

# Clean frontend and Rust build artifacts
pnpm clean

# Desktop app (recommended)
pnpm tauri dev

# Frontend-only preview (no native Vault / Agent backend)
pnpm dev
```

## Contributing

Issues and PRs are welcome.

1. Fork the repo and create a feature branch.
2. Keep changes focused and follow the existing lint/format setup (`pnpm lint` / `pnpm format`).
3. Describe what the PR changes and why in the PR description.

For larger ideas, open an issue first to align on scope.

## License

This project is licensed under the [MIT License](LICENSE).

## Star History

[![Star History Chart](https://api.star-history.com/chart?repos=poco-ai/agentero&type=date&legend=top-left&sealed_token=dKsoXrNYkG3u-nEL3OLp0_aTrlN-GjDpvVEVJvC3xjH13q3viEwwkkB5m6LYT3iKu6LZXtZpQAXalvBwaFQdYgVTjTA1Dzp6NGe_BUQXA1cMt57wNdrYvA)](https://www.star-history.com/?type=date&repos=poco-ai%2Fagentero)

## Acknowledgements

Thanks to the [LinuxDo](https://linux.do/) and [ModelScope](https://modelscope.cn/) communities for their support and feedback.
