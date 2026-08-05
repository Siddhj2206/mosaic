//! Shared layout helpers and small widgets used across the UI.
pub mod dossier;
pub mod format;
pub mod ipo_list;
pub mod sidebar;
pub mod statusbar;
pub mod titlebar;


use gpui::prelude::*;
use gpui::{Div, SharedString, div, px};

use crate::theme::{Palette, S1, S2, RADIUS_SM, FONT_SM, FONT_UI};

/// `h_flex` — row flexbox with centered items (Zed's `h_flex`).
pub fn h_flex() -> Div {
    div().flex().flex_row().items_center()
}

/// `v_flex` — column flexbox.
pub fn v_flex() -> Div {
    div().flex().flex_col()
}

/// Status badge pill (lifecycle color-coded).
pub fn status_badge(status: &str, palette: &Palette) -> Div {
    let (fg, bg) = match status {
        "upcoming" => (palette.text_accent, rgba(0x74ade8, 0x1a)),
        "open" => (palette.success, rgba(0xa1c181, 0x1a)),
        "closed" => (palette.warning, rgba(0xdec184, 0x1a)),
        "listed" => (palette.info, rgba(0x74ade8, 0x1a)),
        "withdrawn" => (palette.text_disabled, rgba(0x878a98, 0x1a)),
        _ => (palette.text_muted, palette.element),
    };
    h_flex()
        .h(px(18.))
        .px(px(6.))
        .rounded(px(RADIUS_SM))
        .bg(bg)
        .text_color(fg)
        .text_size(px(FONT_SM))
        .child(status.to_string())
}

fn rgba(hex: u32, alpha: u32) -> gpui::Hsla {
    let mut h = hex;
    h = (h << 8) | alpha;
    crate::theme::hsla_from_hex(h)
}

/// A key-value row for the dossier ("Price Band" — "₹50 – ₹53").
pub fn kv_row(label: &str, value: impl Into<SharedString>, palette: &Palette) -> Div {
    h_flex()
        .justify_between()
        .py(px(2.))
        .child(
            div()
                .text_size(px(FONT_SM))
                .text_color(palette.text_muted)
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(px(FONT_UI))
                .text_color(palette.text)
                .child(value.into()),
        )
}

/// Section header with a muted label.
pub fn section_label(label: &str, palette: &Palette) -> Div {
    div()
        .text_size(px(FONT_SM))
        .text_color(palette.text_disabled)
        .mb(px(S1))
        .child(label.to_string())
}

/// Muted empty-state block.
pub fn empty_state(text: &str, palette: &Palette) -> Div {
    v_flex()
        .flex_1()
        .items_center()
        .justify_center()
        .gap(px(S2))
        .text_color(palette.text_muted)
        .child(div().text_size(px(FONT_UI)).child(text.to_string()))
}
