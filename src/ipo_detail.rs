use std::collections::HashMap;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::scroll::ScrollableElement;
use gpui_component::{h_flex, v_flex, Disableable, StyledExt};
use mosaic_core::db::types_sql::decimal_to_f64_opt;
use mosaic_core::types::{Ipo, SubscriptionEntry};

const SUB_CATEGORIES: &[&str] = &["QIB", "NII", "NII (Big)", "NII (Small)", "RII", "EMP", "Total"];

pub fn ipo_detail_body(ipo: &Ipo, subscriptions: &[SubscriptionEntry]) -> impl IntoElement {
    let subs: HashMap<&str, rust_decimal::Decimal> = subscriptions
        .iter()
        .filter_map(|e| e.subscribed.map(|v| (e.category.as_str(), v)))
        .collect();

    let max_sub = subs
        .values()
        .filter(|v| **v > rust_decimal::Decimal::ZERO)
        .cloned()
        .max()
        .unwrap_or(rust_decimal::Decimal::ONE);

    let has_rhp = ipo.rhp_url.is_some();
    let has_drhp = ipo.drhp_url.is_some();

    v_flex()
        .flex_1()
        .px(px(24.))
        .py(px(16.))
        .gap(px(24.))
        .overflow_y_scrollbar()
        .child(info_card(ipo))
        .child(timetable_card(ipo))
        .when(!subs.is_empty(), |this| {
            this.child(subscription_card(&subs, max_sub))
        })
        .when(has_rhp || has_drhp, |this| this.child(documents_card(ipo)))
}

fn info_card(ipo: &Ipo) -> Div {
    let exchange = ipo.exchange.clone().unwrap_or_default();
    let price = match (ipo.price_band_low, ipo.price_band_high) {
        (Some(l), Some(h)) => format!("\u{20b9}{l} - \u{20b9}{h}"),
        (Some(l), None) => format!("\u{20b9}{l}"),
        _ => "-".into(),
    };
    let lot = ipo
        .lot_size
        .map(|v| format!("{v} shares"))
        .unwrap_or("-".into());
    let issue = ipo
        .issue_size
        .map(|v| format!("\u{20b9}{v:.2} Cr"))
        .unwrap_or("-".into());

    section_card("IPO Details")
        .child(
            h_flex()
                .gap(px(8.))
                .child(label("Exchange"))
                .child(value(exchange)),
        )
        .child(
            h_flex()
                .gap(px(8.))
                .child(label("Price Band"))
                .child(value(price)),
        )
        .child(
            h_flex()
                .gap(px(8.))
                .child(label("Lot Size"))
                .child(value(lot)),
        )
        .child(
            h_flex()
                .gap(px(8.))
                .child(label("Issue Size"))
                .child(value(issue)),
        )
}

fn timetable_card(ipo: &Ipo) -> Div {
    section_card("Timetable")
        .child(
            h_flex()
                .gap(px(8.))
                .child(label("Open Date"))
                .child(value(ipo.open_date.clone().unwrap_or_default())),
        )
        .child(
            h_flex()
                .gap(px(8.))
                .child(label("Close Date"))
                .child(value(ipo.close_date.clone().unwrap_or_default())),
        )
        .child(
            h_flex()
                .gap(px(8.))
                .child(label("Allotment"))
                .child(value(ipo.allotment_date.clone().unwrap_or_default())),
        )
        .child(
            h_flex()
                .gap(px(8.))
                .child(label("Listing"))
                .child(value(ipo.listing_date.clone().unwrap_or_default())),
        )
}

fn subscription_card(
    subs: &HashMap<&str, rust_decimal::Decimal>,
    max_sub: rust_decimal::Decimal,
) -> Div {
    let max_f64 = decimal_to_f64_opt(Some(max_sub)).unwrap_or(1.0);

    section_card("Subscription (× times)").children(
        SUB_CATEGORIES.iter().filter_map(|cat| {
            let val = subs.get(cat)?;
            Some(subscription_bar(*cat, *val, max_f64))
        }),
    )
}

fn documents_card(ipo: &Ipo) -> Div {
    let has_rhp = ipo.rhp_url.is_some();
    let has_drhp = ipo.drhp_url.is_some();
    let rhp_url = ipo.rhp_url.clone();
    let drhp_url = ipo.drhp_url.clone();
    section_card("Documents")
        .child(
            h_flex()
                .gap(px(12.))
                .child(
                    Button::new("rhp")
                        .ghost()
                        .label("RHP")
                        .when(!has_rhp, |b| b.disabled(true))
                        .on_click({
                            let url = rhp_url.clone();
                            move |_, _, _| {
                                if let Some(u) = &url {
                                    let _ = open::that(u);
                                }
                            }
                        }),
                )
                .child(
                    Button::new("drhp")
                        .ghost()
                        .label("DRHP")
                        .when(!has_drhp, |b| b.disabled(true))
                        .on_click({
                            let url = drhp_url.clone();
                            move |_, _, _| {
                                if let Some(u) = &url {
                                    let _ = open::that(u);
                                }
                            }
                        }),
                ),
        )
}

fn section_card(title: impl Into<SharedString>) -> Div {
    let t: SharedString = title.into();
    v_flex()
        .gap(px(8.))
        .child(
            div()
                .text_color(rgb(0x8b8d91))
                .text_xs()
                .child(t.to_uppercase()),
        )
}

fn label(text: impl Into<SharedString>) -> Div {
    div().w(px(100.)).text_color(rgb(0x8b8d91)).child(text.into())
}

fn value(text: impl Into<SharedString>) -> Div {
    div().text_color(rgb(0xe4e5e7)).child(text.into())
}

fn subscription_bar(cat: impl Into<SharedString>, val: rust_decimal::Decimal, max_f64: f64) -> Div {
    let val_f64 = decimal_to_f64_opt(Some(val)).unwrap_or(0.0);
    let pct = if max_f64 > 0.0 {
        (val_f64 / max_f64).min(1.0)
    } else {
        0.0
    };
    let bar_px = (pct * 200.0).max(4.0);

    let cat_str: SharedString = cat.into();
    h_flex()
        .gap(px(12.))
        .items_center()
        .child(div().w(px(80.)).text_color(rgb(0x8b8d91)).child(cat_str))
        .child(
            div()
                .w(px(200.))
                .h(px(8.))
                .bg(rgb(0x2e2f32))
                .rounded(px(4.))
                .overflow_hidden()
                .child(
                    div()
                        .h_full()
                        .w(px(bar_px as f32))
                        .bg(rgb(0x14b8a6))
                        .rounded(px(4.)),
                ),
        )
        .child(
            div()
                .w(px(70.))
                .text_right()
                .text_color(rgb(0x14b8a6))
                .font_bold()
                .child(format!("{val:.2}×")),
        )
}

