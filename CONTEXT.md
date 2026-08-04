# Mosaic — Context

Deterministic IPO tracker for India (NSE/BSE mainboard). Rust desktop app (GPUI), SQLite single source of truth. v1 covers the full IPO lifecycle: calendar → detail dossier → day-wise subscription → post-listing performance.

## Core principles

1. **No AI slop.** Every figure on screen traces to a stored record with a `source` and `ingested_at`. No LLM generates numbers, comps, or scores.
2. **Point-in-time, not current-state.** Subscription and price data are append-only snapshot rows. Historical views always reflect data as known at the time, never hindsight-revised.
3. **Real-money signals over text.** Subscription times-subscribed (actual capital commitments, NSE-published) is the primary demand signal. Textual sentiment has no place in v1.
4. **Provenance on every row.** `source` identifies the scraper that wrote the row; `ingested_at` pins when.

## Glossary

- **IPO lifecycle / status** — the live state machine on `ipos.status`: `upcoming` → `open` → `closed` → `listed`; `withdrawn` at any point. Derived chips ("closing today", "listing today") are UI-level, not stored.
- **Subscription category** — QIB (qualified institutional buyers, excluding anchors), NII (non-institutional investors), Retail (RII), Total. NSE publishes these four; the sNII/bNII split is Chittorgarh-only and deferred to v2.
- **Snapshot** — one append-only row set per (IPO, poll day, category). Day-wise subscription history is the accumulation of daily NSE polls. Same-day re-poll upserts (NSE revises intra-day; the last poll of the day wins).
- **Price history** — append-only OHLCV+VWAP rows per (IPO, trade_date) from listing day onward, NSE historicalOR-sourced. One row per day; re-ingestion upserts.
- **Sync run / ingestion run** — one background pass over the enabled sources; logged in `ingestion_runs` for audit. UI shows relative time of last successful run.
- **Dossier** — the detail view for one IPO: hero line, key facts, timetable, issue split, subscription grid, performance, doc links.
- **Provenance** — the (source, ingested_at) pair attached to every stored record; the answer to "where did this number come from".
- **Mainboard** — NSE mainboard segment (as opposed to SME). v1 tracks mainboard only. Chittorgarh's mainboard report is the filter authority.

## Data sources (v1)

| Data | Source | Notes |
|---|---|---|
| Calendar (upcoming/open) | NSE `api/all-upcoming-issues` | cookie session; filter `series==EQ` |
| Closed/listed history | Chittorgarh mainboard + listing-dates reports | server-rendered tables |
| IPO details | NSE `api/ipo-detail?symbol=X` | issueInfo.dataList + RHP/ANCHOR doc links |
| Subscription | NSE `api/ipo-detail` `activeCat.dataList`, polled daily during window | snapshots with `ingested_at` |
| EOD price history | NSE `api/historicalOR/generateSecurityWiseHistoricalData` | OHLCV+VWAP+volume, from listing day |

NSE access mechanics: fresh cookie jar per run (GET home page first), browser UA, `Referer: https://www.nseindia.com/`, ≥2s spacing. Re-session on 401/403/503.

## Architecture

- **Workspace**: `mosaic-core` (types, DB, config, trait — no HTTP), `mosaic-scrapers` (NSE/Chittorgarh/IPO Watch impls — reqwest::blocking), `mosaic` (GPUI bin). No tokio in libs; GPUI BackgroundExecutor for blocking work.
- **UI**: 3 panes — sidebar nav, IPO list (380px, sortable, inline category expansion), dossier with tabs (Overview / Subscription / Performance / Docs). Zed One Dark styling 1:1 via hand-rolled `theme.rs` palette (see ADR-0003).
- **Sync**: immediate first run; calendar every 6h; subscription daily 18:00 IST during windows; EOD daily 19:30 IST; backoff 30s→1h on failure; manual refresh resets backoff.

## Out of scope (v1)

GMP (unofficial/unsourced), SME IPOs, US/HK markets, allotment-status registrar lookup, anchor lock-ins, financials/KPIs/valuation, news/sentiment, comps engine, ownership tracking, CLI, web/mobile.

See `docs/adr/` for the decision record.
