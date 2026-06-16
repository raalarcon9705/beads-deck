//! Collapsible tree view (Epics / Loose / Backlog / Archived).

use crate::app::App;
use crate::bd::Issue;
use crate::state::RowAction;
use crate::theme as t;
use crate::util::*;
use eframe::egui;
use egui::{Margin, RichText, Rounding};
use std::collections::HashMap;

impl App {
    pub(crate) fn tree_view(&mut self, ui: &mut egui::Ui) {
        let p = t::pal();
        // Partition by bucket. Precedence: Archived (label) > Backlog (P4) > Active.
        let mut active: Vec<usize> = Vec::new();
        let mut backlog: Vec<usize> = Vec::new();
        let mut archived: Vec<usize> = Vec::new();
        for (idx, i) in self.issues.iter().enumerate() {
            if is_archived(i) {
                archived.push(idx);
            } else if is_backlog(i) {
                backlog.push(idx);
            } else {
                active.push(idx);
            }
        }
        // Epic tree over the ACTIVE set. Active roots split into Epics vs Loose.
        let active_ids: std::collections::HashSet<&str> =
            active.iter().map(|&i| self.issues[i].id.as_str()).collect();
        let mut children: HashMap<String, Vec<usize>> = HashMap::new();
        let mut epic_roots: Vec<usize> = Vec::new();
        let mut loose_roots: Vec<usize> = Vec::new();
        for &idx in &active {
            let i = &self.issues[idx];
            match &i.parent {
                Some(par) if active_ids.contains(par.as_str()) => {
                    children.entry(par.clone()).or_default().push(idx)
                }
                _ if i.issue_type == "epic" => epic_roots.push(idx),
                _ => loose_roots.push(idx),
            }
        }
        let epic_roots = self.apply_sort(epic_roots);
        let loose_roots = self.apply_sort(loose_roots);
        let backlog = self.apply_sort(backlog);
        let archived = self.apply_sort(archived);

        // Visible counts (respect filters).
        let epics_n = epic_roots.iter().filter(|&&r| self.node_visible(r, &children)).count();
        let loose_n = loose_roots.iter().filter(|&&r| self.node_visible(r, &children)).count();
        let backlog_n = backlog.iter().filter(|&&i| self.passes_filter(&self.issues[i])).count();
        let archived_n = archived.iter().filter(|&&i| self.passes_filter(&self.issues[i])).count();

        let mut clicked: Option<String> = None;
        let mut toggled: Option<String> = None;
        // Visual order of selectable rows (for shift range-select).
        let mut order: Vec<String> = Vec::new();
        egui::Frame::none()
            .fill(p.surface)
            .rounding(Rounding::same(t::R_LG))
            .stroke(egui::Stroke::new(1.0, p.border))
            .inner_margin(Margin::same(t::SP_SM))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());

                        tree_group(ui, t::ic::EPICS, "Epics", epics_n, true, |ui| {
                            for &r in &epic_roots {
                                self.tree_node(ui, r, &children, &mut clicked, &mut toggled, &mut order);
                            }
                        });
                        tree_group(ui, t::ic::LOOSE, "Loose Beads", loose_n, false, |ui| {
                            for &r in &loose_roots {
                                self.tree_node(ui, r, &children, &mut clicked, &mut toggled, &mut order);
                            }
                        });
                        tree_group(ui, t::ic::BACKLOG, "Backlog", backlog_n, false, |ui| {
                            for &i in &backlog {
                                if self.passes_filter(&self.issues[i]) {
                                    order.push(self.issues[i].id.clone());
                                    match self.tree_row(ui, &self.issues[i]) {
                                        Some(RowAction::Open) => clicked = Some(self.issues[i].id.clone()),
                                        Some(RowAction::Toggle) => toggled = Some(self.issues[i].id.clone()),
                                        None => {}
                                    }
                                }
                            }
                        });
                        tree_group(ui, t::ic::ARCHIVE, "Archived", archived_n, false, |ui| {
                            for &i in &archived {
                                if self.passes_filter(&self.issues[i]) {
                                    order.push(self.issues[i].id.clone());
                                    match self.tree_row(ui, &self.issues[i]) {
                                        Some(RowAction::Open) => clicked = Some(self.issues[i].id.clone()),
                                        Some(RowAction::Toggle) => toggled = Some(self.issues[i].id.clone()),
                                        None => {}
                                    }
                                }
                            }
                        });
                    });
            });
        self.visible_order = order;
        if let Some(id) = toggled {
            let shift = ui.input(|i| i.modifiers.shift);
            self.apply_select(id, shift);
        }
        if let Some(id) = clicked {
            self.select(id);
        }
    }

    /// Whether a tree root should render given the active filters.
    pub(crate) fn node_visible(&self, idx: usize, children: &HashMap<String, Vec<usize>>) -> bool {
        if self.passes_filter(&self.issues[idx]) {
            return true;
        }
        if let Some(k) = children.get(&self.issues[idx].id) {
            return k.iter().any(|&c| self.subtree_has_match(c, children));
        }
        false
    }

    pub(crate) fn tree_node(
        &self,
        ui: &mut egui::Ui,
        idx: usize,
        children: &HashMap<String, Vec<usize>>,
        clicked: &mut Option<String>,
        toggled: &mut Option<String>,
        order: &mut Vec<String>,
    ) {
        let i = &self.issues[idx];
        let kids = children.get(&i.id);
        let visible = self.passes_filter(i)
            || kids
                .map(|k| k.iter().any(|&c| self.subtree_has_match(c, children)))
                .unwrap_or(false);
        if !visible {
            return;
        }
        if let Some(kids) = kids {
            // The epic header is itself a selectable row; record it before its
            // children so the shift-range order matches what's on screen.
            order.push(i.id.clone());
            let (glyph, tc) = t::type_glyph(&i.issue_type);
            let header = egui::CollapsingHeader::new(
                RichText::new(format!("{}  {}   {}", glyph, i.id, i.title)).color(tc).strong(),
            )
            .id_salt(&i.id)
            .default_open(true)
            .show(ui, |ui| {
                // The epic itself is rendered ONLY as the header above — do not
                // emit a `tree_row` for `i` here, or it would duplicate the epic
                // as the first child of its own body.
                for &c in kids {
                    self.tree_node(ui, c, children, clicked, toggled, order);
                }
            });
            // A click on the header (the label area) should open the epic's detail
            // panel — mirroring how `tree_row` decides between Open and Toggle.
            // The triangle still toggles expand/collapse independently.
            if header.header_response.clicked() {
                if self.select_mode {
                    *toggled = Some(i.id.clone());
                } else {
                    *clicked = Some(i.id.clone());
                }
            }
        } else if self.passes_filter(i) {
            order.push(i.id.clone());
            match self.tree_row(ui, i) {
                Some(RowAction::Open) => *clicked = Some(i.id.clone()),
                Some(RowAction::Toggle) => *toggled = Some(i.id.clone()),
                None => {}
            }
        }
    }

    pub(crate) fn tree_row(&self, ui: &mut egui::Ui, i: &Issue) -> Option<RowAction> {
        let p = t::pal();
        let selected = self.selected.as_deref() == Some(&i.id);
        let checked = self.selected_ids.contains(&i.id);
        // Captures the id label's rect so the full-row overlay can detect (and
        // prioritize) a click on the id.
        let mut id_rect = egui::Rect::NOTHING;
        let resp = egui::Frame::none()
            .fill(if checked {
                p.green_t
            } else if selected {
                p.blue_t
            } else {
                egui::Color32::TRANSPARENT
            })
            .rounding(Rounding::same(t::R_SM))
            .inner_margin(Margin::symmetric(6.0, 3.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if self.select_mode {
                        let (glyph, color) = if checked {
                            (t::ic::CHECKBOX, p.green)
                        } else {
                            (t::ic::UNCHECKED, p.text_sub)
                        };
                        ui.label(RichText::new(glyph).color(color));
                    }
                    let (glyph, tc) = t::type_glyph(&i.issue_type);
                    ui.label(RichText::new(glyph).color(tc));
                    t::priority_lozenge(ui, i.priority);
                    t::status_lozenge(ui, &i.status);
                    id_rect = t::copyable_id(ui, &i.id, t::FS_CAPTION).rect;
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(a) = i.assignee.as_deref().filter(|a| !a.is_empty()) {
                            t::avatar(ui, a, 20.0);
                        }
                        if i.comment_count > 0 {
                            ui.label(RichText::new(format!("{}{}", t::ic::COMMENT, i.comment_count)).size(t::FS_CAPTION).color(p.text_sub));
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(RichText::new(&i.title).color(p.text)).truncate());
                        });
                    });
                });
            })
            .response;
        let row_resp = resp.interact(egui::Sense::click());
        // A click on the id rect copies instead of opening/toggling; the row
        // Frame's full-rect interaction would otherwise shadow the id label.
        if row_resp.clicked()
            && row_resp
                .interact_pointer_pos()
                .is_some_and(|pos| id_rect.contains(pos))
        {
            t::copy_id_to_clipboard(ui, &i.id);
            return None;
        }
        let clicked = row_resp.clicked();
        if self.select_mode {
            // In select mode the whole row toggles selection (matches the board).
            return clicked.then_some(RowAction::Toggle);
        }
        clicked.then_some(RowAction::Open)
    }

    pub(crate) fn subtree_has_match(&self, idx: usize, children: &HashMap<String, Vec<usize>>) -> bool {
        let i = &self.issues[idx];
        if self.passes_filter(i) {
            return true;
        }
        if let Some(kids) = children.get(&i.id) {
            return kids.iter().any(|&c| self.subtree_has_match(c, children));
        }
        false
    }
}

pub(crate) fn tree_group(
    ui: &mut egui::Ui,
    icon: &str,
    label: &str,
    count: usize,
    default_open: bool,
    body: impl FnOnce(&mut egui::Ui),
) {
    let p = t::pal();
    ui.add_space(2.0);
    egui::CollapsingHeader::new(
        RichText::new(format!("{}   {}   {}", icon, label.to_uppercase(), count))
            .strong()
            .size(t::FS_SMALL)
            .color(p.text_sub),
    )
    .id_salt(label)
    .default_open(default_open)
    .show(ui, body);
}

// ---------------------------------------------------------------------------
// Activity tab data + widgets
// ---------------------------------------------------------------------------

