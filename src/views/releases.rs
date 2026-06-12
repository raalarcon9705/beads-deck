//! Releases view: beads grouped by `release:` label, with conversion to epic.

use crate::app::App;
use crate::theme as t;
use crate::util::*;
use eframe::egui;
use egui::{Margin, RichText, Rounding};

impl App {
    pub(crate) fn releases_view(&mut self, ui: &mut egui::Ui) {
        let p = t::pal();
        // Bucket indices: one slot per release (sorted), plus a trailing None slot.
        let mut groups: Vec<(Option<String>, Vec<usize>)> =
            self.releases().into_iter().map(|r| (Some(r), Vec::new())).collect();
        groups.push((None, Vec::new()));
        for (idx, i) in self.issues.iter().enumerate() {
            let rel = release_of(i).map(str::to_string);
            let slot = match &rel {
                Some(r) => groups.iter_mut().find(|(g, _)| g.as_deref() == Some(r.as_str())),
                None => groups.last_mut(),
            };
            if let Some((_, v)) = slot {
                v.push(idx);
            }
        }

        let mut clicked: Option<String> = None;
        let mut convert: Option<String> = None;
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

                        if self.releases().is_empty() {
                            ui.add_space(t::SP_MD);
                            ui.label(RichText::new("No releases yet.").strong().color(p.text));
                            ui.label(
                                RichText::new(format!(
                                    "Tag beads with a `{RELEASE_PREFIX}<name>` label (from the detail panel) to group them here."
                                ))
                                .size(t::FS_CAPTION)
                                .color(p.text_sub),
                            );
                            return;
                        }

                        for (rel, idxs) in &groups {
                            let visible = self.apply_sort(
                                idxs.iter()
                                    .copied()
                                    .filter(|&i| self.passes_filter(&self.issues[i]))
                                    .collect(),
                            );
                            // Skip an empty "No release" bucket; always show named releases.
                            if visible.is_empty() && rel.is_none() {
                                continue;
                            }
                            let total = idxs.len();
                            let shipped = idxs.iter().filter(|&&i| is_closed(&self.issues[i])).count();
                            let (icon, name) = match rel {
                                Some(r) => (t::ic::RELEASE, r.clone()),
                                None => ("\u{2014}", "No release".to_string()),
                            };
                            let header = format!("{icon}   {name}   \u{2014}  {shipped}/{total} shipped");
                            ui.add_space(2.0);
                            egui::CollapsingHeader::new(
                                RichText::new(header).strong().size(t::FS_SMALL).color(p.text_sub),
                            )
                            .id_salt(rel.clone().unwrap_or_else(|| "\u{0}none".into()))
                            .default_open(true)
                            .show(ui, |ui| {
                                if let Some(r) = rel {
                                    if ui
                                        .button(RichText::new(format!("{} Convert to epic", t::ic::ARROW_RIGHT)).size(t::FS_CAPTION))
                                        .on_hover_text("Create an epic and reparent every bead in this release under it")
                                        .clicked()
                                    {
                                        convert = Some(r.clone());
                                    }
                                    ui.add_space(t::SP_XS);
                                }
                                for &i in &visible {
                                    if self.tree_row(ui, &self.issues[i]) {
                                        clicked = Some(self.issues[i].id.clone());
                                    }
                                }
                            });
                        }
                    });
            });

        if let Some(id) = clicked {
            self.select(id);
        }
        if let Some(r) = convert {
            self.confirm_convert = Some(r);
        }
    }

}
