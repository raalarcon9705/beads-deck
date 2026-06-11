<p align="center">
  <img src="assets/logo.svg" width="120" alt="Beads Deck logo" />
</p>

<h1 align="center">Beads Deck</h1>

<p align="center">
  A lightweight, native dashboard for the <a href="https://github.com/steveyegge/beads">beads</a> (<code>bd</code>) issue tracker.<br/>
  Built in Rust + <a href="https://github.com/emilk/egui">egui</a> — one small binary, no webview, low RAM.
</p>

---

Beads Deck gives you real-time, Jira-like visibility into your `bd` issues — epics, board,
activity feed and rich bead details — and lets you make changes (status, priority, assignee,
archive, backlog, create/delete) without leaving the app. It reads everything dynamically per
workspace, so custom statuses and `initech` agents just show up.

## Features

- **Three views**
  - **Board** — kanban columns for every workflow status present in the project (custom states included), with cards, avatars and counts.
  - **Tree** — collapsible sections: **Epics**, **Loose Beads**, **Backlog** (P4), **Archived** (`archived` label), with truncating titles and a resizable detail panel.
  - **Activity** — live `AGENTS` row (from `initech` roles + assignees + event actors), a `PIPELINE` summary, and a chronological event feed grouped by day, built from `.beads/interactions.jsonl`.
- **Real-time (Live)** — watches the workspace event log and auto-refreshes within ~2s. Toggle with the `● Live` button.
- **Rich detail panel** — Markdown descriptions and comments with tables, code highlighting, **Mermaid diagrams** (via `mmdc`), and clickable links (bare URLs are auto-linked and open in your browser).
- **Search everything** — id, title, description **and comment bodies** (the latter via `bd export` indexing, since `bd search` alone doesn't cover comments).
- **Write operations**
  - Change **status / priority / assignee** inline (assignees come from the live `initech` roster).
  - **Archive / Unarchive** (`archived` label), **Move to backlog** (P4), **Delete** (with confirmation).
  - **Create a bead** (title, type, priority, assignee, parent-epic selector, description).
  - **Add / remove agents** via `initech add-agent` / `delete-agent`.
- **Workspace selector** — card grid of your projects, native folder picker to add one, remembers the last opened workspace, `← Back` to switch.
- **Theme** — Light / Dark / **Auto** (follows the OS), driven by a single design-token palette.

## Requirements

- [`bd`](https://github.com/steveyegge/beads) (the beads CLI) on your `PATH` — required at runtime.
- [`dolt`](https://github.com/dolthub/dolt) if your workspace uses the embedded Dolt backend.
- Optional: [`initech`](https://github.com/raalarcon9705) for the agent roster (the `Agents` row degrades gracefully without it).
- Optional: [`@mermaid-js/mermaid-cli`](https://github.com/mermaid-js/mermaid-cli) (`mmdc`) to render Mermaid diagrams. Without it, diagrams show as code blocks.

## Install

### Homebrew

```bash
brew install --build-from-source https://raw.githubusercontent.com/raalarcon9705/beads-deck/main/Formula/beads-deck.rb
```

Or pin to the latest commit:

```bash
brew install --HEAD https://raw.githubusercontent.com/raalarcon9705/beads-deck/main/Formula/beads-deck.rb
```

### install.sh (build from source)

```bash
curl -fsSL https://raw.githubusercontent.com/raalarcon9705/beads-deck/main/install.sh | bash
```

Installs to `~/.local/bin` by default (override with `PREFIX=/usr/local/bin`). Requires Rust (`cargo`).

### From source

```bash
git clone https://github.com/raalarcon9705/beads-deck
cd beads-deck
cargo build --release
./target/release/beads-deck
```

## Usage

```bash
beads-deck                       # opens the workspace selector (or resumes the last one)
beads-deck /path/to/project      # open a specific project (a folder containing .beads/)
```

- Use the **workspace selector** to add/open projects, or `← Back` from the header to switch.
- Toggle **Board / Tree / Activity** in the top bar; filter by status / priority / assignee, search, and sort.
- Click a bead to open the detail panel; change its fields, archive, move to backlog, or delete from there.
- **+ New bead** in the header creates one; **+ Add** in Activity adds an `initech` agent.

## Architecture

| File | Responsibility |
|------|----------------|
| `src/bd.rs` | Thin layer over the `bd`/`initech` CLIs: list, show, history, interactions, roles, comment index, and mutations. |
| `src/theme.rs` | Design tokens — light/dark palettes, spacing, radii, type scale, and shared widgets (lozenges, avatars, cards). |
| `src/markdown.rs` | Markdown rendering (`egui_commonmark`) with Mermaid preprocessing and bare-URL autolinking. |
| `src/main.rs` | App state, the three views, detail panel, workspace selector, modals, and real-time polling. |

All `bd` calls run on background threads (a single `bd show` can take seconds on embedded Dolt) and report back over a channel, so the UI never blocks.

## Notes

- **Per-agent attribution**: `bd` records the actor from `$BEADS_ACTOR` / `--actor` / git `user.name`. For the Activity feed to attribute changes to the right agent, set `BEADS_ACTOR=<role>` in each agent's environment.
- Mermaid rendering caches generated SVGs by content hash under your temp dir.
- The workspace registry lives at `~/.beads-deck/registry.json` (imported from Beadbox on first run if present).

## License

MIT © raalarcon9705 — see [LICENSE](LICENSE).
