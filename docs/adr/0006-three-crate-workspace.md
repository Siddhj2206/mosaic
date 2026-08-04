# ADR-0006: Three-crate workspace, no CLI

- **Status**: accepted
- **Date**: 2026-08-05
- **Decides**: wayfinder ticket "Decide: workspace & module layout" (#8); user decision to drop the CLI

## Context

MOSAIC.md proposed mosaic-core / mosaic-scrapers / root crate with two binaries (GUI + CLI). The user dropped the CLI for v1. Clean, maintainable, modular code is a stated goal.

## Decision

Three crates: `mosaic-core` (types, SQLite via rusqlite bundled, config, scraper trait, rate limiter — no HTTP deps), `mosaic-scrapers` (NSE client with cookie session, Chittorgarh report scraper, IPO Watch fallback, rate limiter — reqwest::blocking, scraper, csv), and root `mosaic` (single `[[bin]]`, GPUI app). Synchronous libs; no tokio anywhere. Module map: core `{lib,types,db,schema,error,config,scraper}.rs`; scrapers `{lib,nse,chittorgarh,ipowatch}.rs` + `test_fixtures/`; app `{main,app,theme}.rs` + `ui/{titlebar,sidebar,ipo_list,dossier,subscription,performance,statusbar}.rs`. Files ≤ ~300 lines. Tests: DB on temp files + atomic counter, scraper unit tests on `include_str!` fixtures, live-HTTP tests `#[ignore]`d.

## Consequences

- ToS-sensitive scraper code is isolated and reviewable in one crate.
- Root crate is thin UI glue; logic lives in core for testability.
- Adding a market later = new scraper module + migration, no rewrite.
