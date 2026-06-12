//! Workspace selector screen and the add-workspace modal.

use crate::app::App;
use crate::theme as t;
use crate::util::*;
use eframe::egui;
use egui::{Margin, RichText, Rounding};

pub(crate) enum CardAction {
    Open,
    Remove,
}


impl App {
    pub(crate) fn selector_screen(&mut self, ctx: &egui::Context) {
        let p = t::pal();
        let mut open: Option<String> = None;
        let mut remove: Option<String> = None;
        let mut add = false;
        let entries = self.registry.workspaces.clone();

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(p.page).inner_margin(Margin::same(24.0)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(36.0);
                        ui.add(egui::Image::new(egui::include_image!("../../assets/logo.svg")).fit_to_exact_size(egui::vec2(72.0, 72.0)));
                        ui.add_space(t::SP_SM);
                        ui.label(RichText::new("Select a workspace").size(28.0).strong().color(p.text));
                        ui.label(RichText::new(concat!("v", env!("CARGO_PKG_VERSION"))).color(p.text_sub));
                        ui.add_space(24.0);
                    });
                    // Centered grid with a max width.
                    let max_w = 1040.0_f32.min(ui.available_width());
                    let pad = ((ui.available_width() - max_w) / 2.0).max(0.0);
                    ui.horizontal(|ui| {
                        ui.add_space(pad);
                        ui.allocate_ui(egui::vec2(max_w, 0.0), |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for w in &entries {
                                    if let Some(act) = workspace_card(ui, &w.name, &w.path) {
                                        match act {
                                            CardAction::Open => open = Some(w.path.clone()),
                                            CardAction::Remove => remove = Some(w.path.clone()),
                                        }
                                    }
                                }
                                if add_workspace_card(ui) {
                                    add = true;
                                }
                            });
                        });
                    });
                });
            });

        if let Some(path) = open {
            self.open_workspace(path);
        }
        if let Some(path) = remove {
            self.remove_workspace(&path);
        }
        if add {
            self.show_add = true;
            self.add_error = None;
        }
    }

    // ---- Add Workspace modal ----
    pub(crate) fn add_modal(&mut self, ctx: &egui::Context) {
        let p = t::pal();
        let mut do_add = false;
        let mut cancel = false;
        egui::Window::new(RichText::new("Add Workspace").strong())
            .collapsible(false)
            .resizable(false)
            .auto_sized()
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(440.0);
                ui.label(RichText::new("Point to a project folder that contains a .beads directory.").color(p.text_sub));
                ui.add_space(t::SP_MD);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.add_path)
                            .hint_text("~/Projects/my-project")
                            .desired_width(360.0)
                            .min_size(egui::vec2(0.0, t::CONTROL_H)).vertical_align(egui::Align::Center),
                    );
                    if ui.button(t::ic::FOLDER).on_hover_text("Browse…").clicked() {
                        let mut dialog = rfd::FileDialog::new();
                        if let Ok(home) = std::env::var("HOME") {
                            dialog = dialog.set_directory(home);
                        }
                        if let Some(path) = dialog.pick_folder() {
                            self.add_path = path.display().to_string();
                            self.add_error = None;
                        }
                    }
                });
                if let Some(e) = &self.add_error {
                    ui.add_space(t::SP_XS);
                    ui.colored_label(p.red_d, e);
                }
                ui.add_space(t::SP_MD);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("Add").color(egui::Color32::WHITE)).fill(p.green))
                            .clicked()
                        {
                            do_add = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if cancel {
            self.show_add = false;
            self.add_error = None;
            self.add_path.clear();
        }
        if do_add {
            let path = self.add_path.clone();
            self.add_workspace(path);
        }
    }

}

pub(crate) fn workspace_card(ui: &mut egui::Ui, name: &str, path: &str) -> Option<CardAction> {
    const W: f32 = 320.0;
    let p = t::pal();
    let mut action = None;
    ui.allocate_ui_with_layout(
        egui::vec2(W, 112.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_min_width(W);
            ui.set_max_width(W);
            t::card_frame(false).show(ui, |ui| {
                ui.set_width(W - 24.0);
                ui.set_min_height(84.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t::ic::DOT).size(10.0).color(p.text_sub));
                    ui.label(RichText::new(name).strong().size(16.0).color(p.text));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Label::new(RichText::new(t::ic::CLOSE).color(p.text_sub)).sense(egui::Sense::click()))
                            .on_hover_text("Remove")
                            .clicked()
                        {
                            action = Some(CardAction::Remove);
                        }
                    });
                });
                ui.add(egui::Label::new(RichText::new(short_path(path)).size(t::FS_CAPTION).color(p.text_sub)).truncate());
                ui.add_space(t::SP_SM);
                if ui
                    .add(egui::Button::new(RichText::new("Open").color(egui::Color32::WHITE)).fill(p.green))
                    .clicked()
                {
                    action = Some(CardAction::Open);
                }
            });
        },
    );
    ui.add_space(t::SP_SM);
    action
}

pub(crate) fn add_workspace_card(ui: &mut egui::Ui) -> bool {
    const W: f32 = 320.0;
    let p = t::pal();
    let mut clicked = false;
    ui.allocate_ui_with_layout(
        egui::vec2(W, 112.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_min_width(W);
            ui.set_max_width(W);
            let resp = egui::Frame::none()
                .fill(p.surface_alt)
                .stroke(egui::Stroke::new(1.0, p.border))
                .rounding(Rounding::same(t::R_MD))
                .inner_margin(Margin::same(10.0))
                .show(ui, |ui| {
                    ui.set_width(W - 24.0);
                    ui.set_min_height(84.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("+").size(22.0).color(p.text_sub));
                        ui.add_space(t::SP_SM);
                        ui.vertical(|ui| {
                            ui.add_space(t::SP_SM);
                            ui.label(RichText::new("Add workspace").strong().color(p.text));
                            ui.label(RichText::new("Browse for a beads project").size(t::FS_CAPTION).color(p.text_sub));
                        });
                    });
                })
                .response;
            if resp.interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                clicked = true;
            }
        },
    );
    ui.add_space(t::SP_SM);
    clicked
}

