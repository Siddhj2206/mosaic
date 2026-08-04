//! mosaic-scrapers: concrete `IpoScraper` implementations.
//!
//! - `nse` — NSE official JSON API (primary; ADR-0001)
//! - `chittorgarh` — dashboard + detail pages (server-rendered HTML)
//! - `ipowatch` — recent/upcoming mainboard archive (server-rendered HTML)

pub mod chittorgarh;
pub mod ipowatch;
pub mod nse;
pub mod parse_util;

pub use chittorgarh::{ChittorgarhDetail, ChittorgarhScraper, DashboardEntry};
pub use ipowatch::IpoWatchScraper;
pub use nse::{NseClient, NseScraper};
