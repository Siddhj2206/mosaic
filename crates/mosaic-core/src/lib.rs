//! mosaic-core: shared types, SQLite access, config, scraper trait, and the
//! rate limiter. No HTTP or HTML dependencies — deterministic data layer only.

pub mod config;
pub mod db;
pub mod error;
pub mod rate_limit;
pub mod scraper;
pub mod types;

pub use config::Config;
pub use db::Db;
pub use error::{Error, Result};
pub use rate_limit::RateLimiter;
pub use scraper::IpoScraper;
pub use types::{
    IngestionRun, Ipo, IpoStatus, PricePoint, RunStatus, SubCategory, SubscriptionSnapshot,
    decimal_to_f64_opt, f64_to_decimal_opt, normalize_company_name,
};
