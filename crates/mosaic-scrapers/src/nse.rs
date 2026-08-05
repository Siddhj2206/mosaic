//! NSE official JSON API scraper (ADR-0001).
//!
//! Access mechanics: fresh cookie jar per run (GET home page first), browser
//! User-Agent, `Referer: https://www.nseindia.com/`, ≥2s spacing. Re-session
//! on 401/403/503 via the client's `with_retry`.

use std::time::Duration;

use jiff::civil::Date;
use reqwest::blocking::Client;
use reqwest::header::{HeaderValue, REFERER, USER_AGENT};
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;
use serde_json::Value;

use mosaic_core::{Error, Ipo, IpoScraper, PricePoint, RateLimiter, Result, SubCategory, SubscriptionSnapshot};

use crate::parse_util::{parse_band, parse_day_month_year, parse_err, parse_int, parse_lot, parse_period};

const BROWSER_UA: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:126.0) Gecko/20100101 Firefox/126.0";
const HOME_URL: &str = "https://www.nseindia.com/";
const CALENDAR_URL: &str = "https://www.nseindia.com/api/all-upcoming-issues?category=ipo";
const DETAIL_URL: &str = "https://www.nseindia.com/api/ipo-detail?symbol=";
const HISTORICAL_URL: &str = "https://www.nseindia.com/api/historicalOR/generateSecurityWiseHistoricalData";

/// NSE HTTP client: cookie session + rate limiting.
pub struct NseClient {
    http: Client,
    limiter: RateLimiter,
}

impl NseClient {
    /// Establish a session: browser-like client + warm-up GET on the home
    /// page to obtain session cookies.
    pub fn new() -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(BROWSER_UA));
        headers.insert(REFERER, HeaderValue::from_static(HOME_URL));

        let http = Client::builder()
            .default_headers(headers)
            .cookie_store(true)
            .build()
            .map_err(|e| Error::Http(e.to_string()))?;

        let mut client = NseClient {
            http,
            limiter: RateLimiter::new(Duration::from_secs(2)),
        };
        client.limiter.wait();
        let _ = client.http.get(HOME_URL).send(); // 403 is fine; cookies still set
        Ok(client)
    }

    /// GET a JSON endpoint, retrying once after re-sessioning on 503/401.
    pub fn get_json(&mut self, url: &str) -> Result<Value> {
        for attempt in 0..2 {
            self.limiter.wait();
            let resp = self
                .http
                .get(url)
                .send()
                .map_err(|e| Error::Http(e.to_string()))?;
            let status = resp.status();
            if status == reqwest::StatusCode::SERVICE_UNAVAILABLE
                || status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                if attempt == 0 {
                    // Re-establish the session and retry once.
                    let _ = self.http.get(HOME_URL).send();
                    continue;
                }
                return Err(Error::Http(format!("NSE {url}: HTTP {status}")));
            }
            if !status.is_success() {
                return Err(Error::Http(format!("NSE {url}: HTTP {status}")));
            }
            return resp.json::<Value>().map_err(|e| Error::Parse(format!("NSE {url}: {e}")));
        }
        Err(Error::Http(format!("NSE {url}: exhausted retries")))
    }
}

/// NSE scraper implementing the `IpoScraper` trait.
pub struct NseScraper {
    pub client: NseClient,
}

impl NseScraper {
    pub fn new() -> Result<Self> {
        Ok(NseScraper { client: NseClient::new()? })
    }

    /// Fetch + enrich the IPO calendar.
    pub fn fetch_calendar(&mut self, today: Date) -> Result<Vec<Ipo>> {
        let json = self.client.get_json(CALENDAR_URL)?;
        let mut ipos = parse_calendar(&json.to_string(), today)?;
        for ipo in &mut ipos {
            let symbol = match &ipo.symbol {
                Some(s) => s.clone(),
                None => continue,
            };
            let detail_url = format!("{DETAIL_URL}{symbol}");
            match self.client.get_json(&detail_url) {
                Ok(detail) => {
                    if let Ok(d) = parse_detail(&detail.to_string(), today) {
                        *ipo = d;
                    }
                }
                Err(e) => log::warn!("NSE detail for {symbol} failed: {e}"),
            }
        }
        Ok(ipos)
    }
}

impl IpoScraper for NseScraper {
    fn source(&self) -> &'static str {
        "nse"
    }

    fn fetch_ipos(&mut self, today: Date) -> Result<Vec<Ipo>> {
        self.fetch_calendar(today)
    }

    fn fetch_subscriptions(&mut self, ipo: &Ipo) -> Result<Vec<SubscriptionSnapshot>> {
        let symbol = ipo.symbol.as_deref().ok_or_else(|| parse_err("NSE subscription needs a symbol"))?;
        let json = self.client.get_json(&format!("{DETAIL_URL}{symbol}"))?;
        let mut snapshots = parse_active_cat(&json.to_string())?;
        let now = mosaic_core::types::now_utc();
        for s in &mut snapshots {
            s.ipo_id = ipo.id;
            s.snapshot_at = now.date();
        }
        Ok(snapshots)
    }

    fn fetch_price_history(&mut self, ipo: &Ipo) -> Result<Vec<PricePoint>> {
        let symbol = ipo.symbol.as_deref().ok_or_else(|| parse_err("NSE history needs a symbol"))?;
        let from = ipo.listing_date.unwrap_or_else(|| ipo.close_date.unwrap_or_else(|| jiff::Timestamp::now().to_zoned(jiff::tz::TimeZone::system()).date()));
        let to = jiff::Timestamp::now().to_zoned(jiff::tz::TimeZone::system()).date();
        let url = format!(
            "{HISTORICAL_URL}?from={}&to={}&symbol={}&type=priceVolumeDeliverable&series=EQ",
            fmt_nse_date(from),
            fmt_nse_date(to),
            symbol
        );
        let json = self.client.get_json(&url)?;
        let mut points = parse_historical(&json.to_string())?;
        for p in &mut points {
            p.ipo_id = ipo.id;
        }
        Ok(points)
    }
}

/// "05-Aug-2026" — the date format NSE endpoints expect.
pub fn fmt_nse_date(d: Date) -> String {
    let (year, month, day) = (d.year(), d.month(), d.day());
    format!("{:02}-{:02}-{year}", day, month)
}

// ---------------------------------------------------------------------------
// Parsers (pure; fixture-testable)
// ---------------------------------------------------------------------------

/// Parse `all-upcoming-issues` JSON into IPO records. Mainboard only
/// (`series == "EQ"`); status derived from dates against `today`.
pub fn parse_calendar(json: &str, today: Date) -> Result<Vec<Ipo>> {
    let value: Value = serde_json::from_str(json).map_err(|e| parse_err(format!("calendar JSON: {e}")))?;
    let rows = value
        .as_array()
        .ok_or_else(|| parse_err("calendar: expected array"))?;

    let mut out = Vec::new();
    for row in rows {
        let series = row.get("series").and_then(Value::as_str).unwrap_or("");
        if series != "EQ" {
            continue; // SME and others excluded in v1
        }
        let company_name = row.get("companyName").and_then(Value::as_str).unwrap_or("").trim().to_string();
        if company_name.is_empty() {
            continue;
        }
        let mut ipo = Ipo::new(company_name, "nse");
        ipo.symbol = row.get("symbol").and_then(Value::as_str).map(|s| s.to_string());
        ipo.open_date = row.get("issueStartDate").and_then(Value::as_str).and_then(|s| parse_day_month_year(s));
        ipo.close_date = row.get("issueEndDate").and_then(Value::as_str).and_then(|s| parse_day_month_year(s));
        ipo.shares_offered = row.get("issueSize").and_then(Value::as_str).and_then(parse_int);
        if let Some(band) = row.get("issuePrice").and_then(Value::as_str).and_then(parse_band) {
            ipo.price_band_low = Some(band.0);
            ipo.price_band_high = Some(band.1);
        }
        ipo.derive_status(today);
        out.push(ipo);
    }
    Ok(out)
}

/// Parse `ipo-detail` JSON: enrich an IPO record from `issueInfo.dataList`
/// and attach the RHP URL.
pub fn parse_detail(json: &str, _today: Date) -> Result<Ipo> {
    let value: Value = serde_json::from_str(json).map_err(|e| parse_err(format!("detail JSON: {e}")))?;

    let company_name = value
        .get("companyName")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let mut ipo = Ipo::new(company_name, "nse");

    let data_list = value
        .pointer("/issueInfo/dataList")
        .and_then(Value::as_array)
        .ok_or_else(|| parse_err("detail: issueInfo.dataList missing"))?;

    for item in data_list {
        let title = item.get("title").and_then(Value::as_str).unwrap_or("").trim();
        let raw_value = item.get("value").and_then(Value::as_str).unwrap_or("");
        let value = raw_value.trim();

        match title {
            "Symbol" => ipo.symbol = Some(value.to_string()),
            "Issue Period" => {
                if let Some((open, close)) = parse_period(value, _today) {
                    ipo.open_date = Some(open);
                    ipo.close_date = Some(close);
                }
            }
            "Price Range" => {
                if let Some((low, high)) = parse_band(value) {
                    ipo.price_band_low = Some(low);
                    ipo.price_band_high = Some(high);
                }
            }
            "Face Value" => {
                // "Rs.2 per Equity Share"
                let nums: Vec<Decimal> = value
                    .split_whitespace()
                    .filter_map(|t| crate::parse_util::parse_rupees(t))
                    .collect();
                if let Some(fv) = nums.first() {
                    ipo.face_value = Some(*fv);
                }
            }
            "Bid Lot" => {
                if let Some((min, mult)) = parse_lot(value) {
                    ipo.lot_size = Some(min);
                    ipo.lot_multiples = Some(mult);
                }
            }
            "Issue Type" => ipo.issue_type = Some(value.to_string()),
            "Red Herring Prospectus" => ipo.rhp_url = Some(value.to_string()),
            _ => {}
        }
    }

    if ipo.symbol.is_none() {
        return Err(parse_err("detail: no Symbol in issueInfo"));
    }
    Ok(ipo)
}

/// Parse `activeCat.dataList` into snapshot rows (category, offered, bid,
/// times). The NSE response includes a header row ("Category" as category)
/// which is skipped. `ipo_id`/`snapshot_at` are filled by the caller.
pub fn parse_active_cat(json: &str) -> Result<Vec<SubscriptionSnapshot>> {
    let value: Value = serde_json::from_str(json).map_err(|e| parse_err(format!("activeCat JSON: {e}")))?;
    let rows = value
        .pointer("/activeCat/dataList")
        .and_then(Value::as_array)
        .ok_or_else(|| parse_err("activeCat: dataList missing"))?;

    let mut out = Vec::new();
    for row in rows {
        let cat_str = row.get("category").and_then(Value::as_str).unwrap_or("");
        if cat_str.trim().eq_ignore_ascii_case("category") {
            continue; // header row
        }
        let Some(category) = SubCategory::parse(cat_str) else {
            continue; // unknown category — skip, keep the Total row
        };
        let mut snapshot = SubscriptionSnapshot::new(0, Date::ZERO, category, "nse");
        snapshot.offered_shares = row.get("noOfShareOffered").and_then(Value::as_str).and_then(parse_int);
        snapshot.bid_shares = row.get("noOfSharesBid").and_then(Value::as_str).and_then(parse_int);
        snapshot.times_subscribed = row
            .get("noOfTotalMeant")
            .and_then(Value::as_str)
            .and_then(|s| Decimal::from_str_exact(s).ok());
        out.push(snapshot);
    }
    Ok(out)
}

/// Parse `historicalOR` JSON into price points (OHLCV + VWAP + volume).
pub fn parse_historical(json: &str) -> Result<Vec<PricePoint>> {
    let value: Value = serde_json::from_str(json).map_err(|e| parse_err(format!("historical JSON: {e}")))?;
    let rows = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| parse_err("historical: data missing"))?;

    let mut out = Vec::new();
    for row in rows {
        let Some(trade_date) = row.get("mTIMESTAMP").and_then(Value::as_str).and_then(|s| parse_day_month_year(s)) else {
            continue;
        };
        let mut p = PricePoint::new(0, trade_date, "nse");
        p.open_price = num(row, "CH_OPENING_PRICE");
        p.high_price = num(row, "CH_TRADE_HIGH_PRICE");
        p.low_price = num(row, "CH_TRADE_LOW_PRICE");
        p.close_price = num(row, "CH_CLOSING_PRICE");
        p.vwap = num(row, "VWAP");
        p.volume = row.get("CH_TOT_TRADED_QTY").and_then(Value::as_u64).map(|v| v as i64);
        out.push(p);
    }
    Ok(out)
}

fn num(row: &Value, key: &str) -> Option<Decimal> {
    row.get(key).and_then(|v| {
        v.as_f64()
            .and_then(|f| Decimal::from_f64(f))
            .or_else(|| v.as_str().and_then(|s| Decimal::from_str_exact(s).ok()))
    })
}

// ---------------------------------------------------------------------------
// Fixture tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use mosaic_core::IpoStatus;

    const TODAY: Date = Date::constant(2026, 8, 5);

    #[test]
    fn calendar_parses_mainboard_rows() {
        let json = include_str!("test_fixtures/nse-all-upcoming-issues.json");
        let ipos = parse_calendar(json, TODAY).unwrap();
        assert_eq!(ipos.len(), 1);
        let ipo = &ipos[0];
        assert_eq!(ipo.company_name, "Ardee Industries Limited");
        assert_eq!(ipo.symbol.as_deref(), Some("ARDEE"));
        assert_eq!(ipo.open_date, Some(Date::constant(2026, 8, 5)));
        assert_eq!(ipo.close_date, Some(Date::constant(2026, 8, 7)));
        assert_eq!(ipo.price_band_low, Some(Decimal::from(50)));
        assert_eq!(ipo.price_band_high, Some(Decimal::from(53)));
        assert_eq!(ipo.shares_offered, Some(58422516));
        assert_eq!(ipo.status, IpoStatus::Open);
    }

    #[test]
    fn detail_parses_issue_info() {
        let json = include_str!("test_fixtures/nse-ipo-detail-ARDEE.json");
        let ipo = parse_detail(json, TODAY).unwrap();
        assert_eq!(ipo.symbol.as_deref(), Some("ARDEE"));
        assert_eq!(ipo.open_date, Some(Date::constant(2026, 8, 5)));
        assert_eq!(ipo.close_date, Some(Date::constant(2026, 8, 7)));
        assert_eq!(ipo.price_band_low, Some(Decimal::from(50)));
        assert_eq!(ipo.price_band_high, Some(Decimal::from(53)));
        assert_eq!(ipo.face_value, Some(Decimal::from(2)));
        assert_eq!(ipo.lot_size, Some(281));
        assert_eq!(ipo.lot_multiples, Some(281));
        assert_eq!(ipo.issue_type.as_deref(), Some("100% Book Building"));
        assert!(ipo.rhp_url.as_deref().unwrap_or("").contains("RHP_ARDEE.zip"));
    }

    #[test]
    fn active_cat_skips_header_row() {
        let json = include_str!("test_fixtures/nse-ipo-detail-ARDEE.json");
        let rows = parse_active_cat(json).unwrap();
        // Day-1 fixture: header row skipped, only Total present.
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].category, SubCategory::Total);
        assert_eq!(rows[0].times_subscribed, Some(Decimal::ZERO));
    }

    #[test]
    fn active_cat_full_window_shape() {
        // Full-window shape: QIB/NII/Retail/Total rows in NSE's exact format.
        let json = r#"{
          "activeCat": {
            "dataList": [
              {"category": "Category", "noOfShareOffered": "offered", "noOfSharesBid": "bid", "noOfTotalMeant": "times", "srNo": "1"},
              {"category": "QIB", "noOfShareOffered": "58422429.0", "noOfSharesBid": "123456789.0", "noOfTotalMeant": "2.11", "srNo": null},
              {"category": "NII", "noOfShareOffered": "17526729.0", "noOfSharesBid": "45678901.0", "noOfTotalMeant": "2.61", "srNo": null},
              {"category": "Retail", "noOfShareOffered": "5842243.0", "noOfSharesBid": "98765432.0", "noOfTotalMeant": "16.91", "srNo": null},
              {"category": "Total", "noOfShareOffered": "81791301.0", "noOfSharesBid": "267909122.0", "noOfTotalMeant": "3.28", "srNo": null}
            ]
          }
        }"#;
        let rows = parse_active_cat(json).unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].category, SubCategory::Qib);
        assert_eq!(rows[0].times_subscribed, Some(Decimal::from_str_exact("2.11").unwrap()));
        assert_eq!(rows[3].category, SubCategory::Total);
    }

    #[test]
    fn historical_parses_ohlcv() {
        let json = include_str!("test_fixtures/nse-historical-RELIANCE.json");
        let points = parse_historical(json).unwrap();
        assert_eq!(points.len(), 1);
        let p = &points[0];
        assert_eq!(p.trade_date, Date::constant(2026, 8, 4));
        assert_eq!(p.close_price, Some(Decimal::from_f64(1290.9).unwrap()));
        assert_eq!(p.volume, Some(10759546));
        assert!(p.vwap.is_some());
    }

    #[test]
    fn nse_date_format() {
        assert_eq!(fmt_nse_date(Date::constant(2026, 8, 4)), "04-08-2026");
    }
}

// ---------------------------------------------------------------------------
// Live HTTP tests (excluded from `cargo test`; run with -- --ignored)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod live_tests {
    use super::*;

    #[test]
    #[ignore]
    fn live_calendar_and_detail() {
        let mut scraper = NseScraper::new().unwrap();
        let today = jiff::Timestamp::now().to_zoned(jiff::tz::TimeZone::system()).date();
        let ipos = scraper.fetch_ipos(today).unwrap();
        assert!(!ipos.is_empty(), "expected at least one calendar row");
        let ipo = &ipos[0];
        assert!(ipo.symbol.is_some());
        assert!(ipo.company_name.len() > 3);
        println!("calendar: {} rows; first: {} ({})", ipos.len(), ipo.company_name, ipo.symbol.as_deref().unwrap_or("-"));
    }

    #[test]
    #[ignore]
    fn live_subscription_poll() {
        let mut scraper = NseScraper::new().unwrap();
        let mut ipo = Ipo::new("Ardee Industries Limited", "nse");
        ipo.symbol = Some("ARDEE".into());
        ipo.id = Some(1);
        let rows = scraper.fetch_subscriptions(&ipo).unwrap();
        // Day-1 windows may only expose Total; at least the header must be skipped.
        assert!(rows.iter().all(|r| r.category != SubCategory::Total || r.times_subscribed.is_some()));
        println!("subscription rows: {:?}", rows.iter().map(|r| (r.category.as_str(), r.times_subscribed)).collect::<Vec<_>>());
    }

    #[test]
    #[ignore]
    fn live_eod_history() {
        let mut scraper = NseScraper::new().unwrap();
        let mut ipo = Ipo::new("Reliance Industries", "nse");
        ipo.symbol = Some("RELIANCE".into());
        ipo.id = Some(1);
        ipo.listing_date = Some(jiff::civil::Date::constant(2026, 7, 1));
        let points = scraper.fetch_price_history(&ipo).unwrap();
        assert!(!points.is_empty());
        assert!(points[0].close_price.is_some());
        println!("eod rows: {}; last close: {:?}", points.len(), points.last().and_then(|p| p.close_price));
    }
}
