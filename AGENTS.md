# Mosaic — Agent Guidelines

## Project overview
Mosaic is a deterministic IPO tracker built in Rust. Local desktop app with SQLite. No AI slop — all data traceable to sourced records with `ingested_at` timestamps. v1 targets India only (NSE/BSE mainboard IPOs).

## Workspace structure
- `mosaic-core/` — shared types, DB, config, scraper trait (no HTTP deps)
- `mosaic-scrapers/` — scraper implementations (reqwest::blocking, scraper, csv)
- Root crate `mosaic/` — two `[[bin]]` entries: `mosaic` (GPUI GUI) + `mosaic-cli` (clap CLI)

## Constraints & conventions

### Cargo workflow
- **Never edit `Cargo.toml` directly** — use `cargo add --package <name> <dep>` (with `--features`/`--no-default-features` as needed). For path deps: `cargo add --package <name> <dep> --path <path>`.
- Always use latest available versions. `cargo add` resolves this automatically.

### Code conventions
- Synchronous `mosaic-core` + `mosaic-scrapers` — no tokio in library crates
- GPUI's `BackgroundExecutor` handles blocking work in the GUI
- Two-connection SQLite: reader (`PRAGMA query_only`) for UI, writer (`Mutex<Connection>`, `busy_timeout=5000`) for background
- Logging via `log` crate + `env_logger` (matching GPUI's internal choice), not `tracing`
- `thiserror` for library error types, `anyhow` for binary error handling
- DB migrations via static `MIGRATIONS: &[&str]` array + `PRAGMA user_version`
- Config: `~/.config/mosaic/config.toml` (TOML, serde) + SQLite `key_value_store` (window state)
- Decimal stored as REAL (f64) in SQLite; convert with `decimal_to_f64_opt()` / `f64_to_decimal_opt()` helpers (orphan rule prevents `FromSql`/`ToSql` for `rust_decimal::Decimal`)
- `IpoStatus` implements `FromStr` + `as_str()` for TEXT storage in SQLite

### GPUI patterns
- `Render::render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement`
- `Application::new().run(|cx: &mut App| { cx.open_window(...); })`
- `cx.spawn(async move |cx: &mut AsyncApp| { ... }).detach()` for foreground async
- `cx.background_spawn(async move { ... })` for blocking work
- Entity model: `struct View; impl Render for View { ... }`; create via `cx.new(|cx| View::new(cx))`
- Fire-and-forget pattern: `cx.spawn(...).detach()` with `log::error!` on failure in the update callback

### Testing
- DB tests use temp files + atomic counter (not `:memory:`) to avoid parallel-test conflicts
- Scraper tests parse saved HTML fixtures from `mosaic-scrapers/src/test_fixtures/` via `include_str!`
- Real HTTP tests excluded from `cargo test` — run via `cargo test -- --ignored`

## Skills to load
| Skill | When |
|-------|------|
| `gpui` | Writing views, async tasks, context management, layout, styling |
| `gpui-component` | Using Button, Table, Sidebar, Input, Select, Dialog, chart components |
| `frontend-design` | Visual design direction, typography, layout choices |
| `cli-guidelines` | Reviewing CLI flag/arg design, help text, exit codes |

## Research reference
When unsure about infra/code design patterns, spawn a subagent to explore the Zed codebase at `/var/home/sid/Documents/Projects/mosaic/../scratch/zed/` (or the actual path) for reference patterns in:
- DB migrations (`Domain::MIGRATIONS` pattern)
- Setting management (GPUI globals for settings)
- Asset loading (`AssetSource` trait)
- Testing (`FakeFs`, `TestAppContext`, `#[gpui::test]`)
- Workspace configuration (profiles, `[workspace.dependencies]`)
