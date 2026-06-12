//! Top toolbar: workspace, actions, view tabs, filters and sort.

use crate::app::App;
use crate::state::{Sort, ThemeMode, View};
use crate::theme as t;
use crate::util::*;
use eframe::egui;
use egui::{Margin, RichText, Rounding};

impl App {
    pub(crate) fn top_bar(&mut self, ctx: &egui::Context) {
        let p = t::pal();
        let statuses = self.statuses_present();
        let roster = self.agent_roster();
        egui::TopBottomPanel::top("top")
            .frame(egui::Frame::none().fill(p.surface).inner_margin(Margin::symmetric(12.0, 8.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(format!("{} Back", t::ic::BACK)).on_hover_text(&self.workspace).clicked() {
                        self.go_back();
                    }
                    ui.add_space(t::SP_SM);
                    ui.add(egui::Image::new(egui::include_image!("../../assets/logo.svg")).fit_to_exact_size(egui::vec2(24.0, 24.0)));
                    ui.add_space(t::SP_XS);
                    ui.label(RichText::new(basename(&self.workspace)).size(t::FS_H1).strong().color(p.text));
                    ui.separator();
                    if ui
                        .add(egui::Button::new(RichText::new("+ New bead").color(egui::Color32::WHITE)).fill(p.green))
                        .clicked()
                    {
                        self.show_add_bead = true;
                        self.action_error = None;
                    }
                    if ui.button(format!("{} Reload", t::ic::RELOAD)).clicked() {
                        self.reload();
                    }
                    let live_color = if self.live { p.green } else { p.text_sub };
                    if ui
                        .selectable_label(self.live, RichText::new(format!("{} Live", t::ic::LIVE)).color(live_color))
                        .clicked()
                    {
                        self.live = !self.live;
                    }
                    if self.loading_list {
                        ui.spinner();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Theme selector
                        egui::ComboBox::from_id_salt("theme")
                            .selected_text(match self.theme_mode {
                                ThemeMode::Auto => format!("{} Auto", t::ic::DESKTOP),
                                ThemeMode::Light => format!("{} Light", t::ic::SUN),
                                ThemeMode::Dark => format!("{} Dark", t::ic::MOON),
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.theme_mode, ThemeMode::Auto, format!("{} Auto", t::ic::DESKTOP));
                                ui.selectable_value(&mut self.theme_mode, ThemeMode::Light, format!("{} Light", t::ic::SUN));
                                ui.selectable_value(&mut self.theme_mode, ThemeMode::Dark, format!("{} Dark", t::ic::MOON));
                            });
                        ui.separator();
                        ui.selectable_value(&mut self.view, View::Activity, format!("{} Activity", t::ic::ACTIVITY));
                        ui.selectable_value(&mut self.view, View::Releases, format!("{} Releases", t::ic::RELEASE));
                        ui.selectable_value(&mut self.view, View::Tree, format!("{} Tree", t::ic::TREE));
                        ui.selectable_value(&mut self.view, View::Board, format!("{} Board", t::ic::BOARD));
                        ui.separator();
                        ui.label(RichText::new(format!("{} beads", self.issues.len())).color(p.text_sub));
                    });
                });
                if self.view != View::Activity {
                ui.add_space(t::SP_XS);
                ui.horizontal(|ui| {
                    // Search input (icon inside a rounded frame).
                    egui::Frame::none()
                        .fill(p.surface_alt)
                        .stroke(egui::Stroke::new(1.0, p.border))
                        .rounding(Rounding::same(t::R_MD))
                        .inner_margin(Margin::symmetric(8.0, 4.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(t::ic::SEARCH).color(p.text_sub));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.search)
                                        .hint_text("Search by title or ID…")
                                        .desired_width(220.0)
                                        .frame(false),
                                );
                            });
                        });

                    egui::ComboBox::from_id_salt("st")
                        .width(130.0)
                        .selected_text(
                            self.filter_status
                                .as_ref()
                                .map(|s| t::status_style(s).label)
                                .unwrap_or_else(|| "All Status".into()),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.filter_status, None, "All Status");
                            for s in &statuses {
                                ui.selectable_value(&mut self.filter_status, Some(s.clone()), t::status_style(s).label);
                            }
                        });

                    egui::ComboBox::from_id_salt("pr")
                        .width(120.0)
                        .selected_text(
                            self.filter_priority
                                .map(|p| format!("P{p}"))
                                .unwrap_or_else(|| "All Priority".into()),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.filter_priority, None, "All Priority");
                            for p in 0..=4 {
                                ui.selectable_value(&mut self.filter_priority, Some(p), format!("P{p}"));
                            }
                        });

                    egui::ComboBox::from_id_salt("asg")
                        .width(140.0)
                        .selected_text(
                            self.filter_assignee
                                .clone()
                                .unwrap_or_else(|| "All Assignees".into()),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.filter_assignee, None, "All Assignees");
                            for a in &roster {
                                ui.selectable_value(&mut self.filter_assignee, Some(a.clone()), a.clone());
                            }
                        });

                    ui.add_space(t::SP_SM);
                    ui.label(RichText::new(t::ic::SORT).color(p.text_sub));
                    egui::ComboBox::from_id_salt("sort")
                        .width(170.0)
                        .selected_text(self.sort.label())
                        .show_ui(ui, |ui| {
                            for s in [
                                Sort::Priority,
                                Sort::StatusClosedFirst,
                                Sort::Updated,
                                Sort::Created,
                                Sort::Id,
                            ] {
                                ui.selectable_value(&mut self.sort, s, s.label());
                            }
                        });

                    if self.filter_status.is_some()
                        || self.filter_priority.is_some()
                        || self.filter_assignee.is_some()
                        || !self.search.is_empty()
                    {
                        if ui.button("Clear").clicked() {
                            self.filter_status = None;
                            self.filter_priority = None;
                            self.filter_assignee = None;
                            self.search.clear();
                        }
                    }
                });
                }
            });
    }

    // ---- Board ----
}
