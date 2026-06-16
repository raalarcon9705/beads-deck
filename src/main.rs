#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Beads Deck — a lightweight native dashboard for the `bd` (beads) issue
//! tracker, with a Jira-like UI driven by the design tokens in `theme`.

mod app;
mod bd;
mod lru;
mod markdown;
mod query;
mod registry;
mod schema;
mod state;
mod theme;
mod util;
mod views;

use app::App;
use eframe::egui;

pub(crate) fn load_icon() -> Option<egui::IconData> {
    let svg = include_bytes!("../assets/logo.svg");
    let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default()).ok()?;
    let size = 256u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)?;
    let ts = tree.size();
    let scale = size as f32 / ts.width().max(ts.height());
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(egui::IconData {
        rgba: pixmap.data().to_vec(),
        width: size,
        height: size,
    })
}

/// GUI apps launched from Finder/Spotlight inherit a minimal PATH that omits
/// Homebrew and friends, so `bd`/`initech`/`dolt`/`mmdc` aren't found. Prepend
/// the usual CLI locations so child processes resolve them.
pub(crate) fn ensure_cli_path() {
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        "/opt/homebrew/bin".to_string(),
        "/usr/local/bin".to_string(),
        format!("{home}/.cargo/bin"),
        format!("{home}/.local/bin"),
    ];
    let current = std::env::var("PATH").unwrap_or_default();
    let mut parts: Vec<String> = Vec::new();
    for c in candidates {
        if std::path::Path::new(&c).is_dir()
            && !current.split(':').any(|p| p == c)
            && !parts.contains(&c)
        {
            parts.push(c);
        }
    }
    if !parts.is_empty() {
        let new = if current.is_empty() {
            parts.join(":")
        } else {
            format!("{}:{current}", parts.join(":"))
        };
        std::env::set_var("PATH", new);
    }
}

fn main() -> eframe::Result {
    ensure_cli_path();
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1320.0, 860.0])
        .with_min_inner_size([900.0, 600.0])
        .with_title("Beads Deck");
    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(icon);
    }
    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Beads Deck",
        native_options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
