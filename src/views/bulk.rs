//! Bulk actions: a floating action bar shown over the current view (Board /
//! Tree / Releases) while beads are multi-selected, plus the bulk `bd` runners.

use crate::app::App;
use crate::theme as t;
use crate::util::*;
use eframe::egui;
use egui::{Margin, RichText, Rounding};

/// A bulk action picked from the floating bar this frame.
enum Bulk {
    Status(String),
    Priority(i64),
    Assignee(Option<String>),
    Release(String),
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
                            ui.menu_button("Status \u{25BE}", |ui| {
                                for s in &statuses {
                                    if ui.button(t::status_style(s).label).clicked() {
                                        act = Some(Bulk::Status(s.clone()));
                                        ui.close_menu();
                                    }
                                }
                            });
                            ui.menu_button("Priority \u{25BE}", |ui| {
                                for pr in 0..=4 {
                                    if ui.button(format!("P{pr}")).clicked() {
                                        act = Some(Bulk::Priority(pr));
                                        ui.close_menu();
                                    }
                                }
                            });
                            ui.menu_button("Assignee \u{25BE}", |ui| {
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
                            ui.menu_button("Release \u{25BE}", |ui| {
                                if releases.is_empty() {
                                    ui.label(RichText::new("No releases yet").weak());
                                }
                                for r in &releases {
                                    if ui.button(format!("{} {r}", t::ic::RELEASE)).clicked() {
                                        act = Some(Bulk::Release(r.clone()));
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
            Some(Bulk::Release(r)) => self.bulk_label("add", &format!("{RELEASE_PREFIX}{r}")),
            Some(Bulk::Archive) => self.bulk_label("add", "archived"),
            Some(Bulk::Unarchive) => self.bulk_label("remove", "archived"),
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

    /// `bd update <ids…> <flag> <value>` across the selection, then clear it.
    pub(crate) fn bulk_update(&mut self, flag: &str, value: &str) {
        let ids = self.selected_vec();
        if ids.is_empty() {
            return;
        }
        let mut args = vec!["update".to_string()];
        args.extend(ids);
        args.push(flag.to_string());
        args.push(value.to_string());
        self.run_cmd("bd", args, None);
        self.selected_ids.clear();
    }

    /// `bd label add|remove <ids…> <label>` across the selection, then clear it.
    pub(crate) fn bulk_label(&mut self, op: &str, label: &str) {
        let ids = self.selected_vec();
        if ids.is_empty() {
            return;
        }
        let mut args = vec!["label".to_string(), op.to_string()];
        args.extend(ids);
        args.push(label.to_string());
        self.run_cmd("bd", args, None);
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
                let mut args = vec!["delete".to_string()];
                args.extend(ids);
                args.push("--force".to_string());
                self.run_cmd("bd", args, None);
            }
            self.selected_ids.clear();
        }
    }
}
