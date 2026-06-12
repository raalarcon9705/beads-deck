# Beads Deck — Agent Map

A lightweight native desktop dashboard for the [`bd`](https://github.com/steveyegge/beads)
(beads) issue tracker. Single Rust binary, no webview. UI is built with
[`egui`](https://github.com/emilk/egui)/`eframe`.

This document orients AI agents (and humans) to the codebase: what each file is
responsible for and where to make a given kind of change.

> **Navigation tip:** this project has a CodeGraph index (`.codegraph/`,
> gitignored). Prefer `codegraph_*` tools for structural questions ("where is X
> defined", "what calls Y", "what breaks if I change Z") over grep. Run
> `codegraph init -i` to (re)build the index if it's missing.

## Big picture

```
bd CLI (subprocess, JSON) ──▶ src/bd.rs ──▶ App state (src/app.rs) ──▶ egui views (src/views/*)
        ▲                                        │
        └──────── mutations (bd ...) ◀───────────┘  (background thread + mpsc channel)
```

- **All data comes from the `bd` CLI**, shelled out per workspace and parsed as
  JSON (`src/bd.rs`). Nothing is stored locally except the workspace registry.
- **The UI is immediate-mode** (egui): `App::update` runs every frame, reads
  `App` state, and renders the active view. There is no retained widget tree.
- **Reads and writes run on background threads** and report back through an
  `mpsc` channel (`Msg`); `App::drain` applies the results. The event log file
  mtime is polled (~2s) to auto-refresh in "Live" mode.
- **A "release" is modeled as a `release:<name>` label**, orthogonal to the
  single-parent epic hierarchy — so a bead can belong to a release *and* an epic.

## File map

### Crate root & domain

| File | Responsibility |
|---|---|
| `src/main.rs` | Entry point only: module declarations, `main()`, window/icon setup, `PATH` fix for Finder/Spotlight launches. |
| `src/bd.rs` | Thin layer over the `bd` CLI. Every read/write shells out to `bd` with the workspace as CWD. Defines the data structs (`Issue`, `Dep`, `Comment`, `HistoryEntry`, `Interaction`) and functions (`list_all`, `show`, `history`, `create_epic`, `run_cmd`, `read_interactions`, …). **Add new `bd` operations here.** |
| `src/markdown.rs` | Markdown preprocessing for the detail panel (Mermaid via `mmdc`, auto-linking). |
| `src/theme.rs` | Design tokens: runtime light/dark `Palette`, spacing/radius/type-scale consts, shared widgets (lozenges, avatars, `copyable_id`), `CONTROL_H` (uniform control height), and the **`ic` module** (Phosphor icon-font aliases). `install_fonts` registers the Phosphor font; `apply` pushes visuals. **Add icons/colors/spacing here.** |
| `src/util.rs` | Small pure helpers: `is_archived`/`is_backlog`/`is_closed`, `release_of`, `RELEASE_PREFIX`, `STATUS_ORDER`/`status_rank`, path/mtime helpers. |
| `src/registry.rs` | `Registry`/`WorkspaceEntry` persisted at `~/.beads-deck/registry.json`, plus one-time import from Beadbox. |
| `src/state.rs` | UI state enums (`Sort`, `View`, `ThemeMode`, `DetailTab`, `BeadAction`) and the background-thread `Msg` type. |

### Application core

| File | Responsibility |
|---|---|
| `src/app.rs` | The `App` struct (all UI state) + lifecycle: `new`, eframe `update` (frame loop & layout), `drain` (apply channel messages), workspace management, theme reconcile, and the **`bd` command runners** (`reload`, `run_cmd`, `run_cmd_optimistic`, `bd_update`, `set_release`, `select`, …). |
| `src/query.rs` | Read-side helpers on `App`: `passes_filter`, `apply_sort`, `statuses_present`, `releases`, `assignees`, and `convert_release` (release → epic). |

### Views (`src/views/`, one surface per file)

| File | Responsibility |
|---|---|
| `src/views.rs` | Declares the view submodules. |
| `src/views/topbar.rs` | Top toolbar: workspace name, actions, view tabs, search/filters/sort. |
| `src/views/board.rs` | Kanban board with drag-and-drop columns + draggable cards. |
| `src/views/tree.rs` | Collapsible tree (Epics / Loose / Backlog / Archived) + the `tree_group`/`tree_row` widgets. |
| `src/views/releases.rs` | **Releases view**: beads grouped by `release:` label with shipped/total progress and "convert to epic". |
| `src/views/detail.rs` | Right-hand detail panel + Details/Comments/History tabs. Inline status/priority/assignee/release editors. |
| `src/views/activity.rs` | Activity feed: agents row, pipeline summary, chronological event feed (+ `AgentCard`/`PipeCard`/`FeedItem` and their card widgets). |
| `src/views/selector.rs` | Workspace selector screen + add-workspace modal + workspace cards. |
| `src/views/modals.rs` | Modal dialogs: new bead, delete confirm, convert-to-epic confirm, add/remove agent. |

## Conventions

- **Cross-module `impl App`:** `App` fields and methods are `pub(crate)` so
  inherent `impl App` blocks can live in sibling modules (views/query). Keep new
  view methods `pub(crate)`.
- **Icons:** never use raw emoji/Unicode glyphs — egui's default fonts lack many.
  Use `t::ic::*` (Phosphor). Add a new alias in `theme::ic`.
- **Controls alignment:** interactive widgets share `t::CONTROL_H`. For text
  inputs, set `.min_size(vec2(0.0, t::CONTROL_H)).vertical_align(Center)` so they
  line up with buttons/combos.
- **Mutations are optimistic where it matters** (status drag) and otherwise
  trigger a full reload via `Msg::Mutated`.
- **Asset paths in macros** (`include_image!`) are relative to the source file —
  view modules use `../../assets/...`.

## Where do I…?

- **Add a `bd` operation** → function in `src/bd.rs`, call it from a runner in
  `src/app.rs` (or `query.rs`).
- **Add a view/tab** → new `View` variant in `src/state.rs`, a module under
  `src/views/`, a tab button in `topbar.rs`, and a dispatch arm in
  `App::update` (`src/app.rs`).
- **Add a detail-panel action** → `BeadAction` variant in `src/state.rs`, UI in
  `views/detail.rs`, handler in the `match action` block there.
- **Change colors/spacing/icons** → `src/theme.rs`.

## Build / run

```sh
cargo build                 # debug build
cargo run -- <workspace>    # open a specific .beads workspace
cargo build --release       # release binary (CI packages this per platform)
```

Releases: bump `Cargo.toml`, tag `vX.Y.Z`, push the tag → `.github/workflows/release.yml`
builds binaries for macOS/Linux/Windows and publishes the GitHub Release; then
pin the source tarball `sha256` in `Formula/beads-deck.rb`.
