---
name: agentero-cli
version: 6
description: >-
  Use the Agentero CLI (bin `agentero-cli` on Windows) to create, discover, and
  inspect a local research vault and catalog—list/get papers, import by id/URL,
  check wikilinks, layout regions (figures/tables/formulas), region-anchored
  marks, download assets, parse PAPER.md, export bib—without BYOA. Prefer --json.
  Use when managing a vault headless, scripting Motif/Agentero, or exploring
  papers via machine APIs ($agentero-cli / /agentero-cli).
---

# Agentero CLI (Windows)

## Role

You use the **`agentero-cli` CLI** as a stable machine interface to an Agentero
vault. You do **not** treat the CLI as a chat runtime: it has **no BYOA**, no
ACP, no paper-reader. Reading and writing lecture-style `NOTES.md` is **your**
job (or use the separate `paper-reader` skill / desktop Zap workflow).

Design reference (repo): `docs/backend/cli.md`.

## Prerequisites

- Binary name: **`agentero-cli`** (Windows). Desktop: 设置 → 关于 → 安装 CLI
  writes the `agentero-cli.cmd` shim and adds its install dir to the **user
  PATH** (HKCU\Environment). Freshly opened terminals can run `agentero-cli`
  right away; already-running terminals/processes must be restarted to see the
  new PATH.
- Shells: examples use **PowerShell** (`pwsh` / `powershell`); cmd equivalents
  are given where syntax differs. Quote args containing spaces or cmd
  metacharacters (`&`, `|`, `^`, `<`, `>`): cmd uses `"..."`, PowerShell `'...'`.
- Prefer always passing **`--json`** for machine parsing (disables interactive
  prompts from `inquire`).
- Destructive file deletes: pass **`-y` / `--yes`** under `--json` / non-TTY;
  humans on a TTY may confirm via prompt instead.
- Resolve vault with (first wins): `--vault <path>` → env `AGENTERO_VAULT` →
  cwd walk-up (`.agentero/catalog.sqlite` or standard dirs) → CLI config
  `default_vault`.

If `agentero-cli` is missing from PATH, say so and fall back to reading Vault
files directly; do not invent catalog rows.

## Hard boundaries

| Do | Do not |
|---|---|
| Call CLI for vault/catalog/import/assets | Spawn coding agents via CLI |
| Read files at returned paths | Assume CLI wrote full lecture NOTES |
| Progressive disclosure L0→L4 | Dump entire PDF/TeX into the prompt by default |
| Skip overwrite of user NOTES on re-import | Force-overwrite without explicit user ask |

## Progressive disclosure (same as Vault model)

1. **L0** — `AGENTS.md` (if present)
2. **L1** — `agentero-cli paper list --json` (catalog; no full-text)
3. **L2** — `{paper}/NOTES.md`
4. **L2.5** — layout index + marks
   - `agentero-cli layout list <paper> --json` (sidebar figures/tables/algorithms/formulas)
   - `{paper}/marks/*.json` (reader highlights / asks / translates; prefer CLI write)
5. **L3** — `{paper}/PAPER.md` (if no TeX)
6. **L4** — `{paper}/source/**` (TeX preferred when present)

After `paper get --json`, use `data.assets` (`marksDir` = reader annotations),
`data.suggestedReads` / `paper paths`, then `read_file` those paths. Do **not**
paste whole sources unless needed.

**Figures / formulas (preferred over inventing coordinates):**

```powershell
agentero-cli layout list <paper> --kind figure --json
agentero-cli layout list <paper> --kind formula --json
agentero-cli mark add <paper> --region figure-3 --comment "…" --json
# optional ask shell:
agentero-cli mark add <paper> --region <id> --question "…" --json
```

Requires `{paper}/source/layout-index.json` (written when the desktop runs layout
analysis). If `layout_index_missing`, tell the user to open the paper in Agentero
and run Figures analysis — do not invent bboxes.

Reader marks under `{paper}/marks/` can be referenced from Markdown as annotation
wikilinks: `[[papers/…/NOTES@<id>|label]]` / `![[…@<id>]]` (same sugar as the app).
When you edit NOTES, prefer real ids from `marks/` or the desktop copy action;
do not invent ids. `agentero-cli wiki check` validates path + fragment **shape**
for `@id` / `#@id`, but does **not** open marks to verify the id still exists.
Do **not** write EmbedPDF `marks/annotations.json` by hand.

## Default agent protocol

```powershell
# 1) Confirm vault root
agentero-cli vault which --json
# or: agentero-cli vault info --json

# 2) L1 index (optional filters: --unread, --query, --tag)
agentero-cli paper list --json
agentero-cli paper tag list --json
agentero-cli paper list --tag nlp --json

# 3) One paper: meta + asset flags + suggested paths
agentero-cli paper get <path|id> --json
# minimal paths only:
agentero-cli paper paths <path|id> --json

# 4) Read files yourself in order: NOTES → marks/ → PAPER.md / TeX

# 5) Import (exact id / DOI / URL) — creates shell NOTES, not lecture body
agentero-cli import id <arxiv|doi|url> --json

# 6) After you finish your own notes, optional catalog flags only:
agentero-cli paper set-read <path|id> --json
agentero-cli paper tag set <path|id> nlp survey --json
# incremental:
#   agentero-cli paper tag add <path|id> draft --json
#   agentero-cli paper tag rm  <path|id> survey --json
# clear all: agentero-cli paper tag set <path|id> --clear --json
```

## Command map (MVP)

Global: `--vault`, `--json` / `--output json`, `-y` / `--yes`, `--translator-url`.

| Intent | Command |
|---|---|
| Create vault | `agentero-cli vault create <path> --json` |
| Current vault path | `agentero-cli vault which --json` |
| Summary / health | `agentero-cli vault info --json` / `vault check --json` |
| File tree | `agentero-cli tree [path] --json` |
| Wikilink integrity | `agentero-cli wiki check [file-or-directory] --json` |
| List papers | `agentero-cli paper list [--unread] [--query …] [--tag …] --json` |
| List tags | `agentero-cli paper tag list --json` |
| Get paper | `agentero-cli paper get <path\|id> --json` |
| Paths only | `agentero-cli paper paths <path\|id> --json` |
| Download PDF/TeX | `agentero-cli paper download <path\|id> --json` |
| PDF → PAPER.md | `agentero-cli paper parse <path\|id> [--force] --json` |
| Delete → recycle bin | `agentero-cli paper delete <path> --json`（可用 `trash restore` 恢复） |
| Permanent delete | `agentero-cli paper delete <path> --files -y --json`（不可恢复，仅在用户明确要求时） |
| Mark is_read | `agentero-cli paper set-read <path\|id> [--false] --json` |
| Set / add / remove tags | `agentero-cli paper tag set\|add\|rm <path\|id> … --json`（清空：`tag set --clear`） |
| Magic-wand import | `agentero-cli import id <text> [--parent papers/…] --json` |
| Bib import/export | `agentero-cli import bib <file\|-> --json` / `export bib [-o\|--out file\|-] --json` |
| Layout regions | `agentero-cli layout list <paper> [--kind figure\|table\|algorithm\|formula] --json` |
| Layout get | `agentero-cli layout get <paper> <regionId> --json` |
| Mark on region | `agentero-cli mark add <paper> --region <id> [--comment …] [--question …] --json` |
| Mark list/get/delete | `agentero-cli mark list\|get\|delete … --json`（delete 需 `-y`） |
| Move paper / org folder | `agentero-cli paper move <from> <destParent> --json` |
| Recycle bin | `agentero-cli trash list --json` / `trash restore <batchId> <stored> --json` |
| Purge recycle bin | `agentero-cli trash purge [batchId] [stored] -y --json`（省略参数 = 清空全部） |
| Diagnose vault | `agentero-cli doctor --json` |
| Safe repairs | `agentero-cli doctor fix aliases\|visual-marks\|catalog-duplicates --json` |
| Persist default vault | `agentero-cli vault use <path> --json` |
| Show CLI config | `agentero-cli config show --json` |
| Open vault in desktop App | `agentero-cli open <path>` |

There is **no** `agentero-cli graph` command. Do not invent subcommands: when
unsure, run `agentero-cli <group> --help` and use only what it prints.

## JSON contract

Success:

```json
{ "ok": true, "data": { } }
```

Failure (non-zero exit):

```json
{
  "ok": false,
  "error": { "code": "paper_not_found", "message": "…", "details": {} }
}
```

Stdout = result; stderr = progress/diagnostics. Parse `error.code` when retrying.

Common codes: `vault_not_found`, `vault_invalid`, `paper_not_found`,
`paper_ambiguous`, `import_failed`, `export_failed`, `asset_missing`,
`needs_confirmation`, `wikilink_check_failed`, `wiki_index_failed`.

`wiki check` uses the same resolver as Agentero navigation and returns non-zero
when it finds `missing`, `ambiguous`, or `invalidFragment` occurrences. The
structured report remains available in `error.details`; pass a Vault-relative
Markdown file or directory to isolate the scope.

## Path / id resolution

- Prefer **Vault-relative `path`** (e.g. `papers/1706.03762`).
- Bare **id**: if multiple catalog rows match, CLI errors with candidates—retry with full `path`.

## Workflow recipes

### Explore an existing vault

1. `vault which` / `vault info`
2. `paper list`
3. For each target: `paper get` → read `suggestedReads`
4. Cite Vault-relative paths in your answer; end with `## Sources` when substantial

### Ingest then take notes yourself

1. `import id <ref> --json` → note `data.path`
2. If needed: `paper download` / `paper parse`
3. Write or update `{path}/NOTES.md` (preserve user prose; do not wipe `marks/`)
4. Optional: `paper set-read <path>` only after notes are done
5. For full lecture structure, invoke **`paper-reader`** skill instead of expecting CLI to write it

### Batch / scripts

PowerShell:

```powershell
$env:AGENTERO_VAULT = "C:\path\to\vault"
Get-Content ids.txt | ForEach-Object {
  agentero-cli import id $_ --json
  if ($LASTEXITCODE -ne 0) { Write-Error "fail $_" }
}
```

cmd (interactive uses `%i`; inside a `.bat` / `.cmd` use `%%i`):

```bat
set "AGENTERO_VAULT=C:\path\to\vault"
for /f "usebackq delims=" %i in ("ids.txt") do agentero-cli import id %i --json
```

## Activation notes

Agentero may inject this entire SKILL.md. Depending on the agent:

- **Codex**: `$agentero-cli`
- **Claude**: `/agentero-cli`
- **Other**: follow this body; do not wait for a separate `$` / `/` command

## Rules

- Keep Obsidian wikilinks `[[...]]` when you edit Markdown.
- Never invent catalog metadata; trust CLI / files.
- Never overwrite user-written NOTES without explicit request.
- Prefer short tool loops: list → get → read files → answer.
