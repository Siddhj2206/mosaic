# ADR-0002: Point-in-time schema — append-only snapshots

- **Status**: accepted
- **Date**: 2026-08-05
- **Decides**: wayfinder ticket "Decide: SQLite schema (point-in-time model)" (#4)

## Context

Mosaic's core principle is that historical views use data as known at the time. Subscription and price data change over time (NSE revises intra-day; day-wise subscription accumulates). MOSAIC.md proposed normalized append-only snapshot tables.

## Decision

Five tables in v1: `markets` (seeded India-only), `ipos` (mutable current-state row with status lifecycle), `subscription_snapshots` (append-only, one row per IPO × snapshot day × category, `UNIQUE(ipo_id, snapshot_at, category)` upsert), `price_history` (append-only, `UNIQUE(ipo_id, trade_date)` upsert), `ingestion_runs` (audit log). No `exchange_rates` stub in v1. Migrations via static `MIGRATIONS: &[&str]` array + `PRAGMA user_version`.

## Consequences

- Same-day re-polls upsert rather than accumulate — correct, because NSE revises intra-day and the last poll of the day is the most accurate known value.
- `ipos` is mutable; status transitions are visible via `updated_at`. A full event log is v2 material (listing-day performance derives from `price_history` anyway).
- Money stored as REAL (f64) per convention; `rust_decimal` in the app layer.
- Cross-market growth (US/HK) later means a new migration adding `market_id`-style columns — not a rewrite.
