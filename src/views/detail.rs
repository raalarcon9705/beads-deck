//! Right-hand detail panel and its Details/Comments/History tabs.

use crate::app::App;
use crate::bd::{HistoryEntry, Issue};
use crate::markdown;
use crate::state::{BeadAction, DetailTab};
use crate::theme as t;
use crate::util::*;
use eframe::egui;
use egui::{Margin, RichText, Rounding};
use egui_commonmark::CommonMarkCache;

impl App {
    pub(crate) fn detail_panel(&mut self, ui: &mut egui::Ui) {
        let p = t::pal();
        // Always claim the full panel width so narrow states (spinner / empty)
        // never let egui shrink the resizable panel back to default.
        ui.set_min_width(ui.available_width());

        if self.selected.is_none() {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.label(RichText::new("Select a bead to see details").color(p.text_sub).size(14.0));
            });
            return;
        }
        if self.loading_detail {
            ui.add_space(t::SP_LG);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Loading detail (bd show can be slow)…").color(p.text_sub));
            });
            return;
        }
        if let Some(err) = self.detail_error.clone() {
            ui.colored_label(p.red_d, format!("{} {err}", t::ic::WARNING));
            return;
        }
        let Some(i) = self.detail.clone() else { return };
        let mut nav: Option<String> = None;
        let mut action: Option<BeadAction> = None;

        // Options (computed before the UI closures borrow nothing of self).
        let mut status_opts: Vec<String> = self.selectable_statuses();
        // Ensure the bead's own status is selectable even if unconfigured.
        if !status_opts.contains(&i.status) {
            status_opts.push(i.status.clone());
        }
        let roster = self.agent_roster();
        let releases = self.releases();
        let cur_release = release_of(&i).map(str::to_string);
        // Editable outside self so the combo closure can borrow it freely.
        let mut release_buf = std::mem::take(&mut self.release_buf);
        let mut adding_release = self.adding_release;
        let focus_new_release = std::mem::take(&mut self.focus_release);
        let mut request_focus_next = false;
        let archived_now = is_archived(&i);
        let backlog_now = is_backlog(&i);

        ui.horizontal(|ui| {
            let (glyph, tc) = t::type_glyph(&i.issue_type);
            ui.label(RichText::new(glyph).size(t::FS_H1).color(tc));
            t::copyable_id(ui, &i.id, t::FS_BODY);
            if let Some(par) = &i.parent {
                ui.label(RichText::new(t::ic::PARENT).size(t::FS_SMALL).color(p.text_sub));
                if t::bead_link(ui, par) {
                    nav = Some(par.clone());
                }
            }
            // Action buttons pinned right.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t::ic::DELETE).on_hover_text("Delete bead").clicked() {
                    action = Some(BeadAction::Delete);
                }
                let arch_label = if archived_now { t::ic::UNARCHIVE } else { t::ic::ARCHIVE };
                let arch_hint = if archived_now { "Unarchive" } else { "Archive bead" };
                if ui.button(arch_label).on_hover_text(arch_hint).clicked() {
                    action = Some(BeadAction::ArchiveToggle(archived_now));
                }
                if !backlog_now
                    && ui.button(t::ic::BACKLOG).on_hover_text("Move to backlog (P4)").clicked()
                {
                    action = Some(BeadAction::Backlog);
                }
            });
        });
        ui.add_space(2.0);
        ui.label(RichText::new(&i.title).size(t::FS_H1).strong().color(p.text));
        ui.add_space(t::SP_SM);
        // Interactive status / priority / assignee.
        ui.horizontal_wrapped(|ui| {
            let cur = t::status_style(&i.status);
            egui::ComboBox::from_id_salt("d_status")
                .selected_text(RichText::new(cur.label).color(cur.fg).strong())
                .show_ui(ui, |ui| {
                    for s in &status_opts {
                        if ui.selectable_label(s == &i.status, t::status_style(s).label).clicked()
                            && s != &i.status
                        {
                            action = Some(BeadAction::Status(s.clone()));
                        }
                    }
                });
            egui::ComboBox::from_id_salt("d_prio")
                .selected_text(format!("P{}", i.priority))
                .show_ui(ui, |ui| {
                    for pr in 0..=4 {
                        if ui.selectable_label(pr == i.priority, format!("P{pr}")).clicked()
                            && pr != i.priority
                        {
                            action = Some(BeadAction::Priority(pr));
                        }
                    }
                });
            let cur_a = i.assignee.clone().unwrap_or_default();
            egui::ComboBox::from_id_salt("d_asg")
                .selected_text(if cur_a.is_empty() { "Unassigned".to_string() } else { cur_a.clone() })
                .show_ui(ui, |ui| {
                    if ui.selectable_label(cur_a.is_empty(), "None").clicked() && !cur_a.is_empty() {
                        action = Some(BeadAction::Assignee(None));
                    }
                    for a in &roster {
                        if ui.selectable_label(&cur_a == a, a.as_str()).clicked() && &cur_a != a {
                            action = Some(BeadAction::Assignee(Some(a.clone())));
                        }
                    }
                });
            let rel_text = cur_release.clone().unwrap_or_else(|| "No release".to_string());
            egui::ComboBox::from_id_salt("d_release")
                .selected_text(RichText::new(format!("{} {rel_text}", t::ic::RELEASE)))
                .show_ui(ui, |ui| {
                    if ui.selectable_label(cur_release.is_none(), "No release").clicked()
                        && cur_release.is_some()
                    {
                        action = Some(BeadAction::SetRelease(None));
                    }
                    for r in &releases {
                        if ui.selectable_label(cur_release.as_deref() == Some(r), r.as_str()).clicked()
                            && cur_release.as_deref() != Some(r)
                        {
                            action = Some(BeadAction::SetRelease(Some(r.clone())));
                        }
                    }
                });
            // Inline "new release" entry, kept OUTSIDE the combo popup (a TextEdit
            // inside it would close the popup on focus). Natural sizing keeps the
            // field and its buttons at the same height as the combos.
            if adding_release {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut release_buf)
                        .hint_text("New release…")
                        .desired_width(140.0)
                        .min_size(egui::vec2(0.0, t::CONTROL_H)).vertical_align(egui::Align::Center),
                );
                if focus_new_release {
                    resp.request_focus();
                }
                let submit = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let add = ui.button(t::ic::CHECK).on_hover_text("Add release").clicked();
                let cancel = ui.button(t::ic::CLOSE).on_hover_text("Cancel").clicked();
                if (add || submit) && !release_buf.trim().is_empty() {
                    action = Some(BeadAction::SetRelease(Some(release_buf.trim().to_string())));
                    release_buf.clear();
                    adding_release = false;
                } else if cancel {
                    release_buf.clear();
                    adding_release = false;
                }
            } else if ui.button(format!("{} New", t::ic::PLUS)).on_hover_text("Create a new release").clicked() {
                adding_release = true;
                request_focus_next = true;
            }
            if archived_now {
                t::lozenge(ui, "Archived", p.amber_d, p.yellow_t);
            }
        });
        self.release_buf = release_buf;
        self.adding_release = adding_release;
        self.focus_release = request_focus_next;
        if let Some(err) = self.action_error.clone() {
            ui.colored_label(p.red_d, format!("{} {err}", t::ic::WARNING));
        }
        ui.add_space(t::SP_SM);
        ui.separator();

        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.detail_tab, DetailTab::Details, "Details");
            ui.selectable_value(&mut self.detail_tab, DetailTab::Comments, format!("Comments ({})", i.comments.len()));
            ui.selectable_value(&mut self.detail_tab, DetailTab::History, "History");
        });
        ui.add_space(t::SP_SM);

        let tab = self.detail_tab;
        let md = self.detail_md.clone();
        let comments_md = self.comments_md.clone();
        let history = self.history.clone();
        let cache = &mut self.md_cache;
        let inner = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| match tab {
                DetailTab::Details => detail_tab(ui, &i, &md, cache),
                DetailTab::Comments => {
                    comments_tab(ui, &i, &comments_md, cache);
                    None
                }
                DetailTab::History => {
                    history_tab(ui, &history);
                    None
                }
            })
            .inner;

        if let Some(id) = nav.or(inner) {
            self.select(id);
        }

        if let Some(act) = action {
            let id = i.id.clone();
            self.action_error = None;
            match act {
                BeadAction::Status(s) => self.bd_update(&id, "--status", &s),
                BeadAction::Priority(pr) => self.bd_update(&id, "--priority", &format!("P{pr}")),
                BeadAction::Assignee(Some(a)) => self.bd_update(&id, "--assignee", &a),
                BeadAction::Assignee(None) => self.bd_update(&id, "--assignee", ""),
                BeadAction::Backlog => self.bd_update(&id, "--priority", "4"),
                BeadAction::SetRelease(new) => self.set_release(&id, cur_release.clone(), new),
                BeadAction::ArchiveToggle(now) => {
                    // Cascade to children when this is an epic; unarchive reverses it.
                    self.set_archived(&[id.clone()], !now);
                }
                BeadAction::Delete => self.confirm_delete = Some(id),
            }
        }
    }
}

pub(crate) fn detail_tab(ui: &mut egui::Ui, i: &Issue, md: &str, cache: &mut CommonMarkCache) -> Option<String> {
    let p = t::pal();
    let mut nav: Option<String> = None;
    if !i.description.is_empty() {
        t::section(ui, "Description");
        markdown::show(ui, cache, md);
    }
    t::section(ui, "Dependencies");
    if i.dependencies.is_empty() {
        ui.label(RichText::new("None").color(p.text_sub));
    } else {
        for d in &i.dependencies {
            ui.horizontal(|ui| {
                if !d.status.is_empty() {
                    t::status_lozenge(ui, &d.status);
                }
                if t::bead_link(ui, &d.id) {
                    nav = Some(d.id.clone());
                }
                ui.add(egui::Label::new(RichText::new(&d.title).color(p.text)).truncate());
            });
        }
    }
    t::section(ui, "Relations");
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{} Blocked by: {}", t::ic::BLOCKED, i.dependency_count)).color(p.text_sub));
        ui.label(RichText::new(format!("{} Blocks: {}", t::ic::ARROW_RIGHT, i.dependent_count)).color(p.text_sub));
    });
    if let (Some(c), Some(reason)) = (&i.closed_at, &i.close_reason) {
        t::section(ui, "Resolution");
        ui.label(RichText::new(format!("Closed {}", t::short_date(c))).color(p.text_sub));
        ui.label(RichText::new(reason).color(p.text));
    }
    nav
}

pub(crate) fn comments_tab(ui: &mut egui::Ui, i: &Issue, bodies: &[String], cache: &mut CommonMarkCache) {
    let p = t::pal();
    if i.comments.is_empty() {
        ui.add_space(t::SP_SM);
        ui.label(RichText::new("No comments").color(p.text_sub));
        return;
    }
    for (idx, c) in i.comments.iter().enumerate() {
        ui.add_space(t::SP_SM);
        egui::Frame::none()
            .fill(p.surface_alt)
            .rounding(Rounding::same(t::R_MD))
            .stroke(egui::Stroke::new(1.0, p.border))
            .inner_margin(Margin::same(t::SP_SM))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    t::avatar(ui, &c.author, 24.0);
                    ui.label(RichText::new(&c.author).strong().color(p.text));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(t::short_date(&c.created_at)).color(p.text_sub).size(t::FS_CAPTION));
                    });
                });
                ui.add_space(2.0);
                // Render the comment body as markdown (preprocessed for mermaid).
                let body = bodies.get(idx).map(String::as_str).unwrap_or(&c.text);
                markdown::show(ui, cache, body);
            });
    }
}

pub(crate) fn history_tab(ui: &mut egui::Ui, history: &Result<Vec<HistoryEntry>, String>) {
    let p = t::pal();
    match history {
        Err(e) => {
            ui.add_space(t::SP_SM);
            ui.colored_label(p.amber_d, "History unavailable (known bd 1.0.x bug):");
            ui.label(RichText::new(e).color(p.text_sub).size(t::FS_CAPTION));
        }
        Ok(entries) if entries.is_empty() => {
            ui.add_space(t::SP_SM);
            ui.label(RichText::new("No history").color(p.text_sub));
        }
        Ok(entries) => {
            let mut last_status = String::new();
            for e in entries {
                let st = e.issue.as_ref().map(|s| s.status.clone()).unwrap_or_default();
                let changed = st != last_status && !last_status.is_empty();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t::short_date(&e.commit_date)).color(p.text_sub).size(t::FS_CAPTION));
                    if !st.is_empty() {
                        t::status_lozenge(ui, &st);
                    }
                    if changed {
                        ui.label(RichText::new("transition").size(10.0).italics().color(p.text_sub));
                    }
                    ui.label(RichText::new(&e.committer).size(t::FS_CAPTION).color(p.text_sub));
                });
                last_status = st;
            }
        }
    }
}
