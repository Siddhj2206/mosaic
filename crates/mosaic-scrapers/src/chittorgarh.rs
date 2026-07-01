use mosaic_core::scraper::{IpoScraper, ScrapeError};
use mosaic_core::types::{Ipo, PricePoint, SubscriptionEntry};

pub struct ChittorgarhScraper;

impl ChittorgarhScraper {
    pub fn new() -> Self {
        Self
    }
}

impl IpoScraper for ChittorgarhScraper {
    fn market_id(&self) -> &str {
        "in"
    }

    fn fetch_ipos(&self) -> Result<Vec<Ipo>, ScrapeError> {
        Err(ScrapeError::Parse("not yet implemented".into()))
    }

    fn fetch_subscriptions(&self, _ipo: &Ipo) -> Result<Vec<SubscriptionEntry>, ScrapeError> {
        Err(ScrapeError::Parse("not yet implemented".into()))
    }

    fn fetch_price_history(&self, _ticker: &str, _market: &str) -> Result<Vec<PricePoint>, ScrapeError> {
        Err(ScrapeError::Parse("not yet implemented".into()))
    }
}
