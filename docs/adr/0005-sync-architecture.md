# ADR-0005: Sync architecture — cadenced polling with provenance

- **Status**: accepted
- **Date**: 2026-08-05
- **Decides**: wayfinder ticket "Decide: sync & ingestion architecture" (#7)

## Context

The GUI must keep data fresh without user action, poll NSE politely (≥2s spacing, cookie sessions), and never lose traceability. MOSAIC.md §7 defined a backoff loop; research added NSE session mechanics and the historicalOR endpoint for EOD.

## Decision

Immediate sync on launch, then a loop inside the `MosaicApp` entity (GPUI BackgroundExecutor, no tokio). Cadence: calendar + Chittorgarh closed/listed refresh every 6h; subscription poll daily at 18:00 IST during each bidding window; EOD pull daily at 19:30 IST for newly listed IPOs and gaps. Per-run NSE session: fresh cookie jar (GET home page → cookies), browser UA, Referer, ≥2s spacing via a shared per-source `RateLimiter`. Backoff 30s→1m→2m→4m→…→1h on failure; manual refresh resets to immediate. Every run logs an `ingestion_runs` row; every write carries `source` + `ingested_at`; the UI status bar shows last-sync relative time and last error.

## Consequences

- Day-wise subscription snapshots accumulate from daily 18:00 polls; a re-poll same-day upserts (NSE revises intra-day).
- First run backfills the current 2026 archive from Chittorgarh reports (mainboard list + listing dates), then EOD history from listing_date→today via historicalOR.
- Scraper health is observable: parse-failure counters and last-ok timestamps per source surface in sync status.
