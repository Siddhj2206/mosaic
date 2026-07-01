use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

pub type DateTime = String;
pub type Date = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IpoStatus {
    Upcoming,
    Open,
    Closed,
    Listed,
    Withdrawn,
}

impl IpoStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            IpoStatus::Upcoming => "upcoming",
            IpoStatus::Open => "open",
            IpoStatus::Closed => "closed",
            IpoStatus::Listed => "listed",
            IpoStatus::Withdrawn => "withdrawn",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "upcoming" => Some(IpoStatus::Upcoming),
            "open" => Some(IpoStatus::Open),
            "closed" => Some(IpoStatus::Closed),
            "listed" => Some(IpoStatus::Listed),
            "withdrawn" => Some(IpoStatus::Withdrawn),
            _ => None,
        }
    }
}

impl FromStr for IpoStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        IpoStatus::from_str(s).ok_or_else(|| format!("invalid IPO status: {s}"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    pub id: String,
    pub name: String,
    pub currency: String,
    pub currency_symbol: String,
}

impl Market {
    pub fn format_amount(&self, amount: Decimal) -> String {
        format!("{}{}", self.currency_symbol, amount)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ipo {
    pub id: Option<i64>,
    pub market_id: String,
    pub company_name: String,
    pub symbol: Option<String>,
    pub exchange: Option<String>,
    pub sector: Option<String>,
    pub offer_type: Option<String>,
    pub price_band_low: Option<Decimal>,
    pub price_band_high: Option<Decimal>,
    pub final_price: Option<Decimal>,
    pub lot_size: Option<i64>,
    pub shares_offered: Option<i64>,
    pub fresh_issue_shares: Option<i64>,
    pub ofs_shares: Option<i64>,
    pub shares_outstanding_post: Option<i64>,
    pub issue_size: Option<Decimal>,
    pub open_date: Option<Date>,
    pub close_date: Option<Date>,
    pub allotment_date: Option<Date>,
    pub listing_date: Option<Date>,
    pub status: IpoStatus,
    pub drhp_url: Option<String>,
    pub rhp_url: Option<String>,
    pub source: String,
    pub ingested_at: DateTime,
    pub updated_at: DateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionEntry {
    pub id: Option<i64>,
    pub ipo_id: i64,
    pub snapshot_at: DateTime,
    pub category: String,
    pub subscribed: Option<Decimal>,
    pub source: String,
    pub ingested_at: DateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricePoint {
    pub id: Option<i64>,
    pub ipo_id: i64,
    pub trade_date: Date,
    pub open_price: Option<Decimal>,
    pub high_price: Option<Decimal>,
    pub low_price: Option<Decimal>,
    pub close_price: Option<Decimal>,
    pub volume: Option<i64>,
    pub source: String,
    pub ingested_at: DateTime,
}
