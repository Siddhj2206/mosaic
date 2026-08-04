//! IPO Watch scraper: the server-rendered mainboard archive.
//!
//! `https://ipowatch.in/upcoming-ipo-list/` serves a plain HTML table of
//! recent + upcoming mainboard IPOs (company, window, size, band) and a
//! separate SME table. v1 uses it as the fallback archive for closed/listed
//! companies NSE no longer serves.

use std::time::Duration;

use jiff::civil::Date;
use reqwest::blocking::Client;
use scraper::{Html, Selector};

use mosaic_core::{Error, Ipo, IpoScraper, RateLimiter, Result};

use crate::parse_util::{parse_band, parse_period, parse_rupees};

const ARCHIVE_URL: &str = "https://ipowatch.in/upcoming-ipo-list/";

/// One row of the mainboard archive table.
#[derive(Debug, Clone, PartialEq)]
pub struct IpoWatchRow {
    pub company: String,
    pub period: String,
    pub issue_size: Option<rust_decimal::Decimal>,
    pub price_band_low: Option<rust_decimal::Decimal>,
    pub price_band_high: Option<rust_decimal::Decimal>,
}

pub struct IpoWatchScraper {
    http: Client,
    limiter: RateLimiter,
}

impl IpoWatchScraper {
    pub fn new() -> Result<Self> {
        let http = Client::builder()
            .user_agent("Mozilla/5.0 (X11; Linux x86_64; rv:126.0) Gecko/20100101 Firefox/126.0")
            .build()
            .map_err(|e| Error::Http(e.to_string()))?;
        Ok(IpoWatchScraper {
            http,
            limiter: RateLimiter::new(Duration::from_secs(2)),
        })
    }

    /// Fetch the recent/upcoming mainboard archive (includes recently listed).
    pub fn fetch_archive(&mut self) -> Result<Vec<IpoWatchRow>> {
        self.limiter.wait();
        let resp = self
            .http
            .get(ARCHIVE_URL)
            .send()
            .map_err(|e| Error::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(Error::Http(format!("ipowatch: HTTP {}", resp.status())));
        }
        let html = resp.text().map_err(|e| Error::Http(e.to_string()))?;
        parse_archive(&html)
    }
}

/// Parse the archive page: the mainboard table is the one whose header is
/// Company | IPO Date | IPO Size | IPO Price Band | Application (the SME
/// table has a "Platform" column instead).
pub fn parse_archive(html: &str) -> Result<Vec<IpoWatchRow>> {
    let document = Html::parse_document(html);
    let table_sel = Selector::parse("table").map_err(|e| Error::parse(e.to_string()))?;
    let row_sel = Selector::parse("tr").map_err(|e| Error::parse(e.to_string()))?;
    let cell_sel = Selector::parse("td, th").map_err(|e| Error::parse(e.to_string()))?;

    let mut out = Vec::new();
    for table in document.select(&table_sel) {
        let rows: Vec<Vec<String>> = table
            .select(&row_sel)
            .map(|row| {
                row.select(&cell_sel)
                    .map(|c| c.text().collect::<String>().trim().to_string())
                    .collect()
            })
            .filter(|cells: &Vec<String>| !cells.is_empty())
            .collect();

        // Find the mainboard table: header contains "IPO Date" but not "Platform".
        let Some(header) = rows.first() else { continue };
        let has_platform = header.iter().any(|h| h.eq_ignore_ascii_case("platform"));
        if !header.iter().any(|h| h.eq_ignore_ascii_case("ipo date")) || has_platform {
            continue;
        }

        for row in rows.iter().skip(1) {
            if row.len() < 3 {
                continue;
            }
            let company = row[0].replace("&amp;", "&").trim().to_string();
            if company.is_empty() || company.contains("₹[.]") {
                continue; // placeholder rows
            }
            let mut entry = IpoWatchRow {
                company,
                period: row[1].clone(),
                issue_size: None,
                price_band_low: None,
                price_band_high: None,
            };
            // Size: "₹3,066.89 Cr." — parse rupees (includes Cr multiplier).
            if let Some(size) = row.get(2).and_then(|s| parse_rupees(s)) {
                // parse_rupees multiplies by 10^7 for "Cr."; issue_size_cr
                // stores ₹ crore, so divide back.
                entry.issue_size = Some(size / rust_decimal::Decimal::from(1000_0000u64));
            }
            if let Some(band) = row.get(3).and_then(|s| parse_band(s)) {
                entry.price_band_low = Some(band.0);
                entry.price_band_high = Some(band.1);
            }
            out.push(entry);
        }
    }
    Ok(out)
}

/// Convert archive rows into `Ipo` records (status derived for `today`).
pub fn rows_to_ipos(rows: &[IpoWatchRow], today: Date) -> Vec<Ipo> {
    let mut ipos = Vec::new();
    for row in rows {
        let mut ipo = Ipo::new(&row.company, "ipowatch");
        if let Some((open, close)) = parse_period(&row.period, today) {
            ipo.open_date = Some(open);
            ipo.close_date = Some(close);
        }
        ipo.issue_size_cr = row.issue_size;
        ipo.price_band_low = row.price_band_low;
        ipo.price_band_high = row.price_band_high;
        ipo.derive_status(today);
        ipos.push(ipo);
    }
    ipos
}

// ---------------------------------------------------------------------------
// Fixture tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const TODAY: Date = Date::constant(2026, 8, 5);

    #[test]
    fn archive_parses_mainboard_and_skips_sme() {
        let html = include_str!("test_fixtures/ipowatch-upcoming.html");
        let rows = parse_archive(html).unwrap();
        // Mainboard table only (SME table has "Platform" column).
        assert!(rows.len() >= 10, "expected >=10 mainboard rows, got {}", rows.len());
        assert!(rows.iter().all(|r| !r.company.contains("SME")));

        let ardee = rows.iter().find(|r| r.company.contains("Ardee")).expect("Ardee row");
        assert_eq!(ardee.period, "5-7 August");
        assert_eq!(ardee.price_band_low, Some(rust_decimal::Decimal::from(50)));
        assert_eq!(ardee.price_band_high, Some(rust_decimal::Decimal::from(53)));

        let dhoot = rows.iter().find(|r| r.company.contains("Dhoot")).expect("Dhoot row");
        // ₹3,066.89 Cr → 3066.89
        assert_eq!(
            dhoot.issue_size,
            Some(rust_decimal::Decimal::from_str_exact("3066.89").unwrap())
        );
    }

    #[test]
    fn rows_become_ipos_with_derived_status() {
        let html = include_str!("test_fixtures/ipowatch-upcoming.html");
        let rows = parse_archive(html).unwrap();
        let ipos = rows_to_ipos(&rows, TODAY);
        let ardee = ipos.iter().find(|i| i.normalized_name == "ardee industries").unwrap();
        assert_eq!(ardee.open_date, Some(Date::constant(2026, 8, 5)));
        assert_eq!(ardee.close_date, Some(Date::constant(2026, 8, 7)));
        assert_eq!(ardee.status, mosaic_core::IpoStatus::Open);
        // No listing date in the archive table, so a closed window derives
        // as Closed until NSE/Chittorgarh detail provides the listing date.
        let manipal = ipos.iter().find(|i| i.normalized_name == "manipal health").unwrap();
        assert_eq!(manipal.status, mosaic_core::IpoStatus::Closed);
    }
}

// ---------------------------------------------------------------------------
// IpoScraper impl
// ---------------------------------------------------------------------------

impl IpoScraper for IpoWatchScraper {
    fn source(&self) -> &'static str {
        "ipowatch"
    }

    /// Fetch the recent/upcoming mainboard archive as IPO records.
    fn fetch_ipos(&mut self, today: Date) -> Result<Vec<Ipo>> {
        let rows = self.fetch_archive()?;
        Ok(rows_to_ipos(&rows, today))
    }

    /// IPO Watch serves no subscription data.
    fn fetch_subscriptions(&mut self, _ipo: &Ipo) -> Result<Vec<mosaic_core::SubscriptionSnapshot>> {
        Ok(Vec::new())
    }

    /// EOD history comes from NSE.
    fn fetch_price_history(&mut self, _ipo: &Ipo) -> Result<Vec<mosaic_core::PricePoint>> {
        Ok(Vec::new())
    }
}
