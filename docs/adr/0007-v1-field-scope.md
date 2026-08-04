# ADR-0007: v1 field scope — what the IPO record tracks

- **Status**: accepted
- **Date**: 2026-08-05
- **Decides**: wayfinder ticket "Decide: v1 IPO record field set" (#3)

## Context

NSE ipo-detail supplies issue period/size/price range/face value/bid lot/issue type/BRLM/registrar/UPI/doc links and per-category subscription. Chittorgarh adds allotment + listing dates, reservation percentages, fresh/OFS split, financials, valuation. Determinism and scope discipline decide what's in v1.

## Decision

Core: identity (company, symbol, exchange, sector), calendar/status (lifecycle, open/close/allotment/listing dates with tentative flag, DRHP/RHP URLs), pricing/sizing (band, final price, face value, lot size + multiples, issue size, fresh/OFS split, issue type, offer type), subscription snapshots (category × offered/bid/times), EOD history (OHLCV + volume + VWAP). Deferred to v2: financials/KPIs, valuation (P/E, market cap), reservation percentages, lot application ladder, shareholding, utilisation of proceeds, anchor details, parsed registrar/UPI fields. Dossier doc links open the RHP for everything deferred.

## Consequences

- One authoritative parse path (NSE JSON) + one table path (Chittorgarh reports) — no per-IPO detail scraping of Chittorgarh's fragile slug+ID pages in v1.
- The dossier is a traceability surface, not a data encyclopedia; the RHP is one click away.
