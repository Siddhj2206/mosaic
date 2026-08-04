//! Chittorgarh scraper: server-rendered pages only.
//!
//! Chittorgarh's report pages (subscription, perf tracker, mainboard list)
//! are Next.js client-rendered and NOT scrapeable via plain HTTP — v1 uses
//! the classic ASP dashboard (`ipo_dashboard.asp`) for the current calendar
//! and per-IPO detail pages for enrichment. IPO Watch covers the closed/
//! listed archive (see `ipowatch.rs`).

use std::time::Duration;

use reqwest::blocking::Client;
use scraper::{Html, Selector};

use rust_decimal::prelude::FromPrimitive;

use mosaic_core::{
    Error, Ipo, IpoScraper, IpoStatus, RateLimiter, Result, SubCategory, SubscriptionSnapshot,
};

use crate::parse_util::{parse_band, parse_day_month_year, parse_int, parse_month_day, parse_period, parse_rupees};

const BASE_URL: &str = "https://www.chittorgarh.com";
const DASHBOARD_URL: &str = "https://www.chittorgarh.com/ipo/ipo_dashboard.asp";

/// One row of the dashboard calendar: company + issue window + detail URL.
#[derive(Debug, Clone, PartialEq)]
pub struct DashboardEntry {
    pub company: String,
    pub period: String,
    pub detail_url: Option<String>,
}

/// Parsed key facts from a Chittorgarh IPO detail page.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChittorgarhDetail {
    pub company: String,
    pub open_date: Option<jiff::civil::Date>,
    pub close_date: Option<jiff::civil::Date>,
    pub listing_date: Option<jiff::civil::Date>,
    pub listing_date_tentative: bool,
    pub price_band_low: Option<rust_decimal::Decimal>,
    pub price_band_high: Option<rust_decimal::Decimal>,
    pub final_price: Option<rust_decimal::Decimal>,
    pub face_value: Option<rust_decimal::Decimal>,
    pub lot_size: Option<i64>,
    pub offer_type: Option<String>,
    pub issue_type: Option<String>,
    pub listing_at: Option<String>,
    pub fresh_issue_shares: Option<i64>,
    pub ofs_shares: Option<i64>,
    pub shares_offered: Option<i64>,
    /// NSE symbol, embedded in the page JSON as `symbol":"MANIPALHOS"`.
    pub symbol: Option<String>,
    /// Final-day subscription rows from the detail page (QIB/NII/Retail/Total).
    /// `ipo_id`/`snapshot_at` are filled by the caller.
    pub final_subscription: Vec<mosaic_core::SubscriptionSnapshot>,
}

pub struct ChittorgarhScraper {
    http: Client,
    limiter: RateLimiter,
}

impl ChittorgarhScraper {
    pub fn new() -> Result<Self> {
        let http = Client::builder()
            .user_agent("Mozilla/5.0 (X11; Linux x86_64; rv:126.0) Gecko/20100101 Firefox/126.0")
            .build()
            .map_err(|e| Error::Http(e.to_string()))?;
        Ok(ChittorgarhScraper {
            http,
            limiter: RateLimiter::new(Duration::from_secs(2)),
        })
    }

    fn get(&mut self, url: &str) -> Result<String> {
        self.limiter.wait();
        let resp = self
            .http
            .get(url)
            .send()
            .map_err(|e| Error::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(Error::Http(format!("chittorgarh {url}: HTTP {}", resp.status())));
        }
        resp.text().map_err(|e| Error::Http(e.to_string()))
    }

    /// Current calendar from the dashboard (upcoming/open; ~15 rows).
    pub fn fetch_dashboard(&mut self) -> Result<Vec<DashboardEntry>> {
        let html = self.get(DASHBOARD_URL)?;
        parse_dashboard(&html)
    }

    /// Enrich a detail page for one company.
    pub fn fetch_detail(&mut self, url: &str) -> Result<ChittorgarhDetail> {
        let html = self.get(url)?;
        parse_detail(&html)
    }
}

// ---------------------------------------------------------------------------
// Parsers (pure; fixture-testable)
// ---------------------------------------------------------------------------

/// Parse the dashboard's current-IPO table (Company | Issue Date) with
/// per-row detail links.
pub fn parse_dashboard(html: &str) -> Result<Vec<DashboardEntry>> {
    let document = Html::parse_document(html);
    let row_sel = Selector::parse("table tr").map_err(|e| Error::parse(e.to_string()))?;
    let cell_sel = Selector::parse("td, th").map_err(|e| Error::parse(e.to_string()))?;
    let link_sel = Selector::parse("a").map_err(|e| Error::parse(e.to_string()))?;

    let mut out = Vec::new();
    for row in document.select(&row_sel) {
        let cells: Vec<String> = row
            .select(&cell_sel)
            .map(|c| c.text().collect::<String>().trim().to_string())
            .filter(|c| !c.is_empty())
            .collect();
        if cells.is_empty() {
            continue;
        }
        // Skip header row.
        if cells[0].eq_ignore_ascii_case("company") {
            continue;
        }
        let detail_url = row
            .select(&link_sel)
            .filter_map(|a| a.value().attr("href"))
            .find(|h| h.contains("/ipo/"))
            .map(|h| h.to_string());
        let entry = DashboardEntry {
            company: cells[0].clone(),
            period: cells.get(1).cloned().unwrap_or_default(),
            detail_url,
        };
        out.push(entry);
    }
    Ok(out)
}

/// Parse a detail page's key-facts + issue-split tables.
pub fn parse_detail(html: &str) -> Result<ChittorgarhDetail> {
    let document = Html::parse_document(html);
    let table_sel = Selector::parse("table").map_err(|e| Error::parse(e.to_string()))?;
    let row_sel = Selector::parse("tr").map_err(|e| Error::parse(e.to_string()))?;
    let cell_sel = Selector::parse("td, th").map_err(|e| Error::parse(e.to_string()))?;

    let mut detail = ChittorgarhDetail::default();

    // Company name lives in the page <h1>, e.g. "Manipal Health Enterprises  IPO Details".
    let h1_sel = Selector::parse("h1").map_err(|e| Error::parse(e.to_string()))?;
    if let Some(h1) = document.select(&h1_sel).next() {
        let text = h1.text().collect::<String>();
        detail.company = text
            .replace("IPO Details", "")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
    }

    for table in document.select(&table_sel) {
        let mut rows: Vec<Vec<String>> = Vec::new();
        for row in table.select(&row_sel) {
            let cells: Vec<String> = row
                .select(&cell_sel)
                .map(|c| c.text().collect::<String>().trim().to_string())
                .collect();
            if !cells.is_empty() {
                rows.push(cells);
            }
        }
        if rows.is_empty() {
            continue;
        }

        // Detect table by header content.
        let header = &rows[0][0];
        match header.as_str() {
            "IPO Date" => parse_key_facts(&rows, &mut detail),
            "Total Issue Size" => parse_issue_split(&rows, &mut detail),
            "Category" if rows[0].get(1).is_some_and(|c| c.contains("Subscription")) => {
                detail.final_subscription = parse_subscription_table(&rows);
            }
            _ => {}
        }
    }

    // NSE symbol is embedded in the page payload: "symbol":"MANIPALHOS"
    if detail.symbol.is_none() {
        if let Some(cap) = extract_embedded_symbol(html) {
            detail.symbol = Some(cap);
        }
    }
    Ok(detail)
}

/// Hand-rolled extraction of the NSE symbol embedded in the page payload:
/// `symbol":"MANIPALHOS"` (with optional backslash escapes).
fn extract_embedded_symbol(haystack: &str) -> Option<String> {
    let mut search_from = 0;
    while let Some(idx) = haystack[search_from..].find("symbol") {
        let start = search_from + idx;
        let rest = &haystack[start + 6..];
        if let Some(colon) = rest.find(':') {
            let after = rest[colon + 1..].trim_start_matches(|c| c == '"' || c == '\\');
            let token: String = after
                .chars()
                .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                .collect();
            if token.len() >= 2 {
                return Some(token);
            }
        }
        search_from = start + 6;
        if search_from >= haystack.len() {
            break;
        }
    }
    None
}

fn parse_key_facts(rows: &[Vec<String>], detail: &mut ChittorgarhDetail) {
    for row in rows {
        if row.len() < 2 {
            continue;
        }
        let label = row[0].trim();
        let value = row[1].trim();
        match label {
            "Company Name" | "Company" => detail.company = value.to_string(),
            "IPO Date" => {
                if let Some((open, close)) = parse_period_now_fallback(value) {
                    detail.open_date = Some(open);
                    detail.close_date = Some(close);
                }
            }
            "Listing Date" => {
                detail.listing_date = parse_day_month_year(value).or_else(|| parse_month_day(value));
                detail.listing_date_tentative = value.contains("(T)") || value.contains("Tentative");
            }
            "Price Band" => {
                if let Some((low, high)) = parse_band(value) {
                    detail.price_band_low = Some(low);
                    detail.price_band_high = Some(high);
                }
            }
            "Issue Price" => detail.final_price = parse_rupees(value),
            "Face Value" => detail.face_value = parse_rupees(value),
            "Lot Size" => detail.lot_size = parse_int(value),
            "Sale Type" => detail.offer_type = Some(value.to_string()),
            "Issue Type" => detail.issue_type = Some(value.to_string()),
            "Listing At" => detail.listing_at = Some(value.to_string()),
            _ => {}
        }
    }
}

fn parse_issue_split(rows: &[Vec<String>], detail: &mut ChittorgarhDetail) {
    for row in rows {
        if row.len() < 2 {
            continue;
        }
        let label = row[0].trim();
        let value = row[1].trim();
        match label {
            "Total Issue Size" => detail.shares_offered = parse_int(value),
            "Fresh Issue" => detail.fresh_issue_shares = parse_int(value),
            "Offer for Sale" => detail.ofs_shares = parse_int(value),
            _ => {}
        }
    }
}

/// Parse the detail page's final subscription table into snapshot rows.
/// Category mapping: "QIB (Ex Anchor)" → qib, "NII" → nii, "Retail" → retail,
/// "Total" → total; bNII/sNII/Employee rows are skipped (no NSE equivalent).
fn parse_subscription_table(rows: &[Vec<String>]) -> Vec<SubscriptionSnapshot> {
    let mut out = Vec::new();
    for row in rows.iter().skip(1) {
        if row.len() < 4 {
            continue;
        }
        let Some(category) = SubCategory::parse(&row[0]) else {
            continue;
        };
        let mut snapshot = SubscriptionSnapshot::new(0, jiff::civil::Date::ZERO, category, "chittorgarh");
        snapshot.times_subscribed = row[1].trim().parse::<f64>().ok().and_then(rust_decimal::Decimal::from_f64);
        snapshot.offered_shares = crate::parse_util::parse_int(&row[2]);
        snapshot.bid_shares = crate::parse_util::parse_int(&row[3]);
        out.push(snapshot);
    }
    out
}

/// Parse a period string using today's date for year defaults.
fn parse_period_now_fallback(s: &str) -> Option<(jiff::civil::Date, jiff::civil::Date)> {
    let today = jiff::Timestamp::now()
        .to_zoned(jiff::tz::TimeZone::system())
        .date();
    parse_period(s, today)
}

// ---------------------------------------------------------------------------
// Fixture tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_parses_current_ipos() {
        let html = include_str!("test_fixtures/chittorgarh-dashboard.html");
        let entries = parse_dashboard(html).unwrap();
        assert!(entries.len() >= 5, "expected >=5 rows, got {}", entries.len());
        let ardee = entries.iter().find(|e| e.company.contains("Ardee")).expect("Ardee on dashboard");
        assert_eq!(
            ardee.detail_url.as_deref(),
            Some("/ipo/ardee-industries-ipo/2860/")
        );
    }

    #[test]
    fn detail_extracts_symbol_and_final_subscription() {
        let html = include_str!("test_fixtures/chittorgarh-detail-manipal.html");
        let detail = parse_detail(html).unwrap();
        assert_eq!(detail.symbol.as_deref(), Some("MANIPALHOS"));
        // QIB (Ex Anchor), NII, Retail, Total mapped; bNII/sNII/Employee skipped.
        let cats: Vec<_> = detail.final_subscription.iter().map(|s| s.category).collect();
        assert_eq!(
            cats,
            vec![
                SubCategory::Qib,
                SubCategory::Nii,
                SubCategory::Retail,
                SubCategory::Total,
            ]
        );
        let total = detail
            .final_subscription
            .iter()
            .find(|s| s.category == SubCategory::Total)
            .unwrap();
        assert_eq!(total.times_subscribed, Some(rust_decimal::Decimal::from_str_exact("5.12").unwrap()));
        assert_eq!(total.offered_shares, Some(86604947));
    }

    #[test]
    fn detail_parses_key_facts_and_split() {
        let html = include_str!("test_fixtures/chittorgarh-detail-manipal.html");
        let detail = parse_detail(html).unwrap();
        assert_eq!(detail.company, "Manipal Health Enterprises");
        assert_eq!(detail.open_date, Some(jiff::civil::Date::constant(2026, 7, 29)));
        assert_eq!(detail.close_date, Some(jiff::civil::Date::constant(2026, 7, 31)));
        assert_eq!(detail.listing_date, Some(jiff::civil::Date::constant(2026, 8, 5)));
        assert_eq!(detail.price_band_low, Some(rust_decimal::Decimal::from(560)));
        assert_eq!(detail.price_band_high, Some(rust_decimal::Decimal::from(590)));
        assert_eq!(detail.final_price, Some(rust_decimal::Decimal::from(590)));
        assert_eq!(detail.face_value, Some(rust_decimal::Decimal::from(2)));
        assert_eq!(detail.lot_size, Some(25));
        assert_eq!(detail.offer_type.as_deref(), Some("Fresh capital cum OFS"));
        assert_eq!(detail.issue_type.as_deref(), Some("Bookbuilding IPO"));
        assert_eq!(detail.listing_at.as_deref(), Some("BSE, NSE"));
        assert_eq!(detail.shares_offered, Some(157233715));
        assert_eq!(detail.fresh_issue_shares, Some(135619881));
        assert_eq!(detail.ofs_shares, Some(21613834));
    }
}

// ---------------------------------------------------------------------------
// IpoScraper impl
// ---------------------------------------------------------------------------

impl IpoScraper for ChittorgarhScraper {
    fn source(&self) -> &'static str {
        "chittorgarh"
    }

    /// Fetch the dashboard calendar, enrich each entry via its detail page,
    /// and return IPO records (current window; ~15 rows).
    fn fetch_ipos(&mut self, today: jiff::civil::Date) -> Result<Vec<Ipo>> {
        let entries = self.fetch_dashboard()?;
        let mut out = Vec::new();
        for entry in entries {
            let Some(url) = &entry.detail_url else {
                continue;
            };
            let full_url = if url.starts_with("http") {
                url.clone()
            } else {
                format!("{BASE_URL}{url}")
            };
            match self.fetch_detail(&full_url) {
                Ok(detail) => out.push(detail_to_ipo(&detail, &full_url, today)),
                Err(e) => log::warn!("chittorgarh detail {full_url}: {e}"),
            }
        }
        Ok(out)
    }

    /// Final-day subscription from the detail page (for closed IPOs NSE no
    /// longer serves). `snapshot_at` = close date, or today when unknown.
    fn fetch_subscriptions(&mut self, ipo: &Ipo) -> Result<Vec<SubscriptionSnapshot>> {
        let Some(url) = &ipo.detail_url else {
            return Ok(Vec::new());
        };
        let detail = self.fetch_detail(url)?;
        let snapshot_at = ipo.close_date.unwrap_or_else(|| {
            jiff::Timestamp::now().to_zoned(jiff::tz::TimeZone::system()).date()
        });
        let mut out = Vec::new();
        for mut s in detail.final_subscription {
            s.ipo_id = ipo.id;
            s.snapshot_at = snapshot_at;
            out.push(s);
        }
        Ok(out)
    }

    /// EOD history comes from NSE; Chittorgarh serves none.
    fn fetch_price_history(&mut self, _ipo: &Ipo) -> Result<Vec<mosaic_core::PricePoint>> {
        Ok(Vec::new())
    }
}

/// Convert a parsed detail page into an IPO record.
pub fn detail_to_ipo(detail: &ChittorgarhDetail, detail_url: &str, today: jiff::civil::Date) -> Ipo {
    let mut ipo = Ipo::new(&detail.company, "chittorgarh");
    ipo.detail_url = Some(detail_url.to_string());
    ipo.symbol = detail.symbol.clone();
    ipo.exchange = detail.listing_at.clone();
    ipo.open_date = detail.open_date;
    ipo.close_date = detail.close_date;
    ipo.listing_date = detail.listing_date;
    ipo.listing_date_tentative = detail.listing_date_tentative;
    ipo.price_band_low = detail.price_band_low;
    ipo.price_band_high = detail.price_band_high;
    ipo.final_price = detail.final_price;
    ipo.face_value = detail.face_value;
    ipo.lot_size = detail.lot_size;
    ipo.offer_type = detail.offer_type.clone();
    ipo.issue_type = detail.issue_type.clone();
    ipo.shares_offered = detail.shares_offered;
    ipo.fresh_issue_shares = detail.fresh_issue_shares;
    ipo.ofs_shares = detail.ofs_shares;
    if ipo.listing_date.is_some() {
        ipo.status = IpoStatus::Listed;
    } else {
        ipo.derive_status(today);
    }
    ipo
}
