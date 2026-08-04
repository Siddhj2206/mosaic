# ADR-0003: Zed One Dark styling via hand-rolled theme module

- **Status**: accepted
- **Date**: 2026-08-05
- **Decides**: wayfinder ticket "Decide: Zed theme system (1:1 styling)" (#6)

## Context

The user requires Zed's styling matched 1:1. Zed's tokens were extracted from the local `Reference/zed` clone (`assets/themes/one/one.json`, `crates/ui/src/styles/`, component sources). gpui-component's theme system would constrain us to its palette vocabulary; Mosaic's app chrome is hand-built divs.

## Decision

A typed palette struct in `src/theme.rs` with literal One Dark values (background `#3b414d`, panel `#2f343e`, element `#2e343e`, editor `#282c33`, hover `#363c46`, active/selected `#454a56`, border `#464b57`, border-variant `#363c46`, border-focused `#47679e`, text `#dce0e5`, muted `#a9afbc`, disabled `#878a98`, accent `#74ade8`, status info/success/warning/error `#74ade8/#a1c181/#dec184/#d07277`, up `#98c379`, down `#e06c75`, scrollbar thumb `#c8ccd44c`). Typography: 14px base UI, 12px small, 16px large, 18px headline, line-height 1.6rem; IBM Plex Sans preferred with system fallback. Metrics: title bar 34px, tab bar 32px, list rows 22px, buttons 28/22px with 4px radius, inputs 32px min-height with 6px radius, scrollbars 6px, floating surfaces 8px radius. Spacing scale 4/6/8/12/16/24. A minimal gpui `Theme` is registered so GPUI-native chrome (menus, tooltips) matches.

## Consequences

- Full control of density and every pixel — tables and charts are hand-built, not gpui-component.
- No font bundling in v1 (binary size); falls back gracefully if IBM Plex Sans is absent.
- Theme tokens live in one file; a future light theme or theme switcher is a palette swap.
