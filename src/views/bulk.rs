//! Bulk actions: a floating action bar shown over the current view (Board /
//! Tree / Releases) while beads are multi-selected, plus the bulk `bd` runners.

use crate::app::App;
use crate::bd;
use crate::state::Msg;
use crate::theme as t;
use crate::util::*;
use eframe::egui;
use egui::{Margin, RichText, Rounding};
use std::thread;

/// A bulk action picked from the floating bar this frame.
enum Bulk {
    Status(String),
    Priority(i64),
    Assignee(Option<String>),
    /// Replace the release across the selection: Some(name) moves all into that
    /// release (dropping any prior release label); None removes them from any.
    SetRelease(Option<String>),
    Archive,
    Unarchive,
    Delete,
    Clear,
}

impl App {
    /// Floating action bar, anchored bottom-center, shown while a bulk selection
    /// is active. Stays within whatever view is open — no navigation required.
    pub(crate) fn bulk_action_bar(&mut self, ctx: &egui::Context) {
        if !self.select_mode || self.selected_ids.is_empty() {
            return;
        }
        let p = t::pal();
        let n = self.selected_ids.len();
        let statuses = self.selectable_statuses();
        let roster = self.agent_roster();
        let releases = self.releases();
        let mut act: Option<Bulk> = None;

        egui::Area::new(egui::Id::new("bulk_bar"))
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -18.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(p.surface)
                    .rounding(Rounding::same(t::R_LG))
                    .stroke(egui::Stroke::new(1.5, p.primary))
                    .inner_margin(Margin::symmetric(12.0, 8.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}  {n} selected", t::ic::CHECKBOX))
                                    .strong()
                                    .color(p.text),
                            );
                            ui.separator();
                            ui.menu_button(format!("Status {}", t::ic::CARET_DOWN), |ui| {
                                for s in &statuses {
                                    if ui.button(t::status_style(s).label).clicked() {
                                        act = Some(Bulk::Status(s.clone()));
                                        ui.close_menu();
                                    }
                                }
                            });
                            ui.menu_button(format!("Priority {}", t::ic::CARET_DOWN), |ui| {
                                for pr in 0..=4 {
                                    if ui.button(format!("P{pr}")).clicked() {
                                        act = Some(Bulk::Priority(pr));
                                        ui.close_menu();
                                    }
                                }
                            });
                            ui.menu_button(format!("Assignee {}", t::ic::CARET_DOWN), |ui| {
                                if ui.button("Unassigned").clicked() {
                                    act = Some(Bulk::Assignee(None));
                                    ui.close_menu();
                                }
                                for a in &roster {
                                    if ui.button(a).clicked() {
                                        act = Some(Bulk::Assignee(Some(a.clone())));
                                        ui.close_menu();
                                    }
                                }
                            });
                            ui.menu_button(format!("Release {}", t::ic::CARET_DOWN), |ui| {
                                if ui.button(format!("{} Remove from release", t::ic::CLOSE)).clicked() {
                                    act = Some(Bulk::SetRelease(None));
                                    ui.close_menu();
                                }
                                ui.separator();
                                if releases.is_empty() {
                                    ui.label(RichText::new("No releases yet — create one from a bead's detail").weak());
                                }
                                for r in &releases {
                                    if ui.button(format!("{} {r}", t::ic::RELEASE)).clicked() {
                                        act = Some(Bulk::SetRelease(Some(r.clone())));
                                        ui.close_menu();
                                    }
                                }
                            });
                            ui.separator();
                            if ui.button(format!("{} Archive", t::ic::ARCHIVE)).clicked() {
                                act = Some(Bulk::Archive);
                            }
                            if ui.button(format!("{} Unarchive", t::ic::UNARCHIVE)).clicked() {
                                act = Some(Bulk::Unarchive);
                            }
                            if ui
                                .button(RichText::new(format!("{} Delete", t::ic::DELETE)).color(p.red_d))
                                .clicked()
                            {
                                act = Some(Bulk::Delete);
                            }
                            ui.separator();
                            if ui.button(t::ic::CLOSE).on_hover_text("Clear selection").clicked() {
                                act = Some(Bulk::Clear);
                            }
                        });
                    });
            });

        match act {
            Some(Bulk::Status(s)) => self.bulk_update("--status", &s),
            Some(Bulk::Priority(pr)) => self.bulk_update("--priority", &format!("P{pr}")),
            Some(Bulk::Assignee(Some(a))) => self.bulk_update("--assignee", &a),
            Some(Bulk::Assignee(None)) => self.bulk_update("--assignee", ""),
            Some(Bulk::SetRelease(new)) => self.bulk_set_release(new),
            Some(Bulk::Archive) => self.bulk_archive(true),
            Some(Bulk::Unarchive) => self.bulk_archive(false),
            Some(Bulk::Delete) => self.confirm_bulk_delete = true,
            Some(Bulk::Clear) => self.selected_ids.clear(),
            None => {}
        }
    }

    fn selected_vec(&self) -> Vec<String> {
        let mut v: Vec<String> = self.selected_ids.iter().cloned().collect();
        v.sort();
        v
    }

    /// `bd update <ids…> <flag> <value>` across the selection. Patches local
    /// state immediately (optimistic) and only reloads on error — same pattern
    /// as drag-and-drop on the board.
    pub(crate) fn bulk_update(&mut self, flag: &str, value: &str) {
        let ids = self.selected_vec();
        if ids.is_empty() {
            return;
        }
        let sel: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();
        for issue in self.issues.iter_mut().filter(|i| sel.contains(i.id.as_str())) {
            match flag {
                "--status" => issue.status = value.to_string(),
                "--priority" => {
                    issue.priority = value.trim_start_matches('P').parse().unwrap_or(issue.priority)
                }
                "--assignee" => {
                    issue.assignee = (!value.is_empty()).then(|| value.to_string())
                }
                _ => {}
            }
        }
        let mut args = vec!["update".to_string()];
        args.extend(ids);
        args.push(flag.to_string());
        args.push(value.to_string());
        self.run_cmd_optimistic("bd", args, None);
        self.selected_ids.clear();
    }

    /// Archive/unarchive the selection, cascading to children of any epics.
    pub(crate) fn bulk_archive(&mut self, archive: bool) {
        let ids = self.selected_vec();
        self.set_archived(&ids, archive);
        self.selected_ids.clear();
    }

    /// `bd label add|remove <ids…> <label>` across the selection (optimistic).
    pub(crate) fn bulk_label(&mut self, op: &str, label: &str) {
        let ids = self.selected_vec();
        if ids.is_empty() {
            return;
        }
        let sel: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();
        for issue in self.issues.iter_mut().filter(|i| sel.contains(i.id.as_str())) {
            match op {
                "add" if !issue.labels.iter().any(|l| l == label) => {
                    issue.labels.push(label.to_string())
                }
                "remove" => issue.labels.retain(|l| l != label),
                _ => {}
            }
        }
        let mut args = vec!["label".to_string(), op.to_string()];
        args.extend(ids);
        args.push(label.to_string());
        self.run_cmd_optimistic("bd", args, None);
        self.selected_ids.clear();
    }

    /// Replace the release across the selection: drop each bead's current
    /// `release:` label, then add the new one (or just remove, for None).
    /// Optimistic local patch + one background thread, like `set_release`.
    pub(crate) fn bulk_set_release(&mut self, new: Option<String>) {
        let ids = self.selected_vec();
        if ids.is_empty() {
            return;
        }
        // Current release per id (for targeted removal).
        let current: Vec<(String, Option<String>)> = ids
            .iter()
            .map(|id| {
                let cur = self
                    .issues
                    .iter()
                    .find(|i| &i.id == id)
                    .and_then(|i| release_of(i).map(str::to_string));
                (id.clone(), cur)
            })
            .collect();
        // Optimistic: drop any release label, add the new one.
        let sel: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();
        for issue in self.issues.iter_mut().filter(|i| sel.contains(i.id.as_str())) {
            issue.labels.retain(|l| !l.starts_with(RELEASE_PREFIX));
            if let Some(n) = &new {
                issue.labels.push(format!("{RELEASE_PREFIX}{n}"));
            }
        }
        self.pending_mutations += 1;
        let (tx, ctx, ws) = (self.tx.clone(), self.ctx.clone(), self.workspace.clone());
        thread::spawn(move || {
            let error = (|| {
                // Remove prior labels, grouped by the release they carried.
                let mut by_rel: std::collections::BTreeMap<String, Vec<String>> =
                    std::collections::BTreeMap::new();
                for (id, cur) in &current {
                    if let Some(c) = cur {
                        by_rel.entry(c.clone()).or_default().push(id.clone());
                    }
                }
                for (rel, rel_ids) in by_rel {
                    let mut args = vec!["label".to_string(), "remove".to_string()];
                    args.extend(rel_ids);
                    args.push(format!("{RELEASE_PREFIX}{rel}"));
                    bd::run_cmd(&ws, "bd", &args)?;
                }
                if let Some(n) = &new {
                    let mut args = vec!["label".to_string(), "add".to_string()];
                    args.extend(current.iter().map(|(id, _)| id.clone()));
                    args.push(format!("{RELEASE_PREFIX}{n}"));
                    bd::run_cmd(&ws, "bd", &args)?;
                }
                Ok::<_, String>(())
            })()
            .err();
            let _ = tx.send(Msg::Mutated { reselect: None, error, optimistic: true });
            ctx.request_repaint();
        });
        self.selected_ids.clear();
    }

    /// Confirmation modal for bulk delete (`bd delete <ids…> --force`).
    pub(crate) fn confirm_bulk_delete_modal(&mut self, ctx: &egui::Context) {
        if !self.confirm_bulk_delete {
            return;
        }
        let p = t::pal();
        let ids = self.selected_vec();
        let (mut yes, mut no) = (false, false);
        egui::Window::new(RichText::new("Delete selected beads?").strong())
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.label(
                    RichText::new(format!(
                        "Permanently delete {} bead(s) and clean up references? This cannot be undone.",
                        ids.len()
                    ))
                    .color(p.text_sub),
                );
                ui.add_space(t::SP_MD);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("Delete").color(egui::Color32::WHITE)).fill(p.red))
                            .clicked()
                        {
                            yes = true;
                        }
                        if ui.button("Cancel").clicked() {
                            no = true;
                        }
                    });
                });
            });
        if no {
            self.confirm_bulk_delete = false;
        }
        if yes {
            self.confirm_bulk_delete = false;
            if !ids.is_empty() {
                let sel: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();
                // Optimistic: drop the beads from the list (and detail) immediately.
                self.issues.retain(|i| !sel.contains(i.id.as_str()));
                if self.selected.as_deref().map(|s| sel.contains(s)).unwrap_or(false) {
                    self.selected = None;
                    self.detail = None;
                }
                let mut args = vec!["delete".to_string()];
                args.extend(ids);
                args.push("--force".to_string());
                self.run_cmd_optimistic("bd", args, None);
            }
            self.selected_ids.clear();
        }
    }
}
