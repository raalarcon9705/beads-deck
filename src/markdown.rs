//! Markdown rendering for the detail panel, with optional Mermaid diagrams.
//!
//! - Markdown is rendered with `egui_commonmark` (tables, code w/ highlight,
//!   lists, links, images).
//! - Mermaid: there is no pure-Rust Mermaid renderer (Mermaid is JS→SVG), so
//!   ```mermaid``` fences are rendered to SVG via the `mmdc` CLI (mermaid-cli)
//!   when it is installed, cached on disk by content hash, and embedded as an
//!   image. Without `mmdc` the block degrades to a labelled code block.
//!
//! `preprocess_mermaid` does the (possibly process-spawning) work ONCE per
//! bead selection; `show` just renders the already-processed string per frame.

use eframe::egui::Ui;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use std::path::PathBuf;
use std::process::Command;

/// Render already-preprocessed markdown.
pub fn show(ui: &mut Ui, cache: &mut CommonMarkCache, md: &str) {
    CommonMarkViewer::new().show(ui, cache, md);
}

/// Full preprocess applied once per bead: render mermaid + autolink bare URLs.
pub fn preprocess(md: &str) -> String {
    autolink(&preprocess_mermaid(md))
}

/// Wrap bare `http(s)://` URLs in CommonMark autolink syntax `<url>` so the
/// renderer makes them clickable. Skips fenced code blocks, inline code, and
/// URLs already inside a link/autolink.
fn autolink(md: &str) -> String {
    let mut out = String::with_capacity(md.len() + 16);
    let mut in_fence = false;
    for line in md.lines() {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_fence = !in_fence;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if in_fence {
            out.push_str(line);
        } else {
            out.push_str(&autolink_line(line));
        }
        out.push('\n');
    }
    out
}

fn matches_at(chars: &[char], i: usize, pat: &str) -> bool {
    pat.chars().enumerate().all(|(k, pc)| chars.get(i + k) == Some(&pc))
}

fn autolink_line(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    let mut in_code = false; // inline `code`
    while i < chars.len() {
        let c = chars[i];
        if c == '`' {
            in_code = !in_code;
            out.push(c);
            i += 1;
            continue;
        }
        if !in_code && (matches_at(&chars, i, "http://") || matches_at(&chars, i, "https://")) {
            // Already inside a markdown link `](url)` / autolink `<url>` / `[url`?
            let prev = i.checked_sub(1).map(|j| chars[j]);
            if matches!(prev, Some('(') | Some('<') | Some('[')) {
                out.push(c);
                i += 1;
                continue;
            }
            let mut j = i;
            while j < chars.len() {
                let cj = chars[j];
                if cj.is_whitespace()
                    || matches!(cj, '<' | '>' | '(' | ')' | '[' | ']' | '"' | '\'' | '`')
                {
                    break;
                }
                j += 1;
            }
            // Drop trailing sentence punctuation from the URL.
            let mut end = j;
            while end > i && matches!(chars[end - 1], '.' | ',' | ';' | ':' | '!' | '?') {
                end -= 1;
            }
            let url: String = chars[i..end].iter().collect();
            out.push('<');
            out.push_str(&url);
            out.push('>');
            i = end;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Whether `mmdc` (mermaid-cli) is callable.
fn mmdc_available() -> bool {
    Command::new("mmdc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn cache_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("beads-deck-mermaid");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Render one mermaid diagram body to a cached SVG; returns its path.
fn render_diagram(body: &str) -> Option<PathBuf> {
    let dir = cache_dir();
    let svg = dir.join(format!("{:016x}.svg", fnv1a(body)));
    if svg.exists() {
        return Some(svg);
    }
    let mmd = svg.with_extension("mmd");
    std::fs::write(&mmd, body).ok()?;
    let out = Command::new("mmdc")
        .args(["-i", mmd.to_str()?, "-o", svg.to_str()?, "-b", "transparent"])
        .output()
        .ok()?;
    (out.status.success() && svg.exists()).then_some(svg)
}

/// Replace ```mermaid fences with rendered-SVG image links (or leave them as
/// code blocks when mmdc is unavailable / rendering fails).
pub fn preprocess_mermaid(md: &str) -> String {
    if !md.contains("```mermaid") {
        return md.to_string();
    }
    let have_mmdc = mmdc_available();
    let mut out = String::with_capacity(md.len());
    let mut lines = md.lines();
    while let Some(line) = lines.next() {
        if line.trim_start().starts_with("```mermaid") {
            let mut body = String::new();
            for l in lines.by_ref() {
                if l.trim_start().starts_with("```") {
                    break;
                }
                body.push_str(l);
                body.push('\n');
            }
            let rendered = have_mmdc.then(|| render_diagram(&body)).flatten();
            match rendered {
                Some(path) => {
                    out.push_str(&format!("\n![mermaid diagram](file://{})\n", path.display()));
                }
                None => {
                    out.push_str("```mermaid\n");
                    out.push_str(&body);
                    out.push_str("```\n");
                    if !have_mmdc {
                        out.push_str(
                            "\n> ℹ️ Install `mermaid-cli` (`npm i -g @mermaid-js/mermaid-cli`) to render this diagram.\n",
                        );
                    }
                }
            }
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}
