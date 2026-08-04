# ADR-0001: NSE official JSON API as primary data source

- **Status**: accepted
- **Date**: 2026-08-05
- **Decides**: wayfinder ticket "Decide: data source matrix for v1" (#2)

## Context

Mosaic needs calendar, IPO details, subscription, and EOD history for India mainboard IPOs, all deterministically traceable. MOSAIC.md proposed Chittorgarh + bhavcopy CSVs. Live research (2026-08-05) showed: NSE has a working cookie-authenticated JSON API family covering all four data types; BSE is session/captcha-gated; bhavcopy CSVs were discontinued 2024-07-08 (NSE circular 62424); Moneycontrol 403s; Investorgain is JS-only; Stooq has a proof-of-work wall.

## Decision

NSE official endpoints are primary for all four data types. Chittorgarh server-rendered reports are the fallback/backfill for closed and listed IPOs (NSE only exposes open/upcoming). Subscription is NSE `ipo-detail` polled daily during the bidding window — the poll time and `ingested_at` are the provenance anchor (NSE publishes no subscription timestamps; aggregators do exactly this). BSE, Moneycontrol, Investorgain, Stooq, and bhavcopy CSVs are rejected for v1.

## Consequences

- One authoritative parse path (NSE JSON) + one table-sourced path (Chittorgarh reports). Small surface for ToS review.
- NSE endpoint fragility: the API family has moved twice in three years. Each scraper carries a health-check/parse-version; sync surfaces failures in the UI.
- No sNII/bNII subscription split in v1 (NSE publishes NII as one category).
- NSE ToS: personal use of site data is tolerated; bulk redistribution requires a license. Mosaic is a local personal tracker — compliant. Revisit before any public redistribution.
