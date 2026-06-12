//! Design tokens for Beads Deck — a single source of truth for colors,
//! spacing, radii and typography, inspired by the Atlassian Design System.
//!
//! Colors live in a runtime [`Palette`] (light or dark) so the whole UI can
//! follow the OS theme. Components read the active palette via [`pal()`];
//! spacing / radii / type scale stay compile-time constants.

use eframe::egui::{self, Color32, FontId, Margin, RichText, Rounding, Stroke, Ui};
use std::cell::Cell;

// ---------------------------------------------------------------------------
// Palette
// ---------------------------------------------------------------------------

/// Naming: `*_d` = readable text color placed *on* the `*_t` tint background.
/// In light mode `_d` is dark and `_t` is light; in dark mode it is inverted,
/// so the same lozenge code works for both themes.
#[derive(Clone, Copy)]
pub struct Palette {
    pub page: Color32,
    pub surface: Color32,
    pub surface_alt: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_sub: Color32,
    pub primary: Color32,
    pub neutral_t: Color32,

    pub blue: Color32,
    pub blue_d: Color32,
    pub blue_t: Color32,
    pub green: Color32,
    pub green_d: Color32,
    pub green_t: Color32,
    pub red: Color32,
    pub red_d: Color32,
    pub red_t: Color32,
    pub amber_d: Color32,
    pub yellow_t: Color32,
    pub purple: Color32,
    pub purple_d: Color32,
    pub purple_t: Color32,
    pub teal: Color32,
    pub teal_d: Color32,
    pub teal_t: Color32,
    pub muted: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

impl Palette {
    pub const fn light() -> Self {
        Self {
            page: rgb(0xF4, 0xF5, 0xF7),
            surface: rgb(0xFF, 0xFF, 0xFF),
            surface_alt: rgb(0xFA, 0xFB, 0xFC),
            border: rgb(0xDF, 0xE1, 0xE6),
            text: rgb(0x17, 0x2B, 0x4D),
            text_sub: rgb(0x5E, 0x6C, 0x84),
            primary: rgb(0x00, 0x52, 0xCC),
            neutral_t: rgb(0xDF, 0xE1, 0xE6),

            blue: rgb(0x00, 0x52, 0xCC),
            blue_d: rgb(0x07, 0x47, 0xA6),
            blue_t: rgb(0xDE, 0xEB, 0xFF),
            green: rgb(0x00, 0x87, 0x5A),
            green_d: rgb(0x00, 0x66, 0x44),
            green_t: rgb(0xE3, 0xFC, 0xEF),
            red: rgb(0xDE, 0x35, 0x0B),
            red_d: rgb(0xBF, 0x26, 0x00),
            red_t: rgb(0xFF, 0xEB, 0xE6),
            amber_d: rgb(0x97, 0x4F, 0x0C),
            yellow_t: rgb(0xFF, 0xF0, 0xB3),
            purple: rgb(0x65, 0x54, 0xC0),
            purple_d: rgb(0x40, 0x32, 0x94),
            purple_t: rgb(0xEA, 0xE6, 0xFF),
            teal: rgb(0x00, 0xA3, 0xBF),
            teal_d: rgb(0x00, 0x78, 0x9C),
            teal_t: rgb(0xE6, 0xFC, 0xFF),
            muted: rgb(0x6B, 0x77, 0x8C),
        }
    }

    /// Atlassian-style dark theme.
    pub const fn dark() -> Self {
        Self {
            page: rgb(0x16, 0x1A, 0x1D),
            surface: rgb(0x22, 0x27, 0x2B),
            surface_alt: rgb(0x28, 0x2E, 0x33),
            border: rgb(0x37, 0x3F, 0x47),
            text: rgb(0xC7, 0xD1, 0xDB),
            text_sub: rgb(0x8C, 0x9B, 0xAB),
            primary: rgb(0x57, 0x9D, 0xFF),
            neutral_t: rgb(0x2C, 0x33, 0x3A),

            blue: rgb(0x57, 0x9D, 0xFF),
            blue_d: rgb(0x85, 0xB8, 0xFF),
            blue_t: rgb(0x09, 0x32, 0x6C),
            green: rgb(0x4B, 0xCE, 0x97),
            green_d: rgb(0x7E, 0xE2, 0xB8),
            green_t: rgb(0x16, 0x4B, 0x35),
            red: rgb(0xF8, 0x71, 0x68),
            red_d: rgb(0xFD, 0x98, 0x91),
            red_t: rgb(0x5D, 0x1F, 0x1A),
            amber_d: rgb(0xF5, 0xCD, 0x47),
            yellow_t: rgb(0x53, 0x3F, 0x04),
            purple: rgb(0x9F, 0x8F, 0xEF),
            purple_d: rgb(0xB8, 0xAC, 0xF6),
            purple_t: rgb(0x35, 0x2C, 0x63),
            teal: rgb(0x60, 0xC6, 0xD2),
            teal_d: rgb(0x9D, 0xD9, 0xE2),
            teal_t: rgb(0x1E, 0x31, 0x37),
            muted: rgb(0x8C, 0x9B, 0xAB),
        }
    }
}

thread_local! {
    static ACTIVE: Cell<Palette> = const { Cell::new(Palette::light()) };
}

/// The active palette (cheap — `Palette` is `Copy`).
pub fn pal() -> Palette {
    ACTIVE.with(|a| a.get())
}

fn set_pal(p: Palette) {
    ACTIVE.with(|a| a.set(p));
}

// ---------------------------------------------------------------------------
// Spacing & radii (4-pt scale) and type scale
// ---------------------------------------------------------------------------
pub const SP_XS: f32 = 4.0;
pub const SP_SM: f32 = 8.0;
pub const SP_MD: f32 = 12.0;
pub const SP_LG: f32 = 16.0;

/// Shared height for interactive controls (buttons, combo boxes, text inputs)
/// so they align on a single row.
pub const CONTROL_H: f32 = 24.0;

pub const R_SM: f32 = 3.0;
pub const R_MD: f32 = 6.0;
pub const R_LG: f32 = 8.0;

pub const FS_CAPTION: f32 = 11.0;
pub const FS_SMALL: f32 = 12.0;
pub const FS_BODY: f32 = 13.0;
pub const FS_H1: f32 = 18.0;

pub const CARD_W: f32 = 248.0;
pub const COL_W: f32 = 272.0;

// ---------------------------------------------------------------------------
// Status / type / priority styles
// ---------------------------------------------------------------------------

pub struct Style {
    pub label: String,
    pub fg: Color32,
    pub bg: Color32,
}

pub fn title_case(s: &str) -> String {
    s.split(|c| c == '_' || c == '-')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut ch = w.chars();
            match ch.next() {
                Some(f) => f.to_uppercase().collect::<String>() + ch.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Label map + colors for the workflow states in the strata project.
pub fn status_style(s: &str) -> Style {
    let p = pal();
    let (label, fg, bg) = match s {
        "open" => ("To Do", p.text_sub, p.neutral_t),
        "in_progress" => ("In Progress", p.blue_d, p.blue_t),
        "blocked" => ("Blocked", p.red_d, p.red_t),
        "ready_for_qa" => ("Ready for QA", p.amber_d, p.yellow_t),
        "in_qa" => ("In QA", p.amber_d, p.yellow_t),
        "qa_passed" => ("QA Passed", p.teal_d, p.teal_t),
        "ready_to_ship" => ("Ready to Ship", p.purple_d, p.purple_t),
        "closed" => ("Done", p.green_d, p.green_t),
        "deferred" => ("Deferred", p.muted, p.neutral_t),
        // Unknown/custom status: stable distinct accent derived from the name.
        other => {
            let pairs = [
                (p.blue_d, p.blue_t),
                (p.green_d, p.green_t),
                (p.red_d, p.red_t),
                (p.amber_d, p.yellow_t),
                (p.purple_d, p.purple_t),
                (p.teal_d, p.teal_t),
            ];
            let mut h: u32 = 2166136261;
            for b in other.bytes() {
                h = (h ^ b as u32).wrapping_mul(16777619);
            }
            let (fg, bg) = pairs[(h as usize) % pairs.len()];
            return Style { label: title_case(other), fg, bg };
        }
    };
    Style { label: label.to_string(), fg, bg }
}

pub fn priority_style(prio: i64) -> Style {
    let p = pal();
    let (fg, bg) = match prio {
        0 => (p.red_d, p.red_t),
        1 => (p.amber_d, p.yellow_t),
        2 => (p.green_d, p.green_t),
        _ => (p.text_sub, p.neutral_t),
    };
    Style { label: format!("P{prio}"), fg, bg }
}

/// Bead type → (icon glyph, accent color).
pub fn type_glyph(ty: &str) -> (&'static str, Color32) {
    let p = pal();
    match ty {
        "epic" => (ic::EPIC, p.purple),
        "feature" => (ic::FEATURE, p.green),
        "task" => (ic::TASK, p.blue),
        "bug" => (ic::BUG, p.red),
        "chore" => (ic::CHORE, p.muted),
        _ => (ic::DOT, p.muted),
    }
}

// ---------------------------------------------------------------------------
// Shared widgets
// ---------------------------------------------------------------------------

/// Tint of an arbitrary accent toward the current surface (for lozenge bgs
/// without a fixed token, e.g. the issue-type lozenge).
pub fn tint_of(c: Color32) -> Color32 {
    let s = pal().surface;
    let f = |x: u8, y: u8| (x as f32 * 0.18 + y as f32 * 0.82).round() as u8;
    Color32::from_rgb(f(c.r(), s.r()), f(c.g(), s.g()), f(c.b(), s.b()))
}

pub fn lozenge(ui: &mut Ui, text: &str, fg: Color32, bg: Color32) {
    egui::Frame::none()
        .fill(bg)
        .rounding(Rounding::same(R_SM))
        .inner_margin(Margin::symmetric(6.0, 2.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text.to_uppercase()).color(fg).size(10.5).strong());
        });
}

/// Clickable bead-id link (monospace, primary color, underlined). Returns
/// true when clicked so callers can navigate to that bead.
pub fn bead_link(ui: &mut Ui, id: &str) -> bool {
    let p = pal();
    let resp = ui.add(
        egui::Label::new(
            RichText::new(id)
                .monospace()
                .size(FS_CAPTION)
                .color(p.primary)
                .underline(),
        )
        .sense(egui::Sense::click()),
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Render a bead id as a monospace label that copies the id to the clipboard
/// when clicked (with a hover hint).
pub fn copyable_id(ui: &mut Ui, id: &str, size: f32) {
    let resp = ui
        .add(
            egui::Label::new(RichText::new(id).monospace().size(size).color(pal().text_sub))
                .sense(egui::Sense::click()),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Click to copy id");
    if resp.clicked() {
        ui.output_mut(|o| o.copied_text = id.to_string());
    }
}

pub fn status_lozenge(ui: &mut Ui, status: &str) {
    let s = status_style(status);
    lozenge(ui, &s.label, s.fg, s.bg);
}

pub fn priority_lozenge(ui: &mut Ui, prio: i64) {
    let s = priority_style(prio);
    lozenge(ui, &s.label, s.fg, s.bg);
}

pub fn avatar_color(name: &str) -> Color32 {
    let p = pal();
    let palette = [p.blue, p.purple, p.green, p.red, p.teal, p.amber_d, p.blue_d, p.purple_d];
    let mut h: u32 = 5381;
    for b in name.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u32);
    }
    palette[(h as usize) % palette.len()]
}

pub fn initials(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .take(2)
        .collect::<String>()
        .to_uppercase()
}

pub fn avatar(ui: &mut Ui, name: &str, size: f32) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), size / 2.0, avatar_color(name));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        initials(name),
        FontId::proportional(size * 0.42),
        Color32::WHITE,
    );
    resp.on_hover_text(name.to_string());
}

/// "2026-06-05T16:52:35Z" -> "2026-06-05 16:52"
pub fn short_date(s: &str) -> String {
    let s = s.replace('T', " ");
    s.split('Z').next().unwrap_or(&s).chars().take(16).collect()
}

pub fn card_frame(selected: bool) -> egui::Frame {
    let p = pal();
    let stroke = if selected {
        Stroke::new(2.0, p.primary)
    } else {
        Stroke::new(1.0, p.border)
    };
    egui::Frame::none()
        .fill(p.surface)
        .rounding(Rounding::same(R_MD))
        .stroke(stroke)
        .inner_margin(Margin::same(10.0))
}

pub fn section(ui: &mut Ui, title: &str) {
    ui.add_space(SP_SM);
    ui.label(RichText::new(title.to_uppercase()).size(FS_CAPTION).strong().color(pal().text_sub));
    ui.add_space(2.0);
}

/// Semantic icon aliases over the Phosphor icon font. Using a font (rather than
/// raw emoji codepoints) guarantees every glyph renders consistently across
/// platforms and aligns to the text baseline like any other character.
pub mod ic {
    pub use egui_phosphor::regular::{
        ARCHIVE, ARROWS_DOWN_UP as SORT, ARROW_CLOCKWISE as RELOAD, ARROW_LEFT as BACK,
        ARROW_RIGHT, ARROW_UP as PARENT, ARROW_U_UP_LEFT as UNARCHIVE, BROADCAST as LIVE, BUG,
        CHART_LINE as ACTIVITY, CHAT_CIRCLE as COMMENT, CHECK, CHECK_SQUARE as CHECKBOX,
        CHECK_SQUARE as TASK,
        CIRCLE_DASHED as LOOSE, CROWN as EPIC, DESKTOP, DOT, FOLDER_OPEN as FOLDER,
        MAGNIFYING_GLASS as SEARCH, MOON, PLUS, PROHIBIT as BLOCKED, ROCKET as RELEASE,
        SPARKLE as FEATURE, SQUARE as UNCHECKED, SQUARES_FOUR as BOARD, STACK as EPICS, SUN,
        TRASH as DELETE,
        TRAY as BACKLOG, TREE_STRUCTURE as TREE, WARNING, WRENCH as CHORE, X as CLOSE,
    };
}

/// Install the Phosphor icon font once (in addition to egui's defaults). Call
/// from app startup, not from [`apply`] — fonts only need to be set a single time.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
}

/// Activate `dark`/light tokens and push the matching egui visuals.
pub fn apply(ctx: &egui::Context, dark: bool) {
    let p = if dark { Palette::dark() } else { Palette::light() };
    set_pal(p);

    let mut style = (*ctx.style()).clone();
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = p.page;
    v.window_fill = p.surface;
    v.extreme_bg_color = p.surface;
    v.override_text_color = Some(p.text);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.inactive.bg_fill = p.surface_alt;
    v.widgets.inactive.weak_bg_fill = p.surface_alt;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.primary);
    // Rounded inputs / dropdowns (design-system radius).
    v.widgets.noninteractive.rounding = Rounding::same(R_MD);
    v.widgets.inactive.rounding = Rounding::same(R_MD);
    v.widgets.hovered.rounding = Rounding::same(R_MD);
    v.widgets.active.rounding = Rounding::same(R_MD);
    v.widgets.open.rounding = Rounding::same(R_MD);
    v.selection.bg_fill = p.blue_t;
    v.selection.stroke = Stroke::new(1.0, p.blue_d);
    v.hyperlink_color = p.primary;
    style.visuals = v;
    style.spacing.item_spacing = egui::vec2(SP_SM, SP_XS + 2.0);
    style.spacing.button_padding = egui::vec2(SP_SM, SP_XS);
    // Uniform control height so buttons, combo boxes and text inputs line up on
    // the same row. (TextEdit ignores this unless given a matching `min_size`.)
    style.spacing.interact_size.y = CONTROL_H;
    style.spacing.menu_margin = Margin::same(SP_XS);
    ctx.set_style(style);
}
