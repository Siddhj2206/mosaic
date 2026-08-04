use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::scroll::ScrollableElement;
use gpui_component::table::*;
use gpui_component::{h_flex, v_flex, IconName, Sizable, StyledExt};
use mosaic_core::config::Config;
use mosaic_core::db::MosaicDb;
use mosaic_core::types::{Ipo, SubscriptionEntry};

use crate::ipo_list::IpoTableDelegate;

gpui::actions!(mosaic, [SyncNow]);
const APP_CONTEXT: &str = "MosaicApp";

const STATUSES: &[(&str, &str)] = &[
    ("all", "All"),
    ("open", "Open"),
    ("upcoming", "Upcoming"),
    ("closed", "Closed"),
    ("listed", "Listed"),
    ("withdrawn", "Withdrawn"),
];

pub struct MosaicApp {
    db: MosaicDb,
    sync_task: Task<()>,
    sync_trigger: Arc<AtomicBool>,
    table_state: Entity<TableState<IpoTableDelegate>>,
    last_sync: Option<String>,

    // IPO data
    all_ipos: Vec<Ipo>,
    filtered_ipos: Vec<Ipo>,
    selected_status: Option<String>,

    // Selection
    selected_ipo: Option<Ipo>,
    selected_ipo_subs: Vec<SubscriptionEntry>,

    // Layout
    collapsed: bool,
}

impl MosaicApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let path = mosaic::db_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let db = MosaicDb::open(&path).expect("Failed to open Mosaic database");

        let all_ipos = db.list_ipos(None).unwrap_or_default();
        let filtered_ipos = all_ipos.clone();
        let delegate = IpoTableDelegate::new(filtered_ipos.clone());
        let table_state = cx.new(|cx| {
            TableState::new(delegate, window, cx)
                .sortable(true)
                .row_selectable(true)
                .col_resizable(true)
        });

        cx.subscribe(&table_state, |this, _emitter, event: &TableEvent, cx| {
            match event {
                TableEvent::SelectRow(row_ix) => {
                    if let Some(ipo) = this.filtered_ipos.get(*row_ix).cloned() {
                        let id = ipo.id.unwrap_or(0);
                        let subs = this.db.get_latest_snapshot(id).unwrap_or_default();
                        this.selected_ipo = Some(ipo);
                        this.selected_ipo_subs = subs;
                        cx.notify();
                    }
                }
                TableEvent::ColumnWidthsChanged(widths) => {
                    let s = Self::widths_to_str(widths);
                    let _ = this.db.kv_set("ui.table_column_widths", &s);
                }
                _ => {}
            }
        })
        .detach();

        let sync_trigger = Arc::new(AtomicBool::new(false));
        let sync_task = Self::schedule_sync(path, sync_trigger.clone(), cx);

        let collapsed = db
            .kv_get("ui.collapsed")
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(false);

        let mut app = Self {
            db,
            sync_task,
            sync_trigger,
            table_state,
            last_sync: None,
            all_ipos,
            filtered_ipos,
            selected_status: None,
            selected_ipo: None,
            selected_ipo_subs: vec![],
            collapsed,
        };

        // Restore filter selection
        if let Some(status) = app.db.kv_get("ui.filter_status").ok().flatten() {
            if !status.is_empty() {
                app.selected_status = Some(status);
                app.apply_filter(cx);
            }
        }

        // Restore column widths
        if let Some(widths_str) = app.db.kv_get("ui.table_column_widths").ok().flatten() {
            let widths = Self::str_to_widths(&widths_str);
            if !widths.is_empty() {
                app.table_state.update(cx, |state, cx| {
                    let delegate = state.delegate_mut();
                    for (i, w) in widths.iter().enumerate() {
                        if let Some(col) = delegate.columns.get_mut(i) {
                            col.width = *w;
                        }
                    }
                    state.refresh(cx);
                });
            }
        }

        app
    }

    fn schedule_sync(db_path: PathBuf, trigger: Arc<AtomicBool>, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let mut backoff = Duration::ZERO;
            let trigger = trigger;
            loop {
                if trigger.swap(false, Ordering::AcqRel) {
                    backoff = Duration::ZERO;
                }
                cx.background_executor().timer(backoff).await;
                let path = db_path.clone();
                let result = cx
                    .background_spawn(async move { mosaic::run_sync(&path) })
                    .await;

                match result {
                    Ok(r) => {
                        backoff = Duration::ZERO;
                        let summary = format!(
                            "{} IPOs, {} updated, {} skipped",
                            r.total, r.updated, r.skipped
                        );
                        this.update(cx, |this, cx| {
                            this.last_sync = Some(summary);
                            this.refresh_ipos(cx);
                        })
                        .ok();
                    }
                    Err(e) => {
                        log::error!("sync failed: {e:#}");
                        if backoff == Duration::ZERO {
                            backoff = Duration::from_secs(1);
                        } else {
                            backoff = std::cmp::min(backoff * 2, Duration::from_secs(60));
                        }
                        continue;
                    }
                }

                let interval = Config::load().refresh_interval_secs.unwrap_or(300);
                cx.background_executor()
                    .timer(Duration::from_secs(interval))
                    .await;
            }
        })
    }

    fn on_sync_now(&mut self, _: &SyncNow, _window: &mut Window, cx: &mut Context<Self>) {
        self.sync_trigger.store(true, Ordering::Release);
        cx.notify();
    }

    fn refresh_ipos(&mut self, cx: &mut Context<Self>) {
        self.all_ipos = self.db.list_ipos(None).unwrap_or_default();
        self.apply_filter(cx);
    }

    fn set_filter(&mut self, status: Option<&'static str>, cx: &mut Context<Self>) {
        self.selected_status = status.map(|s| s.to_string());
        self.apply_filter(cx);
        let key = self.selected_status.clone().unwrap_or_default();
        let _ = self.db.kv_set("ui.filter_status", &key);
    }

    fn apply_filter(&mut self, cx: &mut Context<Self>) {
        self.filtered_ipos = match &self.selected_status {
            Some(s) if s != "all" => self
                .all_ipos
                .iter()
                .filter(|ipo| ipo.status.as_str() == *s)
                .cloned()
                .collect(),
            _ => self.all_ipos.clone(),
        };

        let new_ipos = self.filtered_ipos.clone();
        self.table_state.update(cx, |state, cx| {
            state.delegate_mut().ipos = new_ipos;
            state.clear_selection(cx);
            state.refresh(cx);
        });
        self.selected_ipo = None;
        self.selected_ipo_subs.clear();
        cx.notify();
    }

    fn widths_to_str(widths: &[Pixels]) -> String {
        widths
            .iter()
            .map(|w| f32::from(*w).to_string())
            .collect::<Vec<_>>()
            .join(",")
    }

    fn str_to_widths(s: &str) -> Vec<Pixels> {
        s.split(',')
            .filter_map(|p| p.parse::<f32>().ok())
            .map(px)
            .collect()
    }

    fn icon_sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().downgrade();
        let collapsed = self.collapsed;
        v_flex()
            .w(px(48.))
            .h_full()
            .bg(rgb(0x18181b))
            .items_center()
            .pt(px(12.))
            .gap(px(2.))
            .child(
                div()
                    .size(px(32.))
                    .rounded(px(8.))
                    .bg(rgb(0x14b8a6))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().text_color(rgb(0xffffff)).font_bold().child("M")),
            )
            .child(div().flex_1())
            .child(
                Button::new("collapse")
                    .ghost()
                    .icon(if collapsed {
                        IconName::PanelLeftOpen
                    } else {
                        IconName::PanelLeftClose
                    })
                    .tooltip(if collapsed {
                        "Show sidebar"
                    } else {
                        "Hide sidebar"
                    })
                    .on_click(move |_, _, cx| {
                        entity
                            .update(cx, |this, cx| {
                                this.collapsed = !this.collapsed;
                                this.db
                                    .kv_set(
                                        "ui.collapsed",
                                        if this.collapsed { "true" } else { "false" },
                                    )
                                    .ok();
                                cx.notify();
                            })
                            .ok();
                    }),
            )
    }

    fn ipo_list_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().downgrade();

        let filter_chips = STATUSES.iter().map(|(key, label)| {
            let is_active = match (&self.selected_status, *key) {
                (None, "all") => true,
                (Some(s), k) => s == k,
                _ => false,
            };
            let key_str = *key;
                    h_flex().child(
                Button::new(key_str)
                    .label(*label)
                    .small()
                    .when(is_active, |b| b.primary())
                    .on_click({
                        let entity = entity.clone();
                        move |_, _, cx| {
                            let status = if key_str == "all" {
                                None
                            } else {
                                Some(key_str)
                            };
                            entity
                                .update(cx, move |this, cx| {
                                    this.set_filter(status, cx);
                                })
                                .ok();
                        }
                    }),
            )
        });

        v_flex()
            .w(px(280.))
            .h_full()
            .bg(rgb(0x1c1d1f))
            .border_r(px(1.))
            .border_color(rgb(0x2e2f32))
            .child(
                h_flex()
                    .px(px(12.))
                    .py(px(12.))
                    .gap(px(6.))
                    .children(filter_chips),
            )
            .child(
                v_flex()
                    .flex_1()
                    .child(Table::new(&self.table_state).stripe(true).bordered(true)),
            )
    }

    fn detail_panel(&mut self, cx: &mut Context<Self>) -> Div {
        v_flex()
            .flex_1()
            .h_full()
            .child(
                h_flex()
                    .px(px(16.))
                    .py(px(8.))
                    .bg(rgb(0x1c1d1f))
                    .border_b(px(1.))
                    .border_color(rgb(0x2e2f32))
                    .child(div().text_color(rgb(0x8b8d91)).text_sm().child(
                        match &self.last_sync {
                            Some(s) => format!("Last sync: {s}"),
                            None => "No sync yet".into(),
                        },
                    )),
            )
            .child(if let Some(ipo) = &self.selected_ipo {
                let entity = cx.entity().downgrade();
                let name = ipo.company_name.clone();
                let subs = self.selected_ipo_subs.clone();
                let ipo_clone = ipo.clone();
                v_flex()
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        // Back button + header row
                        h_flex()
                            .px(px(16.))
                            .py(px(8.))
                            .items_center()
                            .gap(px(12.))
                            .border_b(px(1.))
                            .border_color(rgb(0x2e2f32))
                            .child(
                                Button::new("back-to-list")
                                    .ghost()
                                    .label("Back")
                                    .on_click(move |_, _, cx| {
                                        entity
                                            .update(cx, |this, cx| {
                                                this.table_state
                                                    .update(cx, |state, cx| {
                                                        state.clear_selection(cx);
                                                    });
                                                this.selected_ipo = None;
                                                this.selected_ipo_subs.clear();
                                                cx.notify();
                                            })
                                            .ok();
                                    }),
                            )
                            .child(
                                div()
                                    .text_color(rgb(0xe4e5e7))
                                    .text_lg()
                                    .font_bold()
                                    .child(name),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .overflow_y_scrollbar()
                            .child(crate::ipo_detail::ipo_detail_body(&ipo_clone, &subs)),
                    )
            } else {
                v_flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .child(div().text_color(rgb(0x8b8d91)).child("Select an IPO"))
            })
    }
}

impl Render for MosaicApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .size_full()
            .bg(rgb(0x111113))
            .key_context(APP_CONTEXT)
            .on_action(cx.listener(Self::on_sync_now))
            .child(self.icon_sidebar(cx))
            .when(!self.collapsed, |this| {
                this.child(self.ipo_list_panel(window, cx))
            })
            .child(self.detail_panel(cx))
    }
}
