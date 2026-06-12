//! Modal dialogs: new bead, delete confirm, convert confirm, agents.

use crate::app::App;
use crate::theme as t;
use crate::util::*;
use eframe::egui;
use egui::RichText;

impl App {
    pub(crate) fn add_bead_modal(&mut self, ctx: &egui::Context) {
        const FIELD_W: f32 = 300.0;
        let p = t::pal();
        let roster = self.agent_roster();
        let mut epics: Vec<(String, String)> = self
            .issues
            .iter()
            .filter(|i| i.issue_type == "epic")
            .map(|i| (i.id.clone(), i.title.clone()))
            .collect();
        epics.sort_by(|a, b| a.0.cmp(&b.0));
        let (mut create, mut cancel) = (false, false);
        egui::Window::new(RichText::new("New Bead").strong())
            .collapsible(false)
            .resizable(false)
            .auto_sized()
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(460.0);
                let cap = |ui: &mut egui::Ui, s: &str| {
                    ui.label(RichText::new(s).size(t::FS_CAPTION).strong().color(p.text_sub));
                };
                cap(ui, "Title");
                ui.add(egui::TextEdit::singleline(&mut self.nb_title).hint_text("Short summary").desired_width(f32::INFINITY).min_size(egui::vec2(0.0, t::CONTROL_H)).vertical_align(egui::Align::Center));
                ui.add_space(t::SP_SM);

                egui::Grid::new("nb_grid")
                    .num_columns(2)
                    .spacing([t::SP_MD, t::SP_SM])
                    .show(ui, |ui| {
                        cap(ui, "Type");
                        egui::ComboBox::from_id_salt("nb_type")
                            .width(FIELD_W)
                            .selected_text(t::title_case(&self.nb_type))
                            .show_ui(ui, |ui| {
                                for ty in ["epic", "feature", "task", "bug", "chore"] {
                                    ui.selectable_value(&mut self.nb_type, ty.to_string(), t::title_case(ty));
                                }
                            });
                        ui.end_row();

                        cap(ui, "Priority");
                        egui::ComboBox::from_id_salt("nb_prio")
                            .width(FIELD_W)
                            .selected_text(format!("P{}", self.nb_priority))
                            .show_ui(ui, |ui| {
                                for pr in 0..=4 {
                                    ui.selectable_value(&mut self.nb_priority, pr, format!("P{pr}"));
                                }
                            });
                        ui.end_row();

                        cap(ui, "Assignee");
                        egui::ComboBox::from_id_salt("nb_asg")
                            .width(FIELD_W)
                            .selected_text(self.nb_assignee.clone().unwrap_or_else(|| "Unassigned".into()))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.nb_assignee, None, "Unassigned");
                                for a in &roster {
                                    ui.selectable_value(&mut self.nb_assignee, Some(a.clone()), a.clone());
                                }
                            });
                        ui.end_row();

                        cap(ui, "Parent epic");
                        egui::ComboBox::from_id_salt("nb_parent")
                            .width(FIELD_W)
                            .selected_text(if self.nb_parent.is_empty() {
                                "None".to_string()
                            } else {
                                self.nb_parent.clone()
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.nb_parent, String::new(), "None");
                                for (id, title) in &epics {
                                    let mut label = format!("{id}  {title}");
                                    if label.chars().count() > 48 {
                                        label = label.chars().take(48).collect::<String>() + "…";
                                    }
                                    ui.selectable_value(&mut self.nb_parent, id.clone(), label);
                                }
                            });
                        ui.end_row();

                        cap(ui, "Release");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.nb_release)
                                .hint_text("e.g. v0.3.0 (optional)")
                                .desired_width(FIELD_W)
                                .min_size(egui::vec2(0.0, t::CONTROL_H)).vertical_align(egui::Align::Center),
                        );
                        ui.end_row();
                    });

                ui.add_space(t::SP_SM);
                cap(ui, "Description (optional)");
                ui.add(egui::TextEdit::multiline(&mut self.nb_desc).desired_rows(4).desired_width(f32::INFINITY));
                if let Some(err) = self.action_error.clone() {
                    ui.add_space(t::SP_XS);
                    ui.colored_label(p.red_d, err);
                }
                ui.add_space(t::SP_MD);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let enabled = !self.nb_title.trim().is_empty();
                        if ui
                            .add_enabled(enabled, egui::Button::new(RichText::new("Create").color(egui::Color32::WHITE)).fill(p.green))
                            .clicked()
                        {
                            create = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            });

        if cancel {
            self.show_add_bead = false;
            self.action_error = None;
        }
        if create {
            let mut args = vec![
                "create".to_string(),
                "--title".into(),
                self.nb_title.trim().to_string(),
                "--type".into(),
                self.nb_type.clone(),
                "--priority".into(),
                format!("P{}", self.nb_priority),
            ];
            if let Some(a) = &self.nb_assignee {
                args.push("--assignee".into());
                args.push(a.clone());
            }
            if !self.nb_parent.trim().is_empty() {
                args.push("--parent".into());
                args.push(self.nb_parent.trim().to_string());
            }
            if !self.nb_desc.trim().is_empty() {
                args.push("-d".into());
                args.push(self.nb_desc.trim().to_string());
            }
            if !self.nb_release.trim().is_empty() {
                args.push("--labels".into());
                args.push(format!("{RELEASE_PREFIX}{}", self.nb_release.trim()));
            }
            self.run_cmd("bd", args, None);
            // reset form
            self.show_add_bead = false;
            self.nb_title.clear();
            self.nb_desc.clear();
            self.nb_parent.clear();
            self.nb_release.clear();
            self.nb_assignee = None;
            self.nb_type = "task".into();
            self.nb_priority = 2;
        }
    }
}

impl App {
    pub(crate) fn confirm_delete_modal(&mut self, ctx: &egui::Context) {
        let Some(id) = self.confirm_delete.clone() else { return };
        let p = t::pal();
        let (mut yes, mut no) = (false, false);
        egui::Window::new(RichText::new("Delete bead?").strong())
            .collapsible(false)
            .resizable(false)
            .auto_sized()
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.label(RichText::new(format!("Permanently delete {id} and clean up references? This cannot be undone.")).color(p.text_sub));
                ui.add_space(t::SP_MD);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(RichText::new("Delete").color(egui::Color32::WHITE)).fill(p.red)).clicked() {
                            yes = true;
                        }
                        if ui.button("Cancel").clicked() {
                            no = true;
                        }
                    });
                });
            });
        if no {
            self.confirm_delete = None;
        }
        if yes {
            self.confirm_delete = None;
            self.selected = None;
            self.detail = None;
            self.run_cmd("bd", vec!["delete".into(), id], None);
        }
    }

    pub(crate) fn confirm_convert_modal(&mut self, ctx: &egui::Context) {
        let Some(release) = self.confirm_convert.clone() else { return };
        let p = t::pal();
        let count = self
            .issues
            .iter()
            .filter(|i| release_of(i) == Some(release.as_str()))
            .count();
        let (mut yes, mut no) = (false, false);
        egui::Window::new(RichText::new("Convert release to epic?").strong())
            .collapsible(false)
            .resizable(false)
            .auto_sized()
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(420.0);
                ui.label(
                    RichText::new(format!(
                        "Create an epic \u{201C}{release}\u{201D} and reparent its {count} bead(s) under it. \
                         Beads already in another epic will be moved to the new one. \
                         The {RELEASE_PREFIX}{release} label is kept."
                    ))
                    .color(p.text_sub),
                );
                ui.add_space(t::SP_MD);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("Convert").color(egui::Color32::WHITE)).fill(p.green))
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
            self.confirm_convert = None;
        }
        if yes {
            self.confirm_convert = None;
            self.convert_release(release);
        }
    }

    pub(crate) fn confirm_delete_agent_modal(&mut self, ctx: &egui::Context) {
        let Some(name) = self.confirm_delete_agent.clone() else { return };
        let p = t::pal();
        let (mut yes, mut no) = (false, false);
        egui::Window::new(RichText::new("Remove agent?").strong())
            .collapsible(false)
            .resizable(false)
            .auto_sized()
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(360.0);
                ui.label(RichText::new(format!("Remove agent '{name}' from initech.yaml? (workspace dir is preserved)")).color(p.text_sub));
                ui.add_space(t::SP_MD);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(RichText::new("Remove").color(egui::Color32::WHITE)).fill(p.red)).clicked() {
                            yes = true;
                        }
                        if ui.button("Cancel").clicked() {
                            no = true;
                        }
                    });
                });
            });
        if no {
            self.confirm_delete_agent = None;
        }
        if yes {
            self.confirm_delete_agent = None;
            self.run_cmd("initech", vec!["delete-agent".into(), name], None);
        }
    }

    pub(crate) fn add_agent_modal(&mut self, ctx: &egui::Context) {
        let p = t::pal();
        let (mut do_add, mut cancel) = (false, false);
        egui::Window::new(RichText::new("Add Agent").strong())
            .collapsible(false)
            .resizable(false)
            .auto_sized()
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.label(RichText::new("Role name (e.g. eng3, qa3, ops) — scaffolds a workspace and registers it in initech.yaml.").color(p.text_sub));
                ui.add_space(t::SP_MD);
                ui.add(egui::TextEdit::singleline(&mut self.add_agent_name).hint_text("eng3").desired_width(340.0).min_size(egui::vec2(0.0, t::CONTROL_H)).vertical_align(egui::Align::Center));
                ui.add_space(t::SP_XS);
                ui.checkbox(&mut self.add_agent_custom, "Custom role (skip catalog check)");
                if let Some(err) = self.action_error.clone() {
                    ui.add_space(t::SP_XS);
                    ui.colored_label(p.red_d, err);
                }
                ui.add_space(t::SP_MD);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(RichText::new("Add").color(egui::Color32::WHITE)).fill(p.green)).clicked() {
                            do_add = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if cancel {
            self.show_add_agent = false;
            self.add_agent_name.clear();
            self.action_error = None;
        }
        if do_add {
            let name = self.add_agent_name.trim().to_string();
            if !name.is_empty() {
                let mut args = vec!["add-agent".to_string(), name];
                if self.add_agent_custom {
                    args.push("--custom".into());
                }
                self.run_cmd("initech", args, None);
                self.show_add_agent = false;
                self.add_agent_name.clear();
            }
        }
    }
}
