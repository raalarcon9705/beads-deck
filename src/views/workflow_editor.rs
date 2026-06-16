//! Workflow editor modal — a UI/UX over `.beads/deck-workflow.json` so the
//! workflow (states, labels, colors, pipeline order, transitions, role-owners,
//! the external-tracker label) is configured visually, not by hand-editing JSON.
//! Everything here is data; the deck hard-codes no workflow.

use crate::app::App;
use crate::schema::{StateDef, TransitionDef, COLOR_TOKENS};
use crate::theme as t;
use eframe::egui;
use egui::RichText;

impl App {
    pub(crate) fn workflow_editor_modal(&mut self, ctx: &egui::Context) {
        let p = t::pal();
        let mut save = false;
        let mut cancel = false;

        // Roles are a defined set, not free text: the initech roster plus any
        // role already referenced in the schema. Computed before the mutable
        // borrow so the owner/role pickers can use it.
        let role_opts: Vec<String> = {
            let mut v = self.roles.clone();
            for s in &self.editing_schema.states {
                if let Some(o) = &s.owner {
                    v.push(o.clone());
                }
            }
            for tr in &self.editing_schema.transitions {
                if let Some(r) = &tr.role {
                    v.push(r.clone());
                }
            }
            v.sort();
            v.dedup();
            v
        };

        egui::Window::new(RichText::new(format!("{} Workflow Editor", t::ic::CHORE)).strong())
            .collapsible(false)
            .resizable(true)
            .default_width(760.0)
            .default_height(580.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(
                        "Drives Beads Deck for this workspace (.beads/deck-workflow.json). \
                         Everything here is data — the deck hard-codes no workflow.",
                    )
                    .size(t::FS_CAPTION)
                    .color(p.text_sub),
                );
                ui.add_space(t::SP_SM);

                // External-tracker label (e.g. "Jira").
                ui.horizontal(|ui| {
                    ui.label(RichText::new("External-tracker label:").color(p.text_sub));
                    let mut lbl = self.editing_schema.external_ref_label.clone().unwrap_or_default();
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut lbl)
                                .hint_text("e.g. Jira")
                                .desired_width(160.0),
                        )
                        .changed()
                    {
                        self.editing_schema.external_ref_label =
                            (!lbl.trim().is_empty()).then(|| lbl.trim().to_string());
                    }
                });
                ui.add_space(t::SP_SM);
                ui.separator();

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height(430.0)
                    .show(ui, |ui| {
                        let sch = &mut self.editing_schema;

                        // ---- States ----
                        ui.label(
                            RichText::new("STATES  (top → bottom = pipeline order)")
                                .strong()
                                .size(t::FS_SMALL)
                                .color(p.text_sub),
                        );
                        ui.add_space(t::SP_XS);

                        let (mut del, mut up, mut down) = (None, None, None);
                        for idx in 0..sch.states.len() {
                            ui.horizontal(|ui| {
                                let st = &mut sch.states[idx];
                                ui.add(egui::TextEdit::singleline(&mut st.name).hint_text("name").desired_width(120.0));

                                let mut label = st.label.clone().unwrap_or_default();
                                if ui
                                    .add(egui::TextEdit::singleline(&mut label).hint_text("label").desired_width(120.0))
                                    .changed()
                                {
                                    st.label = (!label.trim().is_empty()).then(|| label.trim().to_string());
                                }

                                let cur = st.color.clone().unwrap_or_else(|| "—".into());
                                egui::ComboBox::from_id_salt(("wf_color", idx))
                                    .width(92.0)
                                    .selected_text(cur)
                                    .show_ui(ui, |ui| {
                                        if ui.selectable_label(st.color.is_none(), "—").clicked() {
                                            st.color = None;
                                        }
                                        for tok in COLOR_TOKENS {
                                            if ui.selectable_label(st.color.as_deref() == Some(*tok), *tok).clicked() {
                                                st.color = Some((*tok).to_string());
                                            }
                                        }
                                    });

                                let owner_text = st.owner.clone().unwrap_or_else(|| "— owner".into());
                                egui::ComboBox::from_id_salt(("wf_owner", idx))
                                    .width(84.0)
                                    .selected_text(owner_text)
                                    .show_ui(ui, |ui| {
                                        if ui.selectable_label(st.owner.is_none(), "— none").clicked() {
                                            st.owner = None;
                                        }
                                        for r in &role_opts {
                                            if ui.selectable_label(st.owner.as_deref() == Some(r.as_str()), r).clicked() {
                                                st.owner = Some(r.clone());
                                            }
                                        }
                                    });

                                ui.checkbox(&mut st.side, "side").on_hover_text("Off-pipeline (e.g. blocked)");
                                if ui.small_button(t::ic::CARET_UP).on_hover_text("Move up").clicked() { up = Some(idx); }
                                if ui.small_button(t::ic::CARET_DOWN).on_hover_text("Move down").clicked() { down = Some(idx); }
                                if ui.small_button(RichText::new(t::ic::DELETE).color(p.red_d)).clicked() { del = Some(idx); }
                            });
                        }
                        if ui.button(format!("{} Add state", t::ic::PLUS)).clicked() {
                            sch.states.push(StateDef::default());
                        }
                        if let Some(i) = del { sch.states.remove(i); }
                        if let Some(i) = up { if i > 0 { sch.states.swap(i, i - 1); } }
                        if let Some(i) = down { if i + 1 < sch.states.len() { sch.states.swap(i, i + 1); } }

                        ui.add_space(t::SP_MD);
                        ui.separator();

                        // ---- Transitions ----
                        ui.label(
                            RichText::new("TRANSITIONS  (none = unrestricted)")
                                .strong()
                                .size(t::FS_SMALL)
                                .color(p.text_sub),
                        );
                        ui.add_space(t::SP_XS);

                        let names: Vec<String> = sch.states.iter().map(|s| s.name.clone()).collect();
                        let mut tdel = None;
                        for idx in 0..sch.transitions.len() {
                            ui.horizontal(|ui| {
                                let tr = &mut sch.transitions[idx];
                                egui::ComboBox::from_id_salt(("wf_from", idx))
                                    .width(130.0)
                                    .selected_text(tr.from.clone())
                                    .show_ui(ui, |ui| {
                                        for nm in &names {
                                            if ui.selectable_label(&tr.from == nm, nm).clicked() { tr.from = nm.clone(); }
                                        }
                                    });
                                ui.label(t::ic::ARROW_RIGHT);
                                egui::ComboBox::from_id_salt(("wf_to", idx))
                                    .width(130.0)
                                    .selected_text(tr.to.clone())
                                    .show_ui(ui, |ui| {
                                        for nm in &names {
                                            if ui.selectable_label(&tr.to == nm, nm).clicked() { tr.to = nm.clone(); }
                                        }
                                    });
                                let role_text = tr.role.clone().unwrap_or_else(|| "— role".into());
                                egui::ComboBox::from_id_salt(("wf_role", idx))
                                    .width(90.0)
                                    .selected_text(role_text)
                                    .show_ui(ui, |ui| {
                                        if ui.selectable_label(tr.role.is_none(), "— any").clicked() {
                                            tr.role = None;
                                        }
                                        for r in &role_opts {
                                            if ui.selectable_label(tr.role.as_deref() == Some(r.as_str()), r).clicked() {
                                                tr.role = Some(r.clone());
                                            }
                                        }
                                    });
                                if ui.small_button(RichText::new(t::ic::DELETE).color(p.red_d)).clicked() { tdel = Some(idx); }
                            });
                        }
                        if ui.button(format!("{} Add transition", t::ic::PLUS)).clicked() {
                            let first = names.first().cloned().unwrap_or_default();
                            sch.transitions.push(TransitionDef { from: first.clone(), to: first, role: None });
                        }
                        if let Some(i) = tdel { sch.transitions.remove(i); }
                    });

                ui.add_space(t::SP_SM);
                if let Some(err) = self.action_error.clone() {
                    ui.colored_label(p.red_d, format!("{} {err}", t::ic::WARNING));
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("Save").color(egui::Color32::WHITE)).fill(p.green))
                            .on_hover_text("Write .beads/deck-workflow.json and reload")
                            .clicked()
                        {
                            save = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            });

        if cancel {
            self.show_workflow_editor = false;
        }
        if save {
            self.save_workflow_schema();
        }
    }
}
