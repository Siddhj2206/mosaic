//! The scraper trait — the seam between data sources and the app.
//!
//! Implementations live in `mosaic-scrapers`. The app calls through this
//! trait from a background task; the returned records carry no `ipo_id`
//! (callers assign it after DB lookup).

use jiff::civil::Date;

use crate::error::Result;
use crate::types::{Ipo, PricePoint, SubscriptionSnapshot};

pub trait IpoScraper {
    /// Stable source id written to every record: "nse", "chittorgarh", ...
    fn source(&self) -> &'static str;

    /// Fetch IPO calendar + detail records (upcoming/open for NSE; the 2026
    /// archive for Chittorgarh/IPO Watch). Status should be derived for
    /// `today` before returning.
    fn fetch_ipos(&mut self, today: Date) -> Result<Vec<Ipo>>;

    /// Fetch today's subscription snapshot rows for one IPO. `snapshot_at`
    /// must be set to the poll date by the caller.
    fn fetch_subscriptions(&mut self, ipo: &Ipo) -> Result<Vec<SubscriptionSnapshot>>;

    /// Fetch EOD price history for one IPO (from listing day onward).
    fn fetch_price_history(&mut self, ipo: &Ipo) -> Result<Vec<PricePoint>>;
}
