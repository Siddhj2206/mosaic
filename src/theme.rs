//! The One Dark palette, extracted 1:1 from Zed's `assets/themes/one/one.json`
//! (ADR-0003). All values are literal — this file is the single source of
//! truth for Mosaic's look.

use gpui::{Global, Hsla, hsla};

/// Hex (0xRRGGBBAA or 0xRRGGBB) → Hsla.
pub fn hsla_from_hex(hex: u32) -> Hsla {
    let r = ((hex >> 24) & 0xff) as f32 / 255.0;
    let g = ((hex >> 16) & 0xff) as f32 / 255.0;
    let b = ((hex >> 8) & 0xff) as f32 / 255.0;
    let a = if hex & 0xff000000 == 0 {
        1.0
    } else {
        (hex & 0xff) as f32 / 255.0
    };

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let delta = max - min;

    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };
    let s = if delta == 0.0 {
        0.0
    } else {
        delta / (1.0 - (2.0 * l - 1.0).abs())
    };
    hsla(h, s, l, a)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    // Surfaces
    pub bg: Hsla,             // #3b414d — window/app background
    pub panel: Hsla,          // #2f343e — sidebar, tab bar, status bar, menus
    pub element: Hsla,        // #2e343e — buttons, inputs, chips
    pub editor: Hsla,         // #282c33 — list bg, active tab, toolbar
    // Element states
    pub hover: Hsla,          // #363c46
    pub active: Hsla,         // #454a56 — selected rows, pressed
    // Borders
    pub border: Hsla,         // #464b57
    pub border_variant: Hsla, // #363c46
    pub border_focused: Hsla, // #47679e
    // Text
    pub text: Hsla,           // #dce0e5
    pub text_muted: Hsla,     // #a9afbc
    pub text_disabled: Hsla,  // #878a98
    pub text_accent: Hsla,    // #74ade8
    // Status
    pub info: Hsla,           // #74ade8
    pub success: Hsla,        // #a1c181
    pub warning: Hsla,        // #dec184
    pub error: Hsla,          // #d07277
    // Market direction
    pub up: Hsla,             // #98c379 (Zed diff.plus)
    pub down: Hsla,           // #e06c75 (Zed diff.minus)
    // Scrollbar
    pub scrollbar_thumb: Hsla, // #c8ccd44c
}

impl Global for Palette {}

pub fn palette() -> Palette {
    Palette {
        bg: hsla_from_hex(0x3b414dff),
        panel: hsla_from_hex(0x2f343eff),
        element: hsla_from_hex(0x2e343eff),
        editor: hsla_from_hex(0x282c33ff),
        hover: hsla_from_hex(0x363c46ff),
        active: hsla_from_hex(0x454a56ff),
        border: hsla_from_hex(0x464b57ff),
        border_variant: hsla_from_hex(0x363c46ff),
        border_focused: hsla_from_hex(0x47679eff),
        text: hsla_from_hex(0xdce0e5ff),
        text_muted: hsla_from_hex(0xa9afbcff),
        text_disabled: hsla_from_hex(0x878a98ff),
        text_accent: hsla_from_hex(0x74ade8ff),
        info: hsla_from_hex(0x74ade8ff),
        success: hsla_from_hex(0xa1c181ff),
        warning: hsla_from_hex(0xdec184ff),
        error: hsla_from_hex(0xd07277ff),
        up: hsla_from_hex(0x98c379ff),
        down: hsla_from_hex(0xe06c75ff),
        scrollbar_thumb: hsla_from_hex(0xc8ccd44c),
    }
}

// ---------------------------------------------------------------------------
// Spacing scale (Zed DynamicSpacing at default density)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[allow(dead_code)]
pub const S1: f32 = 4.0;
pub const S2: f32 = 8.0;
pub const S3: f32 = 12.0;
pub const S4: f32 = 16.0;
#[allow(dead_code)]
pub const S6: f32 = 24.0;
#[allow(dead_code)]
pub const S8: f32 = 32.0;

// ---------------------------------------------------------------------------
// Typography (Zed TextSize)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub const FONT_XS: f32 = 10.0;
pub const FONT_SM: f32 = 12.0;
pub const FONT_UI: f32 = 14.0;
pub const FONT_LG: f32 = 16.0;
#[allow(dead_code)]
pub const HEADLINE: f32 = 18.0;

// ---------------------------------------------------------------------------
// Component metrics (Zed)
// ---------------------------------------------------------------------------

pub const TITLEBAR_H: f32 = 34.0;
pub const TABBAR_H: f32 = 32.0;
pub const ROW_H: f32 = 22.0;
#[allow(dead_code)]
pub const BUTTON_H: f32 = 28.0;
pub const BUTTON_SM_H: f32 = 22.0;
pub const INPUT_H: f32 = 32.0;
pub const STATUSBAR_H: f32 = 26.0;
pub const RADIUS_SM: f32 = 4.0;
pub const RADIUS_MD: f32 = 6.0;
#[allow(dead_code)]
pub const RADIUS_LG: f32 = 8.0;
#[allow(dead_code)]
pub const SCROLLBAR_W: f32 = 6.0;
