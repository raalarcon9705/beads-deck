//! Kanban board view with drag-and-drop columns.

use crate::app::App;
use crate::bd::Issue;
use crate::state::RowAction;
use crate::theme as t;
use crate::util::*;
use eframe::egui;
use egui::{Margin, RichText, Rounding};

impl App {
    pub(crate) fn board_view(&mut self, ui: &mut egui::Ui) {
        let mut clicked: Option<String> = None;
        let mut toggled: Option<String> = None;
        let col_h = ui.available_height();
        let cols = self.board_columns();

        // Check if a drag was just released over a column.
        let drag_released = ui.input(|i| i.pointer.any_released());
        let pointer_pos = ui.input(|i| i.pointer.hover_pos());
        let is_dragging = egui::DragAndDrop::has_any_payload(ui.ctx());

        // Collect (status, col_rect) during rendering so we can hit-test on release.
        let mut col_rects: Vec<(String, egui::Rect)> = Vec::new();

        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal_top(|ui| {
                for status in &cols {
                    let s = t::status_style(status);
                    let mut item_idx: Vec<usize> = self
                        .issues
                        .iter()
                        .enumerate()
                        .filter(|(_, i)| {
                            &i.status == status
                                && !is_archived(i)
                                && !is_backlog(i)
                                && self.passes_filter(i)
                        })
                        .map(|(idx, _)| idx)
                        .collect();
                    if item_idx.is_empty() {
                        continue;
                    }
                    item_idx = self.apply_sort(item_idx);
                    let items: Vec<Issue> = item_idx.iter().map(|&i| self.issues[i].clone()).collect();

                    // Is the cursor hovering over this column while dragging?
                    // col_rect will be NOTHING on the first frame; that's fine.
                    let col_rect = self.board_col_rects.get(status).copied().unwrap_or(egui::Rect::NOTHING);
                    let is_drop_target = is_dragging
                        && pointer_pos.map(|p| col_rect.contains(p)).unwrap_or(false);

                    let (stroke_color, stroke_w) = if is_drop_target {
                        (s.fg, 2.0)
                    } else {
                        (egui::Color32::TRANSPARENT, 0.0)
                    };

                    let resp = ui.allocate_ui_with_layout(
                        egui::vec2(t::COL_W, col_h),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_min_width(t::COL_W);
                            ui.set_max_width(t::COL_W);
                            egui::Frame::none()
                                .fill(s.bg)
                                .rounding(Rounding::same(t::R_MD))
                                .stroke(egui::Stroke::new(stroke_w, stroke_color))
                                .inner_margin(Margin::symmetric(10.0, 6.0))
                                .show(ui, |ui| {
                                    ui.set_width(t::COL_W - 4.0);
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(s.label.to_uppercase()).color(s.fg).size(t::FS_CAPTION).strong());
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.label(RichText::new(format!("{}", items.len())).color(s.fg).strong());
                                        });
                                    });
                                });
                            ui.add_space(t::SP_SM);
                            egui::ScrollArea::vertical()
                                .id_salt(status)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.set_width(t::COL_W - 4.0);
                                    for i in &items {
                                        match self.draggable_card(ui, i) {
                                            Some(RowAction::Open) => clicked = Some(i.id.clone()),
                                            Some(RowAction::Toggle) => toggled = Some(i.id.clone()),
                                            None => {}
                                        }
                                        ui.add_space(t::SP_SM);
                                    }
                                });
                        },
                    );
                    col_rects.push((status.clone(), resp.response.rect));
                    self.board_col_rects.insert(status.clone(), resp.response.rect);
                    ui.add_space(t::SP_SM);
                }
            });
        });

        // Resolve a completed drag-drop.
        if drag_released {
            if let Some(bead_id) = egui::DragAndDrop::take_payload::<String>(ui.ctx()) {
                if let Some(pos) = pointer_pos {
                    if let Some((target_status, _)) = col_rects.iter().find(|(_, r)| r.contains(pos)) {
                        let current = self.issues.iter().find(|i| i.id == *bead_id)
                            .map(|i| i.status.clone()).unwrap_or_default();
                        if *target_status != current {
                            // Optimistic: move card in UI immediately, sync in background.
                            self.optimistic_status(&bead_id, target_status);
                            self.run_cmd_optimistic(
                                "bd",
                                vec!["update".into(), (*bead_id).clone(), "--status".into(), target_status.clone()],
                                Some((*bead_id).clone()),
                            );
                        }
                    }
                }
            }
        }

        if let Some(id) = toggled {
            self.toggle_select(id);
        }
        if let Some(id) = clicked {
            self.select(id);
        }
    }

    pub(crate) fn draggable_card(&self, ui: &mut egui::Ui, i: &Issue) -> Option<RowAction> {
        let p = t::pal();
        let selected = self.selected.as_deref() == Some(&i.id);
        let checked = self.selected_ids.contains(&i.id);
        let is_being_dragged = egui::DragAndDrop::payload::<String>(ui.ctx())
            .map(|pay| *pay == i.id).unwrap_or(false);

        // Render the card content, faded if it's the one being dragged.
        let opacity = if is_being_dragged { 0.35 } else { 1.0 };
        let card_resp = ui.add_enabled_ui(true, |ui| {
            ui.set_opacity(opacity);
            t::card_frame(selected || checked).show(ui, |ui| {
                ui.set_width(t::CARD_W);
                ui.style_mut().interaction.selectable_labels = false;
                if self.select_mode {
                    let (glyph, color) = if checked {
                        (t::ic::CHECKBOX, p.green)
                    } else {
                        (t::ic::UNCHECKED, p.text_sub)
                    };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(glyph).color(color));
                        ui.add(egui::Label::new(RichText::new(&i.title).size(t::FS_BODY).color(p.text)).truncate());
                    });
                } else {
                    ui.label(RichText::new(&i.title).size(t::FS_BODY).color(p.text));
                }
                ui.add_space(t::SP_SM);
                ui.horizontal(|ui| {
                    let (glyph, tc) = t::type_glyph(&i.issue_type);
                    ui.label(RichText::new(glyph).color(tc));
                    t::copyable_id(ui, &i.id, t::FS_CAPTION);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(a) = i.assignee.as_deref().filter(|a| !a.is_empty()) {
                            t::avatar(ui, a, 18.0);
                        }
                        t::priority_lozenge(ui, i.priority);
                        if i.comment_count > 0 {
                            ui.label(RichText::new(format!("{}{}", t::ic::COMMENT, i.comment_count)).size(t::FS_CAPTION).color(p.text_sub));
                        }
                        if i.dependency_count > 0 {
                            ui.label(RichText::new(format!("{}{}", t::ic::BLOCKED, i.dependency_count)).size(t::FS_CAPTION).color(p.text_sub));
                        }
                    });
                });
            }).response
        }).inner;

        // In select mode the card is a selection target, not draggable: a click
        // anywhere on it toggles membership in the bulk selection.
        if self.select_mode {
            let resp = ui.interact(
                card_resp.rect,
                egui::Id::new(("card_sel", &i.id)),
                egui::Sense::click(),
            );
            return resp.clicked().then_some(RowAction::Toggle);
        }

        // Overlay the full card rect with a drag+click sense so it wins over
        // child label events — this is the standard egui D&D pattern.
        let drag_resp = ui.interact(
            card_resp.rect,
            egui::Id::new(("card_drag", &i.id)),
            egui::Sense::click_and_drag(),
        );

        if drag_resp.drag_started() {
            egui::DragAndDrop::set_payload(ui.ctx(), i.id.clone());
        }

        // Floating ghost follows the cursor while dragging.
        if is_being_dragged {
            if let Some(pos) = ui.input(|inp| inp.pointer.hover_pos()) {
                egui::show_tooltip_at(
                    ui.ctx(),
                    ui.layer_id(),
                    egui::Id::new("dnd_ghost"),
                    pos + egui::vec2(12.0, 12.0),
                    |ui| {
                        egui::Frame::none()
                            .fill(p.surface)
                            .rounding(Rounding::same(t::R_SM))
                            .stroke(egui::Stroke::new(1.5, p.border))
                            .inner_margin(Margin::same(8.0))
                            .show(ui, |ui| {
                                ui.set_max_width(180.0);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&i.title).size(t::FS_SMALL).color(p.text),
                                    )
                                    .truncate(),
                                );
                                ui.label(
                                    RichText::new(&i.id)
                                        .monospace()
                                        .size(t::FS_CAPTION)
                                        .color(p.text_sub),
                                );
                            });
                    },
                );
            }
        }

        drag_resp.clicked().then_some(RowAction::Open)
    }

    // ---- Activity ----
}
