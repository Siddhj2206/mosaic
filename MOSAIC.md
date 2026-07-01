# Mosaic

A professional, deterministic IPO tracker — built to be a tool I'd actually use, not a demo of an LLM summarizing financial data.

---

## 1. Vision

Mosaic tracks upcoming, current, and past IPO listings, starting with the Indian market (NSE/BSE). It surfaces pricing, valuation, subscription demand, and post-listing performance — all computed from real, sourced data rather than generated or inferred.

**Eventual scope** (beyond v1): historical comparisons against similar past IPOs, ownership/shareholder tracking, news and sentiment integration, and expansion from IPOs to ongoing company tracking more broadly. The schema and architecture are designed so this growth doesn't require a rewrite.

### Core principle: no AI slop

This is the design constraint that shapes everything else. Mosaic does not use an LLM to generate numbers, comps, or sentiment scores. Every figure on screen must trace back to a stored record with a source and a timestamp. If an LLM-based feature is ever added (e.g. summarizing a filing), it summarizes data the deterministic engine already extracted — it never produces the numbers itself.

This shows up in three concrete rules:

1. **Point-in-time storage, not current-state storage.** Every ingested record carries an `as_of_date` and an `ingested_at`. Updates are new rows, not overwrites. A historical comparison should always use the data as it was actually known at the time, not data quietly revised by hindsight.
2. **Real-money signals are trusted over text signals.** IPO subscription data (QIB/NII/Retail bids — actual capital commitments) is treated as the primary demand signal. News/social sentiment, where added, is a clearly labeled secondary panel with a published, versioned scoring methodology — never an opaque LLM call standing in for a number.
3. **Every record has a `source` and `ingested_at` field.** When something looks wrong, it should always be traceable to a specific scrape and a specific time.

---

## 2. Scope

### v1 (this build)
- **Market**: India only (NSE/BSE), mainboard IPOs only (SME IPOs deferred — smaller, noisier data)
- **Domain**: IPOs only — no broader company tracking yet
- **Form factor**: local desktop app, no cloud/hosting dependency

### Explicitly deferred to v2+
- Comparable-company analysis engine
- Ownership / shareholder / promoter / anchor-investor tracking
- News and sentiment integration (GDELT, financial news APIs)
- US market (SEC EDGAR, S-1 filings)
- Expansion beyond IPOs to general company tracking
- GMP (Grey Market Premium) — excluded from v1 because it's unofficial and unsourced (no regulatory filing backs it); revisit once a reliable source is identified

---

## 3. Data model

### IPO lifecycle (status field on `ipos`)
`upcoming` → `open` → `closed` → `listed` (or `withdrawn` at any point)

### v1 schema

```sql
-- Core IPO record — one row per company's IPO
CREATE TABLE ipos (
    id              INTEGER PRIMARY KEY,
    company_name    TEXT NOT NULL,
    symbol          TEXT,                 -- NSE/BSE ticker, NULL until allotted
    exchange        TEXT,                 -- 'NSE', 'BSE', 'NSE+BSE'
    sector          TEXT,
    offer_type      TEXT NOT NULL,        -- 'fresh_issue', 'ofs', 'mixed'

    price_band_low  REAL,
    price_band_high REAL,
    final_price     REAL,                 -- NULL until priced
    lot_size        INTEGER,

    shares_offered           INTEGER,
    fresh_issue_shares       INTEGER,
    ofs_shares                INTEGER,
    shares_outstanding_post   INTEGER,
    issue_size_inr             REAL,      -- computed: price * shares_offered

    open_date       TEXT,                 -- ISO date strings
    close_date      TEXT,
    allotment_date  TEXT,
    listing_date    TEXT,

    status          TEXT NOT NULL,        -- 'upcoming', 'open', 'closed', 'listed', 'withdrawn'
    drhp_url        TEXT,
    rhp_url         TEXT,

    source          TEXT NOT NULL,
    ingested_at     TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

-- Subscription data — append-only, multiple snapshots per day during bidding window
CREATE TABLE subscription_snapshots (
    id              INTEGER PRIMARY KEY,
    ipo_id          INTEGER NOT NULL REFERENCES ipos(id),
    snapshot_at     TEXT NOT NULL,        -- full timestamp

    qib_x           REAL,                 -- times subscribed
    snii_x          REAL,
    bnii_x          REAL,
    retail_x        REAL,
    employee_x      REAL,
    total_x         REAL,

    source          TEXT NOT NULL,
    ingested_at     TEXT NOT NULL
);

-- Daily price history — covers listing day and ongoing post-listing tracking
CREATE TABLE price_history (
    id              INTEGER PRIMARY KEY,
    ipo_id          INTEGER NOT NULL REFERENCES ipos(id),
    trade_date      TEXT NOT NULL,

    open_price      REAL,
    high_price      REAL,
    low_price       REAL,
    close_price     REAL,
    volume          INTEGER,

    source          TEXT NOT NULL,
    ingested_at     TEXT NOT NULL,

    UNIQUE(ipo_id, trade_date)
);

-- Ingestion run log — not user-facing, but essential once real scrapers are running
CREATE TABLE ingestion_runs (
    id              INTEGER PRIMARY KEY,
    source          TEXT NOT NULL,
    started_at      TEXT NOT NULL,
    finished_at     TEXT,
    status          TEXT,                 -- 'success', 'partial', 'failed'
    records_written INTEGER,
    notes           TEXT
);

CREATE INDEX idx_subscription_ipo ON subscription_snapshots(ipo_id, snapshot_at);
CREATE INDEX idx_price_ipo ON price_history(ipo_id, trade_date);
CREATE INDEX idx_ipos_status ON ipos(status);
```

### Schema design notes
- `subscription_snapshots` and `price_history` are append-only by design — never updated in place, only inserted. This is the point-in-time discipline applied directly.
- `ipos` is a mutable current-state row for v1 simplicity. Revisit with a revision-history table if DRHP corrections turn out to matter in practice.
- `companies` (v2, beyond-IPOs) generalizes naturally from `ipos` minus the offering-specific fields; `price_history` already works for any ticker, not just post-IPO ones.

### v2+ schema additions (not built yet, kept in mind)
- `people` / `entities` + `ownership_stakes` (holder, company, role: founder/board/institutional/anchor, stake %, as-of date, source filing) — normalized rather than text dumped into the IPO record
- `news_articles` (immutable: title, source, URL, published date, retrieved date, entity-tagging method + confidence) + a separate `sentiment_scores` table referencing both the article and a `methodology_version`, so re-scoring later never destroys or silently changes history

---

## 4. v1 screens

1. **Calendar/list view** — upcoming, open, recently listed; filterable
2. **IPO detail view** — price band, lot size, issue size, dates, OFS vs fresh split, DRHP link
3. **Subscription tracker** — QIB/NII/Retail/total over the bidding window, charted live while an IPO is open
4. **Listing performance view** — issue price vs day-1 and ongoing price, once listed

---

## 5. Data sources (India)

| Data | Source | Notes |
|---|---|---|
| IPO calendar, DRHP filings | Chittorgarh | Scraped HTML tables; closest thing to a structured archive for Indian IPOs |
| Subscription status (QIB/NII/Retail) | NSE/BSE live data | Published daily during the bidding window |
| Post-listing price history | NSE/BSE (bhavcopy) or Yahoo Finance (`.NS`/`.BO` suffix via `yfinance-rs`) | NSE's official historical API is paid/SFTP; Yahoo Finance is the pragmatic free fallback for ongoing price tracking once a ticker exists |
| GMP | *(excluded from v1)* | Unofficial, unsourced — no regulatory filing backs it |

**Open item**: confirm NSE/BSE data terms of use before any redistribution beyond personal/portfolio use. Free scrapers found during research may not match the exchanges' terms for republishing data.

---

## 6. Technology stack

### Form factor
Local desktop app. SQLite as the single source of truth — no server, no hosting, no "where does state live" problem.

### Why all-Rust (not split with Python)
Originally considered splitting ingestion (Python) from the app (Rust) to lean on Python's scraping ecosystem. Decided against it: none of the actual scraping work (HTTP calls, HTML table parsing, JSON parsing) needs pandas-level tooling, Rust's scraping crates are fully capable, and a single language avoids an unnecessary seam — also better serves the original goal of learning Rust deeply.

### Project structure
Single Cargo workspace, three crates:
- **`mosaic-core`** — shared types (`Ipo`, `SubscriptionSnapshot`, `PricePoint`, etc.) matching the schema, plus SQLite read/write logic. Single source of truth for the data model.
- **`mosaic-ingest`** — CLI binary running the scrapers, writing into SQLite via `mosaic-core`. v1: run manually when fresh data is wanted, no scheduler yet.
- **`mosaic-app`** — the GPUI desktop app, reads from SQLite via `mosaic-core`, renders the four v1 screens.

### Dependencies

| Crate | Purpose | Notes |
|---|---|---|
| `gpui` + `gpui-component` | UI framework + components | Zed's GPU-accelerated UI framework; `gpui-component` (built by Longbridge for their production trading terminal, Longbridge Pro) adds virtualized tables and built-in Line/Bar/Area/Pie charts on top of raw GPUI. Pre-1.0 — pin to specific commits. **Verify current Windows support directly before relying on it** (sources disagreed during research). |
| `reqwest` | HTTP client | Standard choice, async via `tokio` |
| `scraper` | HTML parsing (CSS selectors) | For Chittorgarh tables etc. |
| `serde` / `serde_json` | Serialization | For JSON endpoints (NSE/BSE) |
| `csv` | CSV parsing | For bhavcopy files |
| `jiff` | Date/time | **Not `chrono`** — chrono is now soft-deprecated by its own author as of 2026; jiff is the recommended successor, with better timezone handling and faster parsing |
| `rust_decimal` | Financial arithmetic | Avoids float precision errors in prices/valuations; the most widely-used decimal crate in the Rust ecosystem |
| `rusqlite` | SQLite access | Chosen over `sqlx` — simpler, synchronous (no network latency to hide in a local single-file DB), and the `bundled` feature simplifies Windows builds |
| `yfinance-rs` *(optional, v1.1+)* | Post-listing price history fallback | Async Rust client mirroring Python's `yfinance`; supports `.NS`/`.BO` tickers. Does not cover IPO-specific data (subscription, DRHP) — only useful once a company has a listed ticker |

### Things to verify before/early in implementation
- GPUI / gpui-component actual current platform support (macOS/Linux confirmed; Windows claimed by some sources, not by GPUI's own docs at time of research)
- NSE/BSE data terms of use for the scraping approach
- Pin `gpui`/`gpui-component` to specific commits from the first commit, not after something breaks

---

## 7. Roadmap

**v1** — IPO calendar, subscription tracking, listing performance. India only. (this document)

**v2 candidates** (not sequenced yet):
- Comps engine: rules-based similarity scoring (sector, revenue range, growth bucket, offer type, market cap tier) with transparent weights — not an LLM judgment call
- Ownership/shareholder tracking: promoters, anchor investors, post-IPO shareholding pattern from RHP disclosures
- News/sentiment: GDELT for historical tone/volume timelines, ticker-tagged financial news APIs for sentiment with a versioned, published methodology
- Market conditions composite: recent IPO index performance, India VIX, trailing-quarter withdrawal rate
- US market expansion: SEC EDGAR (S-1 filings, Form 3/4/13D-G for ownership), reusing the same point-in-time architecture
- Generalize beyond IPOs to ongoing company tracking (schema already supports this with minor extension)
