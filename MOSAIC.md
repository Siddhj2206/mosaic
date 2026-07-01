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
-- Markets reference table — seed once, grows as markets are added
CREATE TABLE markets (
    id              TEXT PRIMARY KEY,    -- 'in', 'us', 'hk'
    name            TEXT NOT NULL,
    currency        TEXT NOT NULL,       -- 'INR', 'USD', 'HKD'
    currency_symbol TEXT NOT NULL        -- '₹', '$', 'HK$'
);

-- Seed data for v1
INSERT INTO markets (id, name, currency, currency_symbol) VALUES
    ('in', 'India', 'INR', '₹'),
    ('us', 'United States', 'USD', '$'),
    ('hk', 'Hong Kong', 'HKD', 'HK$');

-- Core IPO record — one row per company's IPO
CREATE TABLE ipos (
    id              INTEGER PRIMARY KEY,
    market_id       TEXT NOT NULL DEFAULT 'in' REFERENCES markets(id),
    company_name    TEXT NOT NULL,
    symbol          TEXT,                 -- ticker, NULL until allotted
    exchange        TEXT,                 -- per-market free text
    sector          TEXT,
    offer_type      TEXT,                 -- per-market free text: 'fresh_issue', 'ofs', 'pure_primary', etc.

    price_band_low  REAL,
    price_band_high REAL,
    final_price     REAL,                 -- NULL until priced
    lot_size        INTEGER,

    shares_offered           INTEGER,
    fresh_issue_shares       INTEGER,
    ofs_shares                INTEGER,
    shares_outstanding_post   INTEGER,
    issue_size                 REAL,      -- computed: price * shares_offered; currency from markets

    open_date       TEXT,                 -- ISO date strings
    close_date      TEXT,
    allotment_date  TEXT,
    listing_date    TEXT,

    status          TEXT NOT NULL,        -- 'upcoming', 'open', 'closed', 'listed', 'withdrawn'
    drhp_url        TEXT,                 -- filing URLs, per-market semantics
    rhp_url         TEXT,

    source          TEXT NOT NULL,
    ingested_at     TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

-- Subscription data — normalized category/value rows for multi-market support
-- Each snapshot_at + ipo_id produces multiple rows (one per category)
CREATE TABLE subscription_snapshots (
    id              INTEGER PRIMARY KEY,
    ipo_id          INTEGER NOT NULL REFERENCES ipos(id),
    snapshot_at     TEXT NOT NULL,        -- full timestamp
    category        TEXT NOT NULL,        -- 'qib', 'nii', 'retail', 'employee', 'institutional', 'placing', 'public', etc.
    subscribed      REAL,                 -- times subscribed (x)

    source          TEXT NOT NULL,
    ingested_at     TEXT NOT NULL
);

-- Daily price history — covers listing day and ongoing post-listing tracking
-- Already generic: works for any ticker, any market
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

-- Exchange rates for cross-market comparisons (v2+)
CREATE TABLE exchange_rates (
    id              INTEGER PRIMARY KEY,
    from_currency   TEXT NOT NULL,
    to_currency     TEXT NOT NULL,
    date            TEXT NOT NULL,
    rate            REAL NOT NULL,
    source          TEXT NOT NULL,
    UNIQUE(from_currency, to_currency, date)
);

CREATE INDEX idx_sub_ipo_cat ON subscription_snapshots(ipo_id, snapshot_at, category);
CREATE INDEX idx_price_ipo ON price_history(ipo_id, trade_date);
CREATE INDEX idx_ipos_status ON ipos(status);
CREATE INDEX idx_ipos_market ON ipos(market_id);
CREATE INDEX idx_xrate_pair ON exchange_rates(from_currency, to_currency, date);
```

### Schema design notes

- `subscription_snapshots` and `price_history` are append-only by design — never updated in place, only inserted. This is the point-in-time discipline applied directly.
- `ipos` is a mutable current-state row for v1 simplicity. Revisit with a revision-history table if DRHP corrections turn out to matter in practice.
- `subscription_snapshots` uses normalized category/value rows instead of fixed columns (`qib_x`, `retail_x`, etc.) to support multiple markets without schema changes. Each market defines its own categories. The app layer pivots rows into columns for display.
- A `markets` table with seed data allows the same schema to serve India, US, Hong Kong, and beyond. Adding a market means implementing a scraper and defining market-specific categories — no migrations needed.
- `currency_symbol` is stored on `markets` for display formatting. `Market::format_amount(Decimal) -> String` in `mosaic-core` handles currency-aware output.
- `exchange_rates` is created now (empty) so the schema is stable when cross-market comparisons are added. Not used in v1.
- `price_history` is already generic and works for any ticker and market. This is the most future-proof table in the schema.
- `companies` (v2, beyond-IPOs) generalizes naturally from `ipos` minus the offering-specific fields; `price_history` already works for any ticker, not just post-IPO ones.

### v2+ schema additions (not built yet, kept in mind)

- `people` / `entities` + `ownership_stakes` (holder, company, role: founder/board/institutional/anchor, stake %, as-of date, source filing) — normalized rather than text dumped into the IPO record
- `news_articles` (immutable: title, source, URL, published date, retrieved date, entity-tagging method + confidence) + a separate `sentiment_scores` table referencing both the article and a `methodology_version`, so re-scoring later never destroys or silently changes history

---

## 4. v1 UI

### Layout (Longbreak Pro-inspired)

```
┌──────────────────────────────────────────────────────────────────┐
│ ┌────────┐ ┌──────────────┐ ┌────────────────────┐ ┌───────────┐│
│ │        │ │              │ │                    │ │           ││
│ │  Icons │ │  IPO List    │ │   Main Content     │ │  IPO      ││
│ │  (60px)│ │  (300px)     │ │   (flex)           │ │  Detail   ││
│ │        │ │              │ │                    │ │  (250px)  ││
│ │  📅    │ │  Company     │ │   Chart / Table    │ │           ││
│ │  📋    │ │  Price Band  │ │   (changes per     │ │  Company  ││
│ │  📊    │ │  Dates       │ │    sidebar nav)    │ │  Price    ││
│ │  📈    │ │  Status      │ │                    │ │  Status   ││
│ │  👁    │ │              │ │                    │ │  ...      ││
│ │        │ ├──────────────┤ │                    │ │           ││
│ │        │ │ [All][Open]  │ │                    │ │           ││
│ │        │ │ [Closed]     │ │                    │ │           ││
│ └────────┘ └──────────────┘ └────────────────────┘ └───────────┘│
└──────────────────────────────────────────────────────────────────┘
```

**Panels:**

| Panel | Width | Behavior |
|-------|-------|----------|
| **Icon sidebar** (left) | 60px | Collapsible via toggle. 5 items: IPO List, Detail, Subscription, Performance, Toggle Detail panel. Collapsed shows icons only; expanded adds labels. |
| **IPO List** | 300px | Resizable. Dense virtualized `DataTable` showing compact columns: Company, Exchange, Price Band, Open → Close, Status. Filter pills at bottom: All / Upcoming / Open / Closed / Listed. Clicking a row sets the global IPO selection. |
| **Main Content** | flex | Resizable. Content changes based on the active sidebar item. Always shows data for the globally selected IPO (or a "Select an IPO" prompt). |
| **IPO Detail** (right) | 250px | Resizable, toggleable. Always shows the selected IPO's key metrics regardless of which content tab is active. Mirrors Longbreak Pro's stock-detail panel. |

### Screen descriptions

**1. IPO List** (calendar, sidebar index 0)
- Default screen when no IPO is selected.
- IPO list panel shows all IPOs filtered by status.
- Clicking a row selects it globally — detail panel and main content update.
- Filter pills at the bottom of the list panel.

**2. IPO Detail** (sidebar index 1)
- Main content area shows a structured field view: all IPO details in a labeled grid (price band, lot size, issue size, dates, OFS vs fresh split, sector, DRHP/RHP links).
- Detail panel shows compact key-value summary (company, price, status, dates).

**3. Subscription Tracker** (sidebar index 2)
- Main content area: grouped bar chart (QIB/NII/Retail per day over the bidding window) at top, virtualized `DataTable` of raw numbers below.
- Detail panel shows subscription summary (total ×, highest category, latest snapshot).

**4. Listing Performance** (sidebar index 3)
- Main content area: `LineChart` of daily close prices with a dashed horizontal reference line at the issue price (green when above, red when below). `DataTable` of OHLCV data below.
- Detail panel shows performance summary (IPO price, listing price, current price, total return %).

### Component architecture (gpui-component)

| Layout element | gpui-component component |
|---|---|
| Icon sidebar | `Sidebar::left().collapsible(true).collapsed()` |
| Resizable panels | `h_resizable()` with `resizable_panel()` children |
| IPO list table | `Table` with custom `TableDelegate` (virtualized, sortable) |
| Subscription grouped bar chart | Low-level `plot::Bar` with band offset, or `BarChart` with per-category series |
| Performance line chart | `LineChart::new().x().y().stroke().dot()` |
| Issue price reference line | `Shape::Line` composited on the chart |
| Detail/metric panels | Hand-built `div()` + `label()` + `p()` styling |
| Filter pills | `Button::new().pill().active()` in `h_flex()` |
| Dark theme | gpui-component `Theme` with chart colors (`chart_1`–`chart_5`) and status colors (positive green, negative red) |

### State management

```rust
struct MosaicApp {
    // Navigation
    selected_tab: usize,            // 0=List, 1=Detail, 2=Subscription, 3=Performance
    sidebar_collapsed: bool,
    detail_panel_visible: bool,

    // Global selection (calendar is the hub)
    selected_ipo_id: Option<i64>,

    // Filter
    status_filter: Option<IpoStatus>,  // None ≡ All

    // Data (loaded from DB)
    db: MosaicDb,
    markets: HashMap<String, Market>,  // preloaded for currency formatting
    ipos: Vec<Ipo>,
    subscriptions: Vec<SubscriptionEntry>,
    price_history: Vec<PricePoint>,

    // Table states (gpui entities)
    ipo_table: Entity<TableState<IpoDelegate>>,
    sub_table: Entity<TableState<SubDelegate>>,
    price_table: Entity<TableState<PriceDelegate>>,
}
```

### Data flow

```
                    ┌─────────────────────┐
                    │  mosaic (GUI app)    │
                    │                     │
                    │  [Refresh] button   │
                    │  → bg_executor      │
                    │  → scraper writes   │
                    │  → channel signal   │
                    │  → re-query + notify│
                    └────────┬────────────┘
                             │ reads/writes
┌──────────────┐       ┌────▼────┐       ┌─────────────────┐
│  mosaic-cli  │──────►│ SQLite  │◄──────│  mosaic(GUI)    │
│  (headless)  │writes │ DB      │reads   │  (UI thread)    │
└──────────────┘       │ WAL mode│       └─────────────────┘
                       └─────────┘
                      ~/.local/share/mosaic/mosaic.db
```

Most users never touch the CLI. The GUI app has a Refresh button that spawns scrapers in the background (`cx.background_executor().spawn()`). The scraper opens its own `MosaicDb` connection, writes data, then signals the UI thread via a channel. The UI re-queries the DB and calls `cx.notify()`.

`mosaic-cli` exists for debugging, cron jobs, and CI — it calls the exact same `mosaic-core` scraper library synchronously.

### Source-file map (root `mosaic` crate)

| File | Role |
|------|------|
| `src/main.rs` | App entry: `Application::new().run()`, `gpui_component::init()`, window creation |
| `src/bin/cli.rs` | CLI entry: `clap` subcommands → calls `mosaic-core` scrapers synchronously. Produces `mosaic-cli` binary. |
| `src/app.rs` | `MosaicApp` entity — root `Render`, state, event routing, background scraper orchestration |
| `src/sidebar.rs` | Icon sidebar construction (5 items, collapsible) |
| `src/ipo_list.rs` | `IpoDelegate` + filtered `DataTable` + pill filter bar |
| `src/ipo_detail.rs` | Structured field grid for the selected IPO |
| `src/subscription.rs` | Grouped bar chart + subscription `DataTable` |
| `src/performance.rs` | Line chart (with reference line) + price `DataTable` |

---

## 5. Data sources (India)

| Data                                 | Source                                                                     | Notes                                                                                                                                    |
| ------------------------------------ | -------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| IPO calendar, DRHP filings           | Chittorgarh                                                                | Scraped HTML tables; closest thing to a structured archive for Indian IPOs                                                               |
| Subscription status (QIB/NII/Retail) | Chittorgarh (sourced from exchange pages)                                  | Published daily during the bidding window. Scraped from Chittorgarh, not NSE directly — NSE's ToS explicitly prohibits automated collection (see open item below). |
| Post-listing price history           | Yahoo Finance (`.NS`/`.BO` suffix via `yfinance-rs`) _(v1.1+)_ or bhavcopy CSV files | NSE's official historical API is paid/SFTP. Yahoo Finance is the pragmatic free fallback. Bhavcopy files are regulatory daily disclosures published as CSVs — lowest legal risk for EOD data. For v1, price history comes from bhavcopy only. yfinance-rs added in v1.1 for ongoing live tracking. |
| GMP                                  | _(excluded from v1)_                                                       | Unofficial, unsourced — no regulatory filing backs it                                                                                    |

**Open item**: NSE's Terms of Use explicitly prohibit "systematic or automated data collection activities (including scraping, data mining, data extraction and data harvesting)" and are backed by a formal licensing framework through NSE Data. Legitimate Indian platforms (Screener, Tickertape, Moneycontrol) all use licensed feeds (e.g. C-MOTS). Our approach mitigates this by: (a) scraping Chittorgarh (third-party aggregator) rather than NSE directly for IPO-specific data; (b) using Yahoo Finance (via `yfinance-rs`) for price history; (c) falling back to bhavcopy CSV files for EOD data where needed. For any broader redistribution, a licensed feed (C-MOTS, broker APIs, or NSE Data directly) would be required.

---

## 6. Technology stack

### Form factor

Local desktop app. SQLite as the single source of truth — no server, no hosting, no "where does state live" problem.

### Why all-Rust (not split with Python)

Originally considered splitting ingestion (Python) from the app (Rust) to lean on Python's scraping ecosystem. Decided against it: none of the actual scraping work (HTTP calls, HTML table parsing, JSON parsing) needs pandas-level tooling, Rust's scraping crates are fully capable, and a single language avoids an unnecessary seam — also better serves the original goal of learning Rust deeply.

### Project structure

Single Cargo workspace, two crates with two binaries in the root:

```
mosaic/                        ← workspace root + root package
├── Cargo.toml                 ← workspace + root crate with two [[bin]] entries
├── src/
│   ├── main.rs                [bin: mosaic]      — GPUI desktop app
│   ├── bin/cli.rs             [bin: mosaic-cli]  — CLI (debug/automation)
│   ├── settings/
│   │   └── default.toml       ← bundled default config
│   ├── app.rs, sidebar.rs, ...                   — UI modules
├── crates/
│   ├── mosaic-core/                               — shared library
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs          ← re-exports
│   │       ├── types.rs        ← Ipo, SubscriptionEntry, Market, etc.
│   │       ├── db.rs           ← MosaicDb (WAL, CRUD, migrations, KVP store)
│   │       ├── config.rs       ← Config struct (deserialized from TOML)
│   │       ├── scraper.rs      ← IpoScraper trait + static registry
│   │       └── scrapers/
│   │           ├── chittorgarh.rs
│   │           └── test_fixtures/    ← saved HTML snapshots for tests
│   │               ├── chittorgarh-ipos.html
│   │               └── chittorgarh-subscriptions.html
│   └── mosaic-test-fixtures/  ← dev-only crate with test helpers (FakeFs etc.)
```

- **`mosaic-core`** — shared types + SQLite + scrapers. Single source of truth for the data model. Scrapers use `reqwest::blocking` — synchronous API, no tokio dependency. `Market::format_amount(Decimal) -> String` provides currency-aware display formatting.
- **`mosaic`** (root crate) — the GPUI desktop app reads from SQLite via `mosaic-core`, renders the four v1 screens. Also contains `mosaic-cli` binary for headless debug/automation.
- **`mosaic-ingest`** — deleted. Its role is split between `mosaic-cli` (same scraper library) and the GUI's built-in background scraper.

### Dependencies (pinned versions)

| Crate                             | Version  | Crate(s) | Purpose                             | Notes                                                                                                                                                                                                                                                                                                                                                                      |
| --------------------------------- | -------- | -------- | ----------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gpui`                            | 0.2.2    | mosaic        | UI framework                        | Zed's GPU-accelerated UI framework. crates.io version is current with git main. Default features include x11, wayland, font-kit, windows-manifest. Windows support: DirectX 11 backend, official builds since Jan 2025.                                                                                                                              |
| `gpui-component`                  | 0.5.1    | mosaic        | UI components                       | Built by Longbridge for their production trading terminal (Longbridge Pro). Virtualized tables, Line/Bar/Area/Pie charts. Compatible with `gpui ^0.2.2`. All Mosaic features available in this version.                                                                                |
| `rusqlite`                        | 0.40.1   | mosaic-core   | SQLite access                       | `bundled` feature compiles SQLite from source. Chosen over `sqlx` — simpler, synchronous (no network latency in a local single-file DB). WAL mode enabled on open for concurrent read/write.                                                                                                                                                                                |
| `jiff`                            | 0.2.31   | mosaic-core   | Date/time                           | Features: `serde`, `tz-system`. **Not `chrono`** — chrono soft-deprecated by its author as of 2026; jiff is the recommended successor with better timezone handling and faster parsing.                                                                                                                                                                                    |
| `rust_decimal`                    | 1.42.1   | mosaic-core   | Financial arithmetic                | Feature: `serde`. Avoids float precision errors in prices/valuations.                                                                                                                                                                                                                                                                                                       |
| `serde` + `serde_json`            | 1.0.228  | mosaic-core   | Serialization                       | `serde` feature: `derive`. For JSON parsing from yfinance-rs / exchange APIs.                                                                                                                                                                                                                                                                                               |
| `reqwest`                         | 0.13.4   | mosaic-core   | HTTP client                         | Features: `blocking`, `rustls`, `json`. Uses `reqwest::blocking` — synchronous API, no tokio needed in the library. GUI wraps it in `background_executor.spawn()`.                                                                                                                                                                                                          |
| `scraper`                         | 0.27.0   | mosaic-core   | HTML parsing (CSS selectors)        | For Chittorgarh tables etc.                                                                                                                                                                                                                                                                                                                                                 |
| `csv`                             | 1.4.0    | mosaic-core   | CSV parsing                         | For bhavcopy files.                                                                                                                                                                                                                                                                                                                                                         |
| `clap`                            | 4.6.1    | mosaic        | CLI argument parsing                | Feature: `derive`. Used by the `mosaic-cli` binary for subcommands.                                                                                                                                                                                                                                                                                                         |
| `thiserror`                       | 2.0.18   | mosaic-core   | Derive `Error`                      | For `mosaic-core` error types.                                                                                                                                                                                                                                                                                                                                              |
| `anyhow`                          | 1.0.103  | mosaic        | Flexible error type                 | For the `mosaic-cli` binary and GUI error handling (binaries use anyhow, libraries use thiserror).                                                                                                                                                                                                                                                                           |
| `log`                             | 0.4.33   | mosaic-core   | Logging facade                      | GPUI uses `log` internally. Scrapers and DB code log via `log::info!`, `log::warn!`, `log::error!`.                                                                                                                                                                                                          |
| `env_logger`                      | 0.11.11  | mosaic        | Log output                          | Initialized in `main()`. Reads `RUST_LOG` env var for level filtering.                                                                                                                                                                                                                                       |
| `toml`                            | 1.1.2    | mosaic-core   | Config deserialization              | Parses `~/.config/mosaic/config.toml` into `Config` struct. v1.1 includes TOML 1.1 spec support.                                                                                                                                                                                                            |
| `rust-embed`                      | 8.11.0   | mosaic-core   | Embed default config in binary      | Bundles `settings/default.toml` into the compiled binary. Used by the `AssetSource` implementation.                                                                                                                                                                                                         |
| `dirs`                            | 6.0.0    | mosaic-core   | XDG directory paths                 | Resolves `~/.local/share/mosaic/` (data), `~/.config/mosaic/` (config), `~/.cache/mosaic/` (cache).                                                                                                                                                                                                        |
| `yfinance-rs` _(v1.1+)_           | —        | mosaic-core   | Post-listing price history fallback | Async Rust client mirroring Python's `yfinance`; supports `.NS`/`.BO` tickers. Only useful once a company has a listed ticker. Deferred to v1.1.                                                                                                                                                                                                                             |

### Things to verify before/early in implementation

- NSE/BSE data terms of use for the scraping approach (see open item in §5 — current plan uses Chittorgarh + Yahoo Finance to avoid direct NSE scraping)
- Verify `gpui` v0.2.2 + `gpui-component` v0.5.1 compile together on target platform (crates.io versions confirmed compatible via semver)

### Infrastructure

#### Database migrations

Schema changes are inevitable as features are added. Migrations use a simple ordered-array pattern inspired by Zed's `Domain::MIGRATIONS`:

```rust
impl MosaicDb {
    const MIGRATIONS: &[&str] = &[
        // Migration 1: initial schema
        "CREATE TABLE IF NOT EXISTS markets ( ... ); \
         CREATE TABLE IF NOT EXISTS ipos ( ... ); \
         ...",
        // Migration 2: future column addition
        "ALTER TABLE ipos ADD COLUMN foo TEXT;",
    ];
}
```

On `MosaicDb::new()`:
1. Read current version from `PRAGMA user_version` (defaults to 0)
2. Apply `MIGRATIONS[current..]` in order inside a transaction
3. Set `PRAGMA user_version = MIGRATIONS.len()` after success

This keeps migrations in code (no external files), linear (no branching), and testable (in-memory SQLite starts at version 0 every time).

#### Configuration & preferences

Two tiers of state:

| Tier | What | Where | Format |
|---|---|---|---|
| **User preferences** | Theme, refresh interval, default market | `~/.config/mosaic/config.toml` | TOML, `serde::Deserialize` |
| **App runtime state** | Window bounds, panel sizes, sidebar collapsed | SQLite `key_value_store` table | JSON values keyed by string |

**Config loading** (`mosaic-core/src/config.rs`):

```rust
#[derive(Debug, Deserialize)]
pub struct Config {
    pub theme: Option<String>,
    pub refresh_interval_secs: Option<u64>,
    pub default_market: Option<String>,
}

impl Config {
    /// Load from XDG config dir, falling back to defaults
    pub fn load() -> Self { ... }
    /// Write back to file
    pub fn save(&self) -> Result<()> { ... }
}
```

At startup `main.rs` calls `Config::load()`, wraps it in a GPUI global, and any view reads via `App::global::<Config>()`. Writing preferences calls `config.save()` then `cx.notify()` on all observers.

**Window state** is stored in the DB via a simple KVP table (Zed's `KeyValueStore` pattern):

```sql
CREATE TABLE IF NOT EXISTS key_value_store (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

Written on window close, read on app start. This avoids parsing a config file just to place a window.

#### Error handling strategy

```rust
// mosaic-core: thiserror for domain errors
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Migration error: {0}")]
    Migration(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ScrapeError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("HTML parse error: {0}")]
    Parse(String),
    #[error("Rate limited, retry after {0:?}")]
    RateLimited(Duration),
}

// Application binary: anyhow::Result for convenience
// use anyhow::{Context, Result};
```

Fire-and-forget pattern (consistent with GPUI's `log_err`):

```rust
fn do_something(&mut self, cx: &mut Context<Self>) {
    cx.spawn(async move |this, cx| {
        let result = cx.background_spawn(async { /* ... */ }).await;
        this.update(cx, |this, cx| {
            match result {
                Ok(data) => this.data = data,
                Err(e) => log::error!("background task failed: {e}"),
            }
            cx.notify();
        }).ok();
    }).detach();
}
```

Error display in the UI follows a simple rule:
- **Recoverable errors** (HTTP timeout, parse failure) → shown in the sync status indicator, no user action needed
- **Fatal errors** (DB corruption at startup) → modal dialog on first render, app exits on dismiss

#### Logging

GPUI uses the `log` crate internally. Mosaic follows the same convention:

- `log::info!("Scraped {n} IPOs from Chittorgarh")` — normal operations
- `log::warn!("Rate limited, backing off {backoff:?}")` — recoverable issues
- `log::error!("DB migration failed: {e}")` — unrecoverable, shown to user

Initialization in `main.rs`:

```rust
fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();
    // ... GPUI app setup
}
```

Users control verbosity via `RUST_LOG=mosaic=debug` or `RUST_LOG=info`. No file logging in v1 — logs go to stderr, visible when running from terminal.

#### Assets

GPUI provides the `AssetSource` trait for loading bundled files:

```rust
pub trait AssetSource: 'static + Send + Sync {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>>;
    fn list(&self, path: &str) -> Result<Vec<SharedString>>;
}
```

For Mosaic, assets are:
- **Default config**: `settings/default.toml` (embedded via `rust-embed`)
- **Icons/SVGs**: Optional in v1; if needed, bundled via embed or loaded from `~/.local/share/mosaic/assets/`
- **Fonts**: GPUI's built-in font system handles this — no custom font bundling needed

Implementation: a simple `struct EmbeddedAssets;` that wraps `rust_embed::RustEmbed` and implements `AssetSource`. Set at startup via `Application::with_assets(EmbeddedAssets)`.

#### Testing strategy

Three tiers:

**Unit tests** (`mosaic-core`):
- `MosaicDb` with in-memory `:memory:` SQLite — fast, isolated, no I/O
- Scraper tests against saved HTML fixtures in `crates/mosaic-core/src/scrapers/test_fixtures/`

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_parse_ipo_list() {
        let html = include_str!("scrapers/test_fixtures/chittorgarh-ipos.html");
        let ipos = parse_ipo_list(html).unwrap();
        assert_eq!(ipos.len(), 15);
        assert_eq!(ipos[0].name, "Hexaware Technologies");
    }
}
```

**Integration tests** (`mosaic` crate via `#[gpui::test]`):
- GPUI's `TestAppContext` with deterministic scheduling
- `FakeFs` for filesystem fixtures (test configs, DB paths)
- `cx.executor().advance_clock(duration)` + `cx.run_until_parked()` for time-sensitive tests
- Windowless assertion: create views, call model methods, check state without rendering

```rust
#[gpui::test]
async fn test_sync_status(cx: &mut TestAppContext) {
    let app = cx.open_window(|cx| MosaicApp::new(cx));
    // simulate background sync completion
    app.update(cx, |app, cx| {
        app.on_sync_complete(Ok(()), cx);
        assert!(app.last_sync_err.is_none());
    });
}
```

**Scraper integration** (manual / CI):
- Real HTTP tests excluded from `cargo test` by default. Run via `cargo test -- --ignored` or a separate test binary with feature flag.
- These test the full pipeline: HTTP → parse → write → read back
- Run before release to catch source site changes

#### Async runtime model

No tokio in the application. GPUI's built-in executor handles everything:

```
GUI thread (ForegroundExecutor)
  └── cx.spawn(async move |this, cx| { ... })
        └── waits on BackgroundExecutor tasks
              └── cx.background_spawn(async { ... })
                    └── reqwest::blocking calls (run on OS thread pool)
                    └── SQLite writes (through writer connection)
```

Key rules:
- `mosaic-core` stays **purely synchronous** — no async runtime dependency
- The GUI crate uses `cx.background_spawn()` for blocking work (HTTP, DB writes)
- `cx.spawn()` (foreground) receives a `WeakEntity<T>` + `&mut AsyncApp` for post-work model updates
- Storing a `Task` on the entity provides automatic cancellation on entity drop
- No tokio, no `futures` crate, no `async_io` — GPUI's executor is sufficient

The background sync task (§7) is the canonical example of this pattern: `cx.spawn(loop { timer().await; background_spawn().await; update_model().await })`.

### Expansion architecture

The schema and crate structure are designed so that adding a market or expanding beyond IPOs doesn't require a rewrite.

**Scraper trait** (in `mosaic-core`):

```rust
trait IpoScraper {
    fn market_id(&self) -> &str;
    fn fetch_ipos(&self) -> Result<Vec<Ipo>>;
    fn fetch_subscriptions(&self, ipo: &Ipo) -> Result<Vec<SubscriptionEntry>>;
    fn fetch_price_history(&self, ticker: &str, market: &str) -> Result<Vec<PricePoint>>;
}
```

The trait is synchronous (`reqwest::blocking`). No tokio needed in the library. Scrapers are registered in a static `Vec<Box<dyn IpoScraper>>` in `mosaic-core`. Both the GUI app and `mosaic-cli` call the same library — the GUI wraps it in `background_executor.spawn()`, the CLI calls it directly.

**Expansion path:**

| Layer | Now (v1) | New market (US/HK) | Beyond IPOs |
|---|---|---|---|
| Schema | `markets`, `ipos` with `market_id`, normalized `subscription_snapshots`, `exchange_rates` (empty stub) | Add market-specific categories to `markets` table | Add `securities` parent table; `ipos` references it |
| Core types | `Ipo`, `SubscriptionEntry`, `Market`, `PricePoint` with `Market::format_amount()` | Categories extend naturally | Add `Security` type, `Ipo` inherits |
| Ingest | `ChittorgarhScraper` in `mosaic-core` library. Called from GUI bg task or `mosaic-cli`. Synchronous API. | New scraper struct implementing same trait | May add non-IPO scraper trait |
| UI | 4 sidebar items, India-focused. Background sync with status indicator. Click to force-refresh. | Market filter in IPO list | Generalized nav items |
| Comparisons | Query methods on `MosaicDb` (sector, size) | Cross-market needs exchange rate table | Works on `security_id` instead of `ipo_id` |

Comparisons live in `mosaic-core` as SQL query methods on `MosaicDb` — no separate crate until complexity warrants one.

---

## 7. Background sync

Users should never need to press a Refresh button. Data syncs automatically in the background while the app runs.

### Architecture

```
┌──────────────────────────────────────────────────────────┐
│  MosaicApp (Entity)                                      │
│  ┌─────────────────┐   ┌─────────────────────────────┐  │
│  │ UI State         │   │ _sync_task: Option<Task<()>> │  │
│  │ (ipos, selected, │   │   loop {                    │  │
│  │  is_syncing,     │   │     timer(backoff).await    │  │
│  │  last_sync_at,   │   │     background_spawn(       │  │
│  │  last_sync_err)  │   │       scraper.run(db_writer) │  │
│  │                  │   │     ).await                  │  │
│  │                  │   │     refresh_from_db(cx)      │  │
│  │                  │   │     cx.notify()              │  │
│  │                  │   │   }                          │  │
│  └─────────────────┘   └─────────────────────────────┘  │
│           │                                               │
│           ▼                                               │
│  ┌─────────────────┐                                      │
│  │ MosaicDb         │  2 connection modes:                │
│  │  - open_reader() │  query_only for UI thread           │
│  │  - open_writer() │  Mutex<Connection> for background   │
│  └─────────────────┘                                      │
│           │                                               │
│           ▼                                               │
│  ┌─────────────────┐                                      │
│  │ SQLite (WAL)     │  ~/.local/share/mosaic/mosaic.db    │
│  └─────────────────┘                                      │
└──────────────────────────────────────────────────────────┘
```

### Sync loop behavior

| Event | Behavior |
|---|---|
| App start | Immediate sync (no initial delay) |
| Successful sync | Reset backoff to 5 min interval |
| HTTP error (transient) | Backoff: 30s → 1m → 2m → 4m → ... → 1hr max |
| Network unreachable | Same exponential backoff; log via `tracing::warn!` |
| App close | `Task` stored on entity is dropped → auto-cancels at next `.await` |
| Manual "Refresh now" | Set backoff to zero → fires immediately on next loop iteration |

### SQLite connection model

Two separate connections, both with `PRAGMA journal_mode=WAL`:

- **Reader** (`MosaicDb::open_reader`): used by the UI thread only. Set `PRAGMA query_only=1` to prevent accidental writes. No lock contention with the writer in WAL mode.
- **Writer** (``MosaicDb::open_writer``): used by the background sync task behind a `Mutex<Connection>`. Set `PRAGMA busy_timeout=5000` (retry for 5 seconds before giving up) and `PRAGMA synchronous=NORMAL` (safe with WAL, faster than FULL).

### Sync status UI indicator

A compact status element at the bottom of the sidebar (or as a thin status bar) showing:

| State | Display | Implementation |
|---|---|---|
| Idle, synced recently | ✓ Synced 2m ago | `last_sync_at.map(relative_time)` |
| Syncing now | ⟳ Syncing... | Render while `is_syncing == true` |
| Sync failed (retrying) | ⚠ Retry in 30s... | Show `last_sync_err` + remaining backoff |

The indicator is clickable: clicking it triggers an immediate manual refresh.

Implementation: three fields on `MosaicApp` (`is_syncing`, `last_sync_result`, `last_sync_at`), rendered in a simple `div` at the bottom of the sidebar panel. A tick timer re-renders the relative-time display every 60 seconds when idle, or every second while syncing.

### Rate limiting

To avoid hammering data sources:

```rust
struct RateLimiter {
    min_interval: Duration,
    last_request: Option<Instant>,
}
impl RateLimiter {
    async fn wait(&mut self, cx: &BackgroundExecutor) {
        if let Some(last) = self.last_request {
            let elapsed = last.elapsed();
            if elapsed < self.min_interval {
                cx.timer(self.min_interval - elapsed).await;
            }
        }
        self.last_request = Some(Instant::now());
    }
}
```

Recommended intervals:
- **Chittorgarh**: 2s between requests (third-party aggregator, not a CDN-backed API)
- **Bhavcopy CSV**: 1s per file (static files on a regulatory server)
- **Yahoo Finance** (v1.1+): 1s minimum

One `RateLimiter` per data source, passed to the scraper.

### GPUI pattern (code template)

```rust
fn start_background_sync(&mut self, cx: &mut Context<Self>) {
    let weak = cx.weak_entity();
    let db_path = self.db.path().to_owned();
    let normal_interval = Duration::from_secs(300);
    let max_backoff = Duration::from_secs(3600);

    let task = cx.spawn(async move |cx| {
        let mut backoff = Duration::ZERO; // immediate first run
        loop {
            cx.background_executor().timer(backoff).await;

            weak.update(cx, |this, _| { this.is_syncing = true; }).ok();

            let result = cx.background_spawn({
                let db_path = db_path.clone();
                async move {
                    let db = MosaicDb::open_writer(&db_path)?;
                    let scraper = ChittorgarhScraper::new();
                    let ipos = scraper.fetch_ipos()?;
                    db.upsert_ipos(&ipos)?;
                    Ok::<_, anyhow::Error>(())
                }
            }).await;

            weak.update(cx, |this, cx| {
                this.is_syncing = false;
                this.last_sync_at = Some(Instant::now());
                match result {
                    Ok(()) => {
                        backoff = normal_interval;
                        this.last_sync_err = None;
                    }
                    Err(e) => {
                        log::warn!("Sync failed: {e}");
                        backoff = (backoff * 2).min(max_backoff);
                        this.last_sync_err = Some(e.to_string());
                    }
                }
                this.ipos = this.db.list_ipos(None);
                cx.notify();
            }).ok();
        }
    });
    self._sync_task = Some(task);
}
```

### Files affected

| File | Change |
|---|---|
| `crates/mosaic-core/src/db.rs` | Add `open_reader()` / `open_writer()`, add `path()` accessor |
| `src/app.rs` | Add `_sync_task`, `is_syncing`, `last_sync_at`, `last_sync_err`, `start_background_sync()`, `refresh_from_db()` |
| `src/sidebar.rs` or `src/sync_status.rs` | Sync status indicator component |

### No additional runtime dependencies

- GPUI's `BackgroundExecutor` does all the async work — no tokio needed.
- `mosaic-core` stays sync-only (no async runtime dependency in the library crate).
- No new crate dependencies beyond what's already in the stack.

---

## 8. Roadmap

**v1** — IPO calendar, subscription tracking, listing performance. India only. (this document)

**v2 candidates** (not sequenced yet):

- Comps engine: rules-based similarity scoring (sector, revenue range, growth bucket, offer type, market cap tier) with transparent weights — not an LLM judgment call
- Ownership/shareholder tracking: promoters, anchor investors, post-IPO shareholding pattern from RHP disclosures
- News/sentiment: GDELT for historical tone/volume timelines, ticker-tagged financial news APIs for sentiment with a versioned, published methodology
- Market conditions composite: recent IPO index performance, India VIX, trailing-quarter withdrawal rate
- US market expansion: SEC EDGAR (S-1 filings, Form 3/4/13D-G for ownership), reusing the same point-in-time architecture
- Generalize beyond IPOs to ongoing company tracking (schema already supports this with minor extension)
