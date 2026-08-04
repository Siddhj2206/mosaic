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

use mosaic_core::{Error, Ipo, RateLimiter, Result};

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
            _ => {}
        }
    }
    Ok(detail)
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
