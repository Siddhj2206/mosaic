use crate::types::{Ipo, PricePoint, SubscriptionEntry};

#[derive(Debug, thiserror::Error)]
pub enum ScrapeError {
    #[error("HTTP request failed: {0}")]
    Http(String),
    #[error("HTML parse error: {0}")]
    Parse(String),
    #[error("Rate limited, retry after {0:?}")]
    RateLimited(std::time::Duration),
}

pub trait IpoScraper {
    fn market_id(&self) -> &str;
    fn fetch_ipos(&self) -> Result<Vec<Ipo>, ScrapeError>;
    fn fetch_subscriptions(&self, ipo: &Ipo) -> Result<Vec<SubscriptionEntry>, ScrapeError>;
    fn fetch_price_history(&self, ticker: &str, market: &str) -> Result<Vec<PricePoint>, ScrapeError>;
}
