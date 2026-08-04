//! Shared domain types for Mosaic.
//!
//! Money is `rust_decimal::Decimal` in memory, stored as REAL (f64) in SQLite
//! (orphan rule prevents `FromSql`/`ToSql` impls). Use the `decimal_to_f64_opt`
//! / `f64_to_decimal_opt` helpers at the DB boundary.
//!
//! Dates are `jiff::civil::Date` / `DateTime`, stored as ISO-8601 TEXT.

use jiff::civil::{Date, DateTime};
use rust_decimal::Decimal;
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Status lifecycle
// ---------------------------------------------------------------------------

/// The live lifecycle of an IPO: `upcoming` → `open` → `closed` → `listed`,
/// with `withdrawn` possible at any point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpoStatus {
    Upcoming,
    Open,
    Closed,
    Listed,
    Withdrawn,
}

impl IpoStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            IpoStatus::Upcoming => "upcoming",
            IpoStatus::Open => "open",
            IpoStatus::Closed => "closed",
            IpoStatus::Listed => "listed",
            IpoStatus::Withdrawn => "withdrawn",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
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

impl std::fmt::Display for IpoStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for IpoStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        IpoStatus::parse(s).ok_or_else(|| format!("unknown IPO status: {s}"))
    }
}

// ---------------------------------------------------------------------------
// Subscription categories
// ---------------------------------------------------------------------------

/// The subscription categories NSE publishes: QIB, NII, Retail, and the
/// Total row. The sNII/bNII split is Chittorgarh-only and not modelled in v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SubCategory {
    Qib,
    Nii,
    Retail,
    Total,
}

impl SubCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            SubCategory::Qib => "qib",
            SubCategory::Nii => "nii",
            SubCategory::Retail => "retail",
            SubCategory::Total => "total",
        }
    }

    /// Case-insensitive parse of the names NSE/Chittorgarh use.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        match s.as_str() {
            "qib" | "qib (ex anchor)" | "qualified institutional buyers" => Some(SubCategory::Qib),
            "nii" | "nib" | "non institutional investors" | "non-institutional investors" => {
                Some(SubCategory::Nii)
            }
            "retail" | "rii" | "retail individual investors" => Some(SubCategory::Retail),
            "total" | "overall" => Some(SubCategory::Total),
            _ => None,
        }
    }
}

impl std::str::FromStr for SubCategory {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        SubCategory::parse(s).ok_or_else(|| format!("unknown subscription category: {s}"))
    }
}

impl std::fmt::Display for SubCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Domain records
// ---------------------------------------------------------------------------

/// One IPO — the mutable current-state row. `status` is the live lifecycle
/// state; subscription and price data live in append-only tables.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ipo {
    pub id: Option<i64>,
    pub company_name: String,
    /// Lower-cased, suffix-stripped name used as the cross-source identity key.
    pub normalized_name: String,
    /// Ticker; NULL until allotted.
    pub symbol: Option<String>,
    /// Free text: "NSE" or "BSE, NSE" etc.
    pub exchange: Option<String>,
    pub sector: Option<String>,
    pub status: IpoStatus,
    pub price_band_low: Option<Decimal>,
    pub price_band_high: Option<Decimal>,
    pub final_price: Option<Decimal>,
    pub face_value: Option<Decimal>,
    /// Minimum shares per application.
    pub lot_size: Option<i64>,
    /// Multiples of the lot per application step.
    pub lot_multiples: Option<i64>,
    /// Total issue size in ₹ crore (issue_size_cr). Prefer explicit amounts.
    pub issue_size_cr: Option<Decimal>,
    /// Shares offered in total (from NSE calendar `issueSize`).
    pub shares_offered: Option<i64>,
    /// Fresh-issue shares (Chittorgarh).
    pub fresh_issue_shares: Option<i64>,
    /// Offer-for-sale shares (Chittorgarh).
    pub ofs_shares: Option<i64>,
    /// "Bookbuilding" | "Fixed Price" (NSE: "100% Book Building").
    pub issue_type: Option<String>,
    /// "Fresh capital cum OFS" | "Fresh Issue" | "Offer for Sale" (Chittorgarh).
    pub offer_type: Option<String>,
    pub open_date: Option<Date>,
    pub close_date: Option<Date>,
    pub allotment_date: Option<Date>,
    pub listing_date: Option<Date>,
    /// Chittorgarh marks tentative listing dates.
    pub listing_date_tentative: bool,
    pub drhp_url: Option<String>,
    pub rhp_url: Option<String>,
    /// Chittorgarh detail-page URL (for enrichment re-runs and the dossier).
    pub detail_url: Option<String>,
    /// Which scraper wrote this row: "nse" | "chittorgarh" | "ipowatch".
    pub source: String,
    pub ingested_at: DateTime,
    pub updated_at: DateTime,
}

impl Ipo {
    pub fn new(company_name: impl Into<String>, source: impl Into<String>) -> Self {
        let company_name = company_name.into();
        let now = now_utc();
        let normalized_name = normalize_company_name(&company_name);
        Ipo {
            id: None,
            company_name,
            normalized_name,
            symbol: None,
            exchange: None,
            sector: None,
            status: IpoStatus::Upcoming,
            price_band_low: None,
            price_band_high: None,
            final_price: None,
            face_value: None,
            lot_size: None,
            lot_multiples: None,
            issue_size_cr: None,
            shares_offered: None,
            fresh_issue_shares: None,
            ofs_shares: None,
            issue_type: None,
            offer_type: None,
            open_date: None,
            close_date: None,
            allotment_date: None,
            listing_date: None,
            listing_date_tentative: false,
            drhp_url: None,
            rhp_url: None,
            detail_url: None,
            source: source.into(),
            ingested_at: now,
            updated_at: now,
        }
    }

    /// Derive lifecycle status from open/close/listing dates against `today`.
    pub fn derive_status(&mut self, today: Date) {
        self.status = match (self.open_date, self.close_date, self.listing_date) {
            (Some(open), Some(_), _) if today < open => IpoStatus::Upcoming,
            (_, _, Some(listing)) if today >= listing => IpoStatus::Listed,
            (Some(open), Some(close), None) if open <= today && today <= close => IpoStatus::Open,
            (Some(_), Some(close), _) if today > close => IpoStatus::Closed,
            (None, None, None) => IpoStatus::Upcoming,
            // Closed window with a future listing date.
            _ => IpoStatus::Closed,
        };
    }
}

/// One append-only subscription snapshot row: (ipo, snapshot day, category).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubscriptionSnapshot {
    pub id: Option<i64>,
    pub ipo_id: Option<i64>,
    /// The day the poll reflects (usually the poll date).
    pub snapshot_at: Date,
    pub category: SubCategory,
    pub offered_shares: Option<i64>,
    pub bid_shares: Option<i64>,
    /// Times subscribed — the primary demand signal.
    pub times_subscribed: Option<Decimal>,
    pub source: String,
    pub ingested_at: DateTime,
}

impl SubscriptionSnapshot {
    pub fn new(
        ipo_id: i64,
        snapshot_at: Date,
        category: SubCategory,
        source: impl Into<String>,
    ) -> Self {
        SubscriptionSnapshot {
            id: None,
            ipo_id: Some(ipo_id),
            snapshot_at,
            category,
            offered_shares: None,
            bid_shares: None,
            times_subscribed: None,
            source: source.into(),
            ingested_at: now_utc(),
        }
    }
}

/// One append-only EOD price row: (ipo, trade date).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PricePoint {
    pub id: Option<i64>,
    pub ipo_id: Option<i64>,
    pub trade_date: Date,
    pub open_price: Option<Decimal>,
    pub high_price: Option<Decimal>,
    pub low_price: Option<Decimal>,
    pub close_price: Option<Decimal>,
    pub volume: Option<i64>,
    pub vwap: Option<Decimal>,
    pub source: String,
    pub ingested_at: DateTime,
}

impl PricePoint {
    pub fn new(ipo_id: i64, trade_date: Date, source: impl Into<String>) -> Self {
        PricePoint {
            id: None,
            ipo_id: Some(ipo_id),
            trade_date,
            open_price: None,
            high_price: None,
            low_price: None,
            close_price: None,
            volume: None,
            vwap: None,
            source: source.into(),
            ingested_at: now_utc(),
        }
    }
}

/// Outcome of a sync run for one source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunStatus {
    Success,
    Partial,
    Failed,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Success => "success",
            RunStatus::Partial => "partial",
            RunStatus::Failed => "failed",
        }
    }
}

/// One ingestion run — the audit row for a sync pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngestionRun {
    pub id: Option<i64>,
    pub source: String,
    pub started_at: DateTime,
    pub finished_at: Option<DateTime>,
    pub status: Option<RunStatus>,
    pub records_written: i64,
    pub notes: Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers
/// Current UTC civil datetime — provenance timestamps are UTC.
pub fn now_utc() -> DateTime {
    jiff::Timestamp::now().to_zoned(jiff::tz::TimeZone::system()).datetime()
}

// ---------------------------------------------------------------------------

/// `decimal_to_f64_opt(Some(d))` — SQLite REAL conversion at the DB boundary.
pub fn decimal_to_f64_opt(d: Option<Decimal>) -> Option<f64> {
    d.and_then(|d| d.to_f64())
}

/// `f64_to_decimal_opt(Some(f))` — SQLite REAL conversion at the DB boundary.
pub fn f64_to_decimal_opt(f: Option<f64>) -> Option<Decimal> {
    f.and_then(|f| Decimal::from_f64(f))
}

/// Strip legal-entity suffixes and lowercase, for cross-source identity.
pub fn normalize_company_name(name: &str) -> String {
    let mut n = name.to_ascii_lowercase();
    for suffix in [
        " limited",
        " ltd",
        " private limited",
        " pvt ltd",
        " pvt. ltd.",
        " private ltd",
        " incorporated",
        " inc",
        " ltd.",
        " limited.",
        " technologies limited",
        " technologies ltd",
    ] {
        if n.ends_with(suffix) {
            let trimmed = n[..n.len() - suffix.len()].trim_end();
            if !trimmed.is_empty() {
                n = trimmed.to_string();
                break;
            }
        }
    }
    n.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i16, m: i8, day: i8) -> Date {
        Date::new(y, m, day).unwrap()
    }

    #[test]
    fn status_roundtrip() {
        for s in [
            IpoStatus::Upcoming,
            IpoStatus::Open,
            IpoStatus::Closed,
            IpoStatus::Listed,
            IpoStatus::Withdrawn,
        ] {
            assert_eq!(IpoStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(IpoStatus::parse("bogus"), None);
        assert_eq!(IpoStatus::parse(""), None);
    }

    #[test]
    fn subcategory_parse_matches_source_vocabulary() {
        assert_eq!(SubCategory::parse("QIB"), Some(SubCategory::Qib));
        assert_eq!(SubCategory::parse("QIB (Ex Anchor)"), Some(SubCategory::Qib));
        assert_eq!(SubCategory::parse("NII"), Some(SubCategory::Nii));
        assert_eq!(SubCategory::parse("Retail"), Some(SubCategory::Retail));
        assert_eq!(SubCategory::parse("Total"), Some(SubCategory::Total));
        assert_eq!(SubCategory::parse("Employees"), None);
    }

    #[test]
    fn normalize_name_strips_suffixes() {
        assert_eq!(
            normalize_company_name("Ardee Industries Limited"),
            "ardee industries"
        );
        assert_eq!(normalize_company_name("Anawil Wire & Engineering"), "anawil wire & engineering");
        assert_eq!(normalize_company_name("Lohia Corp"), "lohia corp");
    }

    #[test]
    fn derive_status_lifecycle() {
        let today = d(2026, 8, 5);
        let mut ipo = Ipo::new("X", "test");

        ipo.open_date = Some(d(2026, 8, 10));
        ipo.close_date = Some(d(2026, 8, 12));
        ipo.derive_status(today);
        assert_eq!(ipo.status, IpoStatus::Upcoming);

        ipo.open_date = Some(d(2026, 8, 5));
        ipo.derive_status(today);
        assert_eq!(ipo.status, IpoStatus::Open);

        ipo.close_date = Some(d(2026, 8, 5));
        ipo.derive_status(today);
        assert_eq!(ipo.status, IpoStatus::Open); // closes end of day

        ipo.close_date = Some(d(2026, 8, 4));
        ipo.derive_status(today);
        assert_eq!(ipo.status, IpoStatus::Closed);

        ipo.listing_date = Some(d(2026, 8, 6));
        ipo.derive_status(today);
        assert_eq!(ipo.status, IpoStatus::Closed); // not yet listed

        ipo.listing_date = Some(d(2026, 8, 5));
        ipo.derive_status(today);
        assert_eq!(ipo.status, IpoStatus::Listed);
    }
}
