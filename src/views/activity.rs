//! Activity feed: agents row, pipeline summary and chronological events.

use crate::app::App;
use crate::bd::{Interaction, Issue};
use crate::theme as t;
use crate::util::*;
use chrono::{DateTime, Utc};
use eframe::egui;
use egui::{Margin, RichText, Rounding};
use std::collections::HashMap;

impl App {
    pub(crate) fn activity_view(&mut self, ui: &mut egui::Ui) {
        let p = t::pal();
        let title_of: HashMap<&str, &Issue> =
            self.issues.iter().map(|i| (i.id.as_str(), i)).collect();

        // ---- AGENTS: union of assignees + event actors, latest activity each.
        let mut agent_names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        // initech roster (present agents) + assignees + event actors.
        for r in &self.roles {
            agent_names.insert(r.clone());
        }
        for i in &self.issues {
            if let Some(a) = i.assignee.as_deref().filter(|a| !a.is_empty()) {
                agent_names.insert(a.to_string());
            }
        }
        for e in &self.events {
            if !e.actor.is_empty() {
                agent_names.insert(e.actor.clone());
            }
        }
        let roles: std::collections::BTreeSet<String> = self.roles.iter().cloned().collect();
        let mut agents: Vec<AgentCard> = Vec::new();
        for name in &agent_names {
            let mut best: Option<(DateTime<Utc>, String, String)> = None;
            for e in &self.events {
                if &e.actor != name {
                    continue;
                }
                if let Some(ts) = parse_ts(&e.created_at) {
                    if best.as_ref().map(|b| ts > b.0).unwrap_or(true) {
                        best = Some((ts, e.issue_id.clone(), event_action(e)));
                    }
                }
            }
            for i in &self.issues {
                if i.assignee.as_deref() == Some(name.as_str()) {
                    if let Some(ts) = i.updated_at.as_deref().and_then(parse_ts) {
                        if best.as_ref().map(|b| ts > b.0).unwrap_or(true) {
                            best = Some((ts, i.id.clone(), "updated bead".into()));
                        }
                    }
                }
            }
            let (ts, bead, action) = match best {
                Some(b) => (Some(b.0), b.1, b.2),
                None => (None, String::new(), "—".into()),
            };
            let title = title_of.get(bead.as_str()).map(|i| i.title.clone()).unwrap_or_default();
            agents.push(AgentCard { name: name.clone(), ts, bead, title, action });
        }
        agents.sort_by(|a, b| b.ts.cmp(&a.ts));
        let now = Utc::now();
        let active_now = agents
            .iter()
            .filter(|c| c.ts.map(|t| (now - t).num_seconds() < 900).unwrap_or(false))
            .count();

        // ---- PIPELINE: Backlog (P4) + each status, excluding archived.
        let mut pipe: Vec<PipeCard> = Vec::new();
        let backlog: Vec<&Issue> = self
            .issues
            .iter()
            .filter(|i| is_backlog(i))
            .collect();
        pipe.push(PipeCard::build("Backlog".into(), p.text_sub, p.neutral_t, &backlog));
        // Dynamic: statuses present in this project, ordered.
        let statuses = self.statuses_present();
        for s in &statuses {
            let items: Vec<&Issue> = self
                .issues
                .iter()
                .filter(|i| &i.status == s && !is_archived(i) && !is_backlog(i))
                .collect();
            if items.is_empty() {
                continue;
            }
            let st = t::status_style(s);
            pipe.push(PipeCard::build(st.label, st.fg, st.bg, &items));
        }

        // ---- FEED: newest first, grouped by day.
        let mut feed: Vec<&Interaction> = self.events.iter().collect();
        feed.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        feed.truncate(200);
        let feed_items: Vec<FeedItem> = feed
            .iter()
            .filter_map(|e| {
                let ts = parse_ts(&e.created_at)?;
                Some(FeedItem {
                    id: e.issue_id.clone(),
                    title: title_of.get(e.issue_id.as_str()).map(|i| i.title.clone()).unwrap_or_else(|| e.issue_id.clone()),
                    meta: format!("{} {}", e.actor, event_action(e)),
                    ago: ago(ts),
                    day: day_label(ts),
                })
            })
            .collect();
        let total_events = self.events.len();

        let mut clicked: Option<String> = None;
        let mut add_agent = false;
        let mut remove_agent: Option<String> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.set_min_width(ui.available_width());

            ui.horizontal(|ui| {
                ui.label(RichText::new("AGENTS").size(t::FS_CAPTION).strong().color(p.text_sub));
                if ui.small_button("+ Add").on_hover_text("Add agent (initech)").clicked() {
                    add_agent = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("{active_now} active now")).size(t::FS_CAPTION).color(p.text_sub));
                });
            });
            egui::ScrollArea::horizontal().id_salt("agents").show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    for c in &agents {
                        let removable = roles.contains(&c.name);
                        match agent_card(ui, c, now, removable) {
                            AgentAction::Open => clicked = Some(c.bead.clone()),
                            AgentAction::Remove => remove_agent = Some(c.name.clone()),
                            AgentAction::None => {}
                        }
                    }
                });
            });

            ui.add_space(t::SP_MD);
            activity_caption(ui, "Pipeline", "");
            egui::ScrollArea::horizontal().id_salt("pipe").show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    for c in &pipe {
                        if pipeline_card(ui, c) {
                            // clicking a sample id navigates
                            clicked = c.first_sample.clone();
                        }
                    }
                });
            });

            ui.add_space(t::SP_MD);
            activity_caption(ui, "Activity", &format!("{total_events} events"));
            ui.add_space(t::SP_XS);
            let mut last_day = String::new();
            for f in &feed_items {
                if f.day != last_day {
                    ui.add_space(t::SP_SM);
                    ui.label(RichText::new(f.day.to_uppercase()).size(t::FS_CAPTION).strong().color(p.text_sub));
                    last_day = f.day.clone();
                }
                if feed_row(ui, f) {
                    clicked = Some(f.id.clone());
                }
            }
        });
        if let Some(id) = clicked {
            self.select(id);
        }
        if add_agent {
            self.show_add_agent = true;
            self.action_error = None;
        }
        if let Some(name) = remove_agent {
            self.confirm_delete_agent = Some(name);
        }
    }

    // ---- Tree ----
}

pub(crate) struct AgentCard {
    name: String,
    ts: Option<DateTime<Utc>>,
    bead: String,
    title: String,
    action: String,
}

pub(crate) struct PipeCard {
    label: String,
    fg: egui::Color32,
    bg: egui::Color32,
    count: usize,
    breakdown: String,
    samples: Vec<String>,
    more: usize,
    first_sample: Option<String>,
}

impl PipeCard {
    pub(crate) fn build(label: String, fg: egui::Color32, bg: egui::Color32, items: &[&Issue]) -> Self {
        let count = items.len();
        let mut by: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        for i in items {
            *by.entry(i.issue_type.as_str()).or_default() += 1;
        }
        let breakdown = ["bug", "task", "feature", "epic", "chore"]
            .iter()
            .filter_map(|t| by.get(*t).map(|n| format!("{n} {t}{}", if *n == 1 { "" } else { "s" })))
            .collect::<Vec<_>>()
            .join("  ·  ");
        let mut ids: Vec<String> = items.iter().map(|i| i.id.clone()).collect();
        ids.sort();
        let samples: Vec<String> = ids.iter().take(4).cloned().collect();
        let more = count.saturating_sub(samples.len());
        let first_sample = ids.first().cloned();
        PipeCard { label, fg, bg, count, breakdown, samples, more, first_sample }
    }
}

pub(crate) struct FeedItem {
    id: String,
    title: String,
    meta: String,
    ago: String,
    day: String,
}

pub(crate) fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
}

pub(crate) fn ago(ts: DateTime<Utc>) -> String {
    let secs = (Utc::now() - ts).num_seconds().max(0);
    if secs < 60 {
        "now".into()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

pub(crate) fn day_label(ts: DateTime<Utc>) -> String {
    let d = ts.date_naive();
    let today = Utc::now().date_naive();
    if d == today {
        "Today".into()
    } else if Some(d) == today.pred_opt() {
        "Yesterday".into()
    } else {
        ts.format("%a, %b %d").to_string()
    }
}

pub(crate) fn event_action(e: &Interaction) -> String {
    match e.field() {
        "status" => format!("{} {}", t::ic::ARROW_RIGHT, t::status_style(&e.new_value()).label),
        "assignee" => {
            let v = e.new_value();
            if v.is_empty() {
                "unassigned".into()
            } else {
                format!("assigned {v}")
            }
        }
        "priority" => format!("set P{}", e.new_value()),
        _ => e.kind.replace('_', " "),
    }
}

pub(crate) fn activity_caption(ui: &mut egui::Ui, label: &str, right: &str) {
    let p = t::pal();
    ui.horizontal(|ui| {
        ui.label(RichText::new(label.to_uppercase()).size(t::FS_CAPTION).strong().color(p.text_sub));
        if !right.is_empty() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(right).size(t::FS_CAPTION).color(p.text_sub));
            });
        }
    });
}

pub(crate) enum AgentAction {
    Open,
    Remove,
    None,
}

pub(crate) fn agent_card(ui: &mut egui::Ui, c: &AgentCard, now: DateTime<Utc>, removable: bool) -> AgentAction {
    const W: f32 = 210.0;
    let p = t::pal();
    let active = c.ts.map(|t| (now - t).num_seconds() < 900).unwrap_or(false);
    let mut act = AgentAction::None;
    ui.allocate_ui_with_layout(
        egui::vec2(W, 96.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_min_width(W);
            ui.set_max_width(W);
            let resp = t::card_frame(false)
                .show(ui, |ui| {
                    ui.set_width(W - 22.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(t::ic::DOT).size(9.0).color(if active { p.green } else { p.text_sub }));
                        ui.label(RichText::new(&c.name).strong().color(p.text));
                        if removable {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui
                                    .add(egui::Label::new(RichText::new(t::ic::CLOSE).color(p.text_sub)).sense(egui::Sense::click()))
                                    .on_hover_text("Remove agent")
                                    .clicked()
                                {
                                    act = AgentAction::Remove;
                                }
                            });
                        }
                    });
                    if !c.bead.is_empty() {
                        t::copyable_id(ui, &c.bead, t::FS_CAPTION);
                        ui.add(egui::Label::new(RichText::new(&c.title).size(t::FS_SMALL).color(p.text)).truncate());
                    }
                    ui.label(RichText::new(&c.action).size(t::FS_CAPTION).color(p.text_sub));
                    if let Some(ts) = c.ts {
                        ui.label(RichText::new(ago(ts)).size(t::FS_CAPTION).color(p.text_sub));
                    }
                })
                .response;
            if matches!(act, AgentAction::None)
                && !c.bead.is_empty()
                && resp.interact(egui::Sense::click()).clicked()
            {
                act = AgentAction::Open;
            }
        },
    );
    ui.add_space(t::SP_SM);
    act
}

pub(crate) fn pipeline_card(ui: &mut egui::Ui, c: &PipeCard) -> bool {
    const W: f32 = 168.0;
    let p = t::pal();
    let clicked = false;
    ui.allocate_ui_with_layout(
        egui::vec2(W, 132.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_min_width(W);
            ui.set_max_width(W);
            egui::Frame::none()
                .fill(c.bg)
                .rounding(Rounding::same(t::R_MD))
                .stroke(egui::Stroke::new(1.0, p.border))
                .inner_margin(Margin::same(10.0))
                .show(ui, |ui| {
                    ui.set_width(W - 22.0);
                    ui.label(RichText::new(c.label.to_uppercase()).size(t::FS_CAPTION).strong().color(c.fg));
                    ui.label(RichText::new(format!("{}", c.count)).size(22.0).strong().color(c.fg));
                    if !c.breakdown.is_empty() {
                        ui.add(egui::Label::new(RichText::new(&c.breakdown).size(10.0).color(p.text_sub)).truncate());
                    }
                    ui.add_space(2.0);
                    for s in &c.samples {
                        t::copyable_id(ui, s, t::FS_CAPTION);
                    }
                    if c.more > 0 {
                        ui.label(RichText::new(format!("+{} more", c.more)).size(t::FS_CAPTION).color(p.text_sub));
                    }
                });
        },
    );
    ui.add_space(t::SP_SM);
    clicked
}

pub(crate) fn feed_row(ui: &mut egui::Ui, f: &FeedItem) -> bool {
    let p = t::pal();
    let mut clicked = false;
    egui::Frame::none()
        .inner_margin(Margin::symmetric(2.0, 3.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(t::ic::ARROW_RIGHT).color(p.text_sub));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(&f.ago).size(t::FS_CAPTION).color(p.text_sub));
                    ui.label(RichText::new(format!("({})", f.meta)).size(t::FS_CAPTION).color(p.text_sub));
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let r = ui.add(
                            egui::Label::new(RichText::new(&f.title).color(p.primary))
                                .truncate()
                                .sense(egui::Sense::click()),
                        );
                        if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            clicked = true;
                        }
                    });
                });
            });
        });
    clicked
}

