# ADR-0004: Three-pane desktop information architecture

- **Status**: accepted
- **Date**: 2026-08-05
- **Decides**: wayfinder ticket "Decide: UI information architecture" (#5)

## Context

Competitor research (Chittorgarh, Moneycontrol, Screener, Groww, Zerodha, Upstox, Investorgain, IPO Watch, Longbridge) found no desktop incumbent for Indian IPO tracking; the desktop precedent is pane-based terminals (Longbridge). Web tools converge on status-first segmentation, stat strips, dense sortable tables, and a deep per-IPO dossier.

## Decision

Three panes: (1) sidebar navigation (IPO list, dossier sections, sync status at bottom); (2) IPO list — 380px, filter pills (All/Upcoming/Open/Closed/Listed), stat strip (counts per status), dense sortable table with inline row expansion revealing per-category subscription (Screener pattern); (3) dossier — tabs Overview (hero line, key facts, timetable, issue split, doc links), Subscription (category × day grid with progress bars), Performance (close-price line chart vs issue-price reference line, OHLCV table), Docs. Status-first pills from Upstox/Groww, full schedule timeline from Zerodha, dual listing-gain/current-gain framing from Moneycontrol. Explicitly excluded: apply CTAs (read-only tracker), GMP, editorial blocks, nav overload.

## Consequences

- One global selection (selected IPO) drives both list highlight and dossier — simple state model.
- Hand-built tables/charts give dense rows (22px) and exact reference-line rendering.
- Panel widths and window state persist via the KVP store.
