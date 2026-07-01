use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use mosaic_core::scraper::{IpoScraper, ScrapeError};
use mosaic_core::types::{Date, DateTime, Ipo, IpoStatus, PricePoint, SubscriptionEntry};
use rust_decimal::Decimal;
use scraper::{Html, Selector};

const BASE_URL: &str = "https://www.chittorgarh.com";
const DASHBOARD_PATH: &str = "/ipo/ipo_dashboard.asp";

pub struct ChittorgarhScraper {
    client: reqwest::blocking::Client,
    detail_cache: RefCell<HashMap<String, (String, String)>>,
    request_delay: Duration,
}

impl ChittorgarhScraper {
    pub fn new() -> Self {
        Self::with_delay(Duration::from_secs(2))
    }

    pub fn with_delay(delay: Duration) -> Self {
        let client = reqwest::blocking::Client::builder()
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");
        Self {
            client,
            detail_cache: RefCell::new(HashMap::new()),
            request_delay: delay,
        }
    }

    fn fetch_html(&self, url: &str) -> Result<String, ScrapeError> {
        let resp = self
            .client
            .get(url)
            .send()
            .map_err(|e| ScrapeError::Http(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(ScrapeError::Http(format!("HTTP {} for {}", status, url)));
        }
        resp.text().map_err(|e| ScrapeError::Http(e.to_string()))
    }

    fn dashboard_url() -> String {
        format!("{}{}", BASE_URL, DASHBOARD_PATH)
    }

}

#[derive(Debug)]
struct DashboardIpo {
    company_name: String,
    href: String,
    badge: String,
    date_range: String,
    row_class: String,
}

fn parse_dashboard(html: &str) -> Result<Vec<DashboardIpo>, ScrapeError> {
    let doc = Html::parse_document(html);

    let table_sel =
        Selector::parse("table.table.striped.my-0.table-hover").map_err(|e| ScrapeError::Parse(e.to_string()))?;
    let row_sel = Selector::parse("tr").map_err(|e| ScrapeError::Parse(e.to_string()))?;
    let link_sel =
        Selector::parse("td > a.text-decoration-none").map_err(|e| ScrapeError::Parse(e.to_string()))?;
    let badge_sel =
        Selector::parse("span.badge.rounded-pill").map_err(|e| ScrapeError::Parse(e.to_string()))?;
    let date_sel =
        Selector::parse("span.float-end.ms-2").map_err(|e| ScrapeError::Parse(e.to_string()))?;

    let table = doc
        .select(&table_sel)
        .next()
        .ok_or_else(|| ScrapeError::Parse("Dashboard table not found".into()))?;

    let mut ipos = Vec::new();

    for row in table.select(&row_sel) {
        let class = row.value().attr("class").unwrap_or("");
        if class.is_empty() && row.select(&link_sel).next().is_none() {
            continue;
        }

        let Some(link) = row.select(&link_sel).next() else {
            continue;
        };

        let company_name = link
            .value()
            .attr("title")
            .or_else(|| link.text().next())
            .unwrap_or("")
            .trim()
            .to_string();

        if company_name.is_empty() {
            continue;
        }

        let href = link.value().attr("href").unwrap_or("").to_string();

        let badge = row
            .select(&badge_sel)
            .next()
            .map(|b| b.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let date_range = row
            .select(&date_sel)
            .next()
            .map(|d| d.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        ipos.push(DashboardIpo {
            company_name,
            href,
            badge,
            date_range,
            row_class: class.to_string(),
        });
    }

    Ok(ipos)
}

fn badge_to_status(badge: &str, row_class: &str) -> IpoStatus {
    match badge {
        "O" | "CT" => IpoStatus::Open,
        "P" => IpoStatus::Upcoming,
        "LT" => IpoStatus::Listed,
        _ => {
            if row_class == "color-green" {
                IpoStatus::Open
            } else if row_class == "color-lightyellow" {
                IpoStatus::Upcoming
            } else if row_class == "color-aqua" {
                IpoStatus::Listed
            } else {
                IpoStatus::Closed
            }
        }
    }
}

fn parse_indian_number(s: &str) -> Option<i64> {
    let cleaned: String = s.chars().filter(|&c| c.is_ascii_digit()).collect();
    cleaned.parse::<i64>().ok()
}

fn parse_crore_to_decimal(s: &str) -> Option<Decimal> {
    let cleaned: String = s
        .chars()
        .filter(|&c| c.is_ascii_digit() || c == '.')
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    let value: f64 = cleaned.parse().ok()?;
    Decimal::from_f64_retain(value * 10_000_000.0)
}

fn parse_price_band(s: &str) -> (Option<Decimal>, Option<Decimal>) {
    let cleaned = s.replace("₹", "").replace("&#8377;", "").replace(',', "");
    if let Some(pos) = cleaned.find("to") {
        let left = cleaned[..pos].trim();
        let right = cleaned[pos + 2..].trim();
        let low = left.parse::<f64>().ok().and_then(Decimal::from_f64_retain);
        let high = right.parse::<f64>().ok().and_then(Decimal::from_f64_retain);
        (low, high)
    } else {
        (None, None)
    }
}

fn parse_lot_size(s: &str) -> Option<i64> {
    let cleaned: String = s.chars().filter(|&c| c.is_ascii_digit()).collect();
    cleaned.parse::<i64>().ok()
}

fn current_year() -> String {
    format!("{}", jiff::Zoned::now().datetime().year())
}

fn has_month(s: &str) -> bool {
    s.split_whitespace().any(|w| {
        w.trim_end_matches(',').len() == 3
            && w.chars().all(|c| c.is_ascii_alphabetic())
            && w.to_lowercase() != "the"
    })
}

fn parse_date_from_range(s: &str) -> (Option<Date>, Option<Date>) {
    let s = s.trim();
    let parts: Vec<&str> = if s.contains("to") {
        s.splitn(2, "to").collect()
    } else if s.contains(" - ") {
        s.splitn(2, " - ").collect()
    } else {
        return (None, None);
    };
    if parts.len() != 2 {
        return (None, None);
    }
    let start_part = parts[0].trim().trim_end_matches(',');
    let end_part_raw = parts[1].trim().trim_end_matches(',');

    let year = extract_year(end_part_raw)
        .or_else(|| extract_year(start_part))
        .map(|s| s.to_string())
        .unwrap_or_else(current_year);

    let end_part = strip_year(end_part_raw);
    let start_part_clean = strip_year(start_part);

    let start_date = if has_month(&start_part_clean) {
        Some(format!("{} {year}", start_part_clean.trim()))
    } else {
        let month = end_part_raw
            .split_whitespace()
            .nth(1)
            .map(|s| s.trim_end_matches(','));
        match month {
            Some(m) if !has_month(&start_part_clean) => {
                Some(format!("{} {} {year}", start_part_clean.trim(), m))
            }
            _ => Some(format!("{} {year}", start_part_clean.trim())),
        }
    };

    let end_date = Some(format!("{} {year}", end_part.trim()));

    (start_date, end_date)
}

fn strip_year(s: &str) -> String {
    s.split_whitespace()
        .filter(|w| !(w.trim_end_matches(',').len() == 4
            && w.trim_end_matches(',').chars().all(|c| c.is_ascii_digit())))
        .map(|w| w.trim_end_matches(','))
        .collect::<Vec<_>>()
        .join(" ")
}

fn extract_year(s: &str) -> Option<&str> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    parts.last().filter(|p| p.len() == 4 && p.chars().all(|c| c.is_ascii_digit())).copied()
}

fn now_iso() -> DateTime {
    jiff::Zoned::now().to_string()
}

struct DetailTable {
    key: String,
    value: String,
}

fn parse_detail_tables(html: &str) -> Result<Vec<DetailTable>, ScrapeError> {
    let doc = Html::parse_document(html);
    let table_sel =
        Selector::parse("table.table.table-hover.w-100.my-0").map_err(|e| ScrapeError::Parse(e.to_string()))?;
    let tr_sel = Selector::parse("tr").map_err(|e| ScrapeError::Parse(e.to_string()))?;
    let td_sel = Selector::parse("td").map_err(|e| ScrapeError::Parse(e.to_string()))?;

    let mut rows = Vec::new();

    for table in doc.select(&table_sel) {
        for tr in table.select(&tr_sel) {
            let cells: Vec<_> = tr.select(&td_sel).collect();
            if cells.len() == 2 {
                let key = cells[0].text().collect::<String>().trim().to_string();
                let value = cells[1].text().collect::<String>().trim().to_string();
                if !key.is_empty() {
                    rows.push(DetailTable { key, value });
                }
            }
        }
    }

    Ok(rows)
}

fn parse_timetable(html: &str) -> HashMap<String, Date> {
    let doc = Html::parse_document(html);
    let mut dates = HashMap::new();

    let ul_sel =
        Selector::parse("ul.top-ratios").expect("valid selector");
    let li_sel =
        Selector::parse("li.d-flex.justify-content-between.ms-2").expect("valid selector");

    if let Some(ul) = doc.select(&ul_sel).next() {
        for li in ul.select(&li_sel) {
            let spans: Vec<_> = li.text().collect::<Vec<_>>();
            if spans.len() >= 2 {
                let label = spans[0].trim().to_string();
                let date_val = spans[1].trim().to_string();
                if !date_val.is_empty() {
                    dates.insert(label, date_val);
                }
            }
        }
    }

    dates
}

fn find_in_rows<'a>(rows: &'a [DetailTable], key: &str) -> Option<&'a str> {
    rows.iter()
        .find(|r| r.key.contains(key))
        .map(|r| r.value.as_str())
}

fn extract_detail_urls(html: &str) -> (Option<String>, Option<String>) {
    let doc = Html::parse_document(html);
    let rhp_sel = Selector::parse("a[title=\"RHP\"], a[title=\"rhp\"]").ok();
    let drhp_sel = Selector::parse("a[title=\"DRHP\"], a[title=\"drhp\"]").ok();

    let rhp = rhp_sel
        .and_then(|sel| doc.select(&sel).next())
        .and_then(|a| a.value().attr("href"))
        .map(|s| s.to_string());

    let drhp = drhp_sel
        .and_then(|sel| doc.select(&sel).next())
        .and_then(|a| a.value().attr("href"))
        .map(|s| s.to_string());

    (rhp, drhp)
}

fn parse_bidding_details_json(html: &str) -> Result<Vec<SubscriptionEntry>, ScrapeError> {
    let needle = "subscriptionDataResponse\\\":{";
    let pos = html
        .find(needle)
        .ok_or_else(|| ScrapeError::Parse("subscriptionDataResponse not found".into()))?;

    let obj_start = pos + needle.len() - 1;

    let bytes = html.as_bytes();
    let mut depth = 0u32;
    let mut end = obj_start;
    let mut in_string = false;

    for i in obj_start..bytes.len() {
        let c = bytes[i];
        if c == b'\\' {
            continue;
        }
        if c == b'"' && !in_string {
            in_string = true;
            continue;
        }
        if c == b'"' && in_string {
            in_string = false;
            continue;
        }
        if !in_string {
            match c {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    if depth != 0 {
        return Err(ScrapeError::Parse("Unmatched braces in subscriptionDataResponse".into()));
    }

    let raw = &html[obj_start..end];
    let unescaped = raw.replace("\\\"", "\"");

    let v: serde_json::Value = serde_json::from_str(&unescaped)
        .map_err(|e| ScrapeError::Parse(format!("JSON parse error: {e} in subscription data")))?;

    let details = v
        .get("ipoBiddingDetails")
        .and_then(|d| d.as_array())
        .ok_or_else(|| ScrapeError::Parse("ipoBiddingDetails array not found in subscription data".into()))?;

    if details.is_empty() {
        return Ok(Vec::new());
    }

    let entry = &details[0];
    let snapshot_at = entry
        .get("date_added")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(now_iso);

    let categories: &[(&str, &str)] = &[
        ("QIB", "qib"),
        ("NII", "nii"),
        ("NII (Big)", "nii_big"),
        ("NII (Small)", "nii_small"),
        ("RII", "rii"),
        ("EMP", "emp"),
        ("Total", "total"),
    ];

    let mut results = Vec::new();
    let ipo_id = entry.get("ipo_id").and_then(|v| v.as_i64()).unwrap_or(0);

    for (category, key) in categories {
        let subscribed = entry.get(*key).and_then(|v| match v {
            serde_json::Value::Number(n) => n.as_f64().and_then(Decimal::from_f64_retain),
            serde_json::Value::String(s) => s.parse::<f64>().ok().and_then(Decimal::from_f64_retain),
            _ => None,
        });

        results.push(SubscriptionEntry {
            id: None,
            ipo_id,
            snapshot_at: snapshot_at.clone(),
            category: category.to_string(),
            subscribed,
            source: "chittorgarh".to_string(),
            ingested_at: now_iso(),
        });
    }

    Ok(results)
}

fn detail_url_from_href(href: &str) -> String {
    format!("{}{}", BASE_URL, href)
}

impl IpoScraper for ChittorgarhScraper {
    fn market_id(&self) -> &str {
        "in"
    }

    fn fetch_ipos(&self) -> Result<Vec<Ipo>, ScrapeError> {
        let dashboard_html = self.fetch_html(&Self::dashboard_url())?;
        let summaries = parse_dashboard(&dashboard_html)?;

        if summaries.is_empty() {
            return Err(ScrapeError::Parse("No IPOs found on dashboard".into()));
        }

        let mut ipos = Vec::new();
        let now = now_iso();

        for summary in &summaries {
            let url = detail_url_from_href(&summary.href);
            log::info!("Fetching IPO detail: {}", summary.company_name);

            let html = self.fetch_html(&url)?;
            std::thread::sleep(self.request_delay);
            self.detail_cache
                .borrow_mut()
                .insert(summary.company_name.clone(), (html.clone(), url.clone()));

            let details = parse_detail_tables(&html).unwrap_or_default();
            let timetable = parse_timetable(&html);
            let (drhp_url, rhp_url) = extract_detail_urls(&html);

            let status = badge_to_status(&summary.badge, &summary.row_class);
            let (open_date, close_date) = parse_date_from_range(&summary.date_range);

            let price_band = find_in_rows(&details, "Price Band");
            let (price_low, price_high) = price_band
                .map(parse_price_band)
                .unwrap_or((None, None));

            let lot_size_str = find_in_rows(&details, "Lot Size");
            let lot_size = lot_size_str.and_then(parse_lot_size);

            let listing_at = find_in_rows(&details, "Listing At").map(|s| s.to_string());
            let offer_type = find_in_rows(&details, "Sale Type").map(|s| s.to_string());
            let _issue_type = find_in_rows(&details, "Issue Type").map(|s| s.to_string());

            let issue_size_str = find_in_rows(&details, "Total Issue Size");
            let issue_size = issue_size_str.and_then(parse_crore_to_decimal);

            let fresh_str = find_in_rows(&details, "Fresh Issue");
            let fresh_shares = fresh_str.and_then(parse_indian_number);

            let ofs_str = find_in_rows(&details, "Offer for Sale");
            let ofs_shares = ofs_str.and_then(parse_indian_number);

            let post_str = find_in_rows(&details, "Share Holding Post Issue");
            let post_shares = post_str.and_then(parse_indian_number);

            let listing_date_detail = find_in_rows(&details, "Listing Date")
                .map(|s| s.rsplitn(2, ',').take(2).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(","))
                .map(|s| s.trim().to_string());

            let listing_date = listing_date_detail.filter(|s| !s.is_empty());

            let allotment_date = timetable.get("Allotment").cloned();
            let open_date = timetable.get("IPO Open").cloned().or(open_date);
            let close_date = timetable.get("IPO Close").cloned().or(close_date);
            let listing_date = listing_date.or_else(|| timetable.get("Listing").cloned());

            let exchange_str = listing_at.clone().unwrap_or_default();
            let exchange = exchange_str.split(',').next().map(|s| s.trim().to_string());

            let ipo = Ipo {
                id: None,
                market_id: "in".to_string(),
                company_name: summary.company_name.clone(),
                symbol: None,
                exchange,
                sector: None,
                offer_type,
                price_band_low: price_low,
                price_band_high: price_high,
                final_price: None,
                lot_size,
                shares_offered: None,
                fresh_issue_shares: fresh_shares,
                ofs_shares,
                shares_outstanding_post: post_shares,
                issue_size,
                open_date,
                close_date,
                allotment_date,
                listing_date,
                status,
                drhp_url,
                rhp_url,
                source: "chittorgarh".to_string(),
                ingested_at: now.clone(),
                updated_at: now.clone(),
            };

            ipos.push(ipo);
        }

        Ok(ipos)
    }

    fn fetch_subscriptions(&self, ipo: &Ipo) -> Result<Vec<SubscriptionEntry>, ScrapeError> {
        let cached = {
            let cache = self.detail_cache.borrow();
            cache.get(&ipo.company_name).cloned()
        };

        match cached {
            Some((html, _url)) => parse_bidding_details_json(&html),
            None => {
                log::info!("Cache miss, fetching detail page for subscriptions: {}", ipo.company_name);
                let url = format!("{}/ipo/{}/", BASE_URL, ipo.company_name.to_lowercase().replace(' ', "-"));
                let html = self.fetch_html(&url)?;
                self.detail_cache
                    .borrow_mut()
                    .insert(ipo.company_name.clone(), (html.clone(), url));
                parse_bidding_details_json(&html)
            }
        }
    }

    fn fetch_price_history(&self, _ticker: &str, _market: &str) -> Result<Vec<PricePoint>, ScrapeError> {
        Err(ScrapeError::Parse(
            "Price history not available from Chittorgarh. Use BSE/NSE or Yahoo Finance source.".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DASHBOARD_HTML: &str = include_str!("test_fixtures/chittorgarh_dashboard.html");
    const DETAIL_HTML: &str = include_str!("test_fixtures/chittorgarh_ipo_detail.html");

    #[test]
    fn test_parse_dashboard_extracts_ipos() {
        let ipos = parse_dashboard(DASHBOARD_HTML).unwrap();
        assert!(!ipos.is_empty(), "should extract at least one IPO");
        assert!(ipos.iter().any(|i| i.company_name.contains("Knack")), "should find Knack Packaging");
    }

    #[test]
    fn test_parse_dashboard_includes_status_badge() {
        let ipos = parse_dashboard(DASHBOARD_HTML).unwrap();
        let knack = ipos.iter().find(|i| i.company_name.contains("Knack")).unwrap();
        assert_eq!(knack.badge, "O", "Knack should be Open");
        assert_eq!(knack.row_class, "color-green");
    }

    #[test]
    fn test_parse_dashboard_includes_date_range() {
        let ipos = parse_dashboard(DASHBOARD_HTML).unwrap();
        let knack = ipos.iter().find(|i| i.company_name.contains("Knack")).unwrap();
        assert!(knack.date_range.contains("Jul"), "should have date range with Jul");
    }

    #[test]
    fn test_badge_to_status_mapping() {
        assert_eq!(badge_to_status("O", ""), IpoStatus::Open);
        assert_eq!(badge_to_status("CT", ""), IpoStatus::Open);
        assert_eq!(badge_to_status("P", ""), IpoStatus::Upcoming);
        assert_eq!(badge_to_status("LT", ""), IpoStatus::Listed);
        assert_eq!(badge_to_status("", "color-green"), IpoStatus::Open);
        assert_eq!(badge_to_status("", "color-lightyellow"), IpoStatus::Upcoming);
        assert_eq!(badge_to_status("", "color-aqua"), IpoStatus::Listed);
    }

    #[test]
    fn test_parse_indian_number() {
        assert_eq!(parse_indian_number("2,58,52,941"), Some(25852941));
        assert_eq!(parse_indian_number("88 Shares"), Some(88));
        assert_eq!(parse_indian_number(""), None);
    }

    #[test]
    fn test_parse_crore_to_decimal() {
        let val = parse_crore_to_decimal("₹439 Cr").unwrap();
        assert_eq!(val.to_string(), "4390000000");
        let val2 = parse_crore_to_decimal("₹60.50 Cr").unwrap();
        assert_eq!(val2.to_string(), "605000000");
    }

    #[test]
    fn test_parse_price_band() {
        let (low, high) = parse_price_band("₹161 to ₹170");
        assert_eq!(low.unwrap().to_string(), "161");
        assert_eq!(high.unwrap().to_string(), "170");
    }

    #[test]
    fn test_parse_price_band_with_entity() {
        let (low, high) = parse_price_band("&#8377;161 to &#8377;170");
        assert_eq!(low.unwrap().to_string(), "161");
        assert_eq!(high.unwrap().to_string(), "170");
    }

    #[test]
    fn test_parse_lot_size() {
        assert_eq!(parse_lot_size("88 Shares"), Some(88));
        assert_eq!(parse_lot_size("100"), Some(100));
    }

    #[test]
    fn test_parse_detail_tables_extracts_ipo_details() {
        let rows = parse_detail_tables(DETAIL_HTML).unwrap();
        assert!(rows.iter().any(|r| r.key.contains("Price Band")), "should find Price Band");
        assert!(rows.iter().any(|r| r.key.contains("Lot Size")), "should find Lot Size");
        assert!(rows.iter().any(|r| r.key.contains("Listing At")), "should find Listing At");
    }

    #[test]
    fn test_detail_tables_price_band_value() {
        let rows = parse_detail_tables(DETAIL_HTML).unwrap();
        let price_band = find_in_rows(&rows, "Price Band").unwrap();
        assert!(price_band.contains("161"), "Price band should contain 161");
        assert!(price_band.contains("170"), "Price band should contain 170");
    }

    #[test]
    fn test_detail_tables_lot_size_value() {
        let rows = parse_detail_tables(DETAIL_HTML).unwrap();
        let lot = find_in_rows(&rows, "Lot Size").unwrap();
        assert!(lot.contains("88"), "Lot size should be 88");
    }

    #[test]
    fn test_detail_tables_issue_size() {
        let rows = parse_detail_tables(DETAIL_HTML).unwrap();
        let total = find_in_rows(&rows, "Total Issue Size").unwrap();
        assert!(total.contains("439"), "Issue size should mention 439 Cr");
    }

    #[test]
    fn test_parse_bidding_details_json() {
        let entries = parse_bidding_details_json(DETAIL_HTML).unwrap();
        assert!(!entries.is_empty(), "should extract subscription entries");

        let qib = entries.iter().find(|e| e.category == "QIB").unwrap();
        assert!(qib.subscribed.is_some(), "QIB subscription should be present");

        let total = entries.iter().find(|e| e.category == "Total").unwrap();
        assert!(total.subscribed.unwrap() > Decimal::ZERO, "Total subscription should be > 0");
    }

    #[test]
    fn test_parse_bidding_details_has_categories() {
        let entries = parse_bidding_details_json(DETAIL_HTML).unwrap();
        let categories: Vec<_> = entries.iter().map(|e| e.category.as_str()).collect();
        assert!(categories.contains(&"QIB"));
        assert!(categories.contains(&"NII"));
        assert!(categories.contains(&"RII"));
        assert!(categories.contains(&"Total"));
    }

    #[test]
    fn test_bidding_details_source_and_ipo_id() {
        let entries = parse_bidding_details_json(DETAIL_HTML).unwrap();
        for entry in &entries {
            assert_eq!(entry.source, "chittorgarh");
            assert_eq!(entry.ipo_id, 2592);
        }
    }

    #[test]
    fn test_now_iso_format() {
        let ts = now_iso();
        assert!(ts.contains('T'), "ISO timestamp should contain T separator");
    }

    #[test]
    fn test_parse_date_from_range() {
        let (start, end) = parse_date_from_range("01 - 03 Jul");
        assert_eq!(start, Some("01 Jul 2026".to_string()));
        assert_eq!(end, Some("03 Jul 2026".to_string()));

        let (start, end) = parse_date_from_range("01 - 03 Jul, 2026");
        assert_eq!(start, Some("01 Jul 2026".to_string()));
        assert_eq!(end, Some("03 Jul 2026".to_string()));

        let (start, end) = parse_date_from_range("29 Jun - 01 Jul");
        assert_eq!(start, Some("29 Jun 2026".to_string()), "cross-month start should keep its own month");
        assert_eq!(end, Some("01 Jul 2026".to_string()), "cross-month end should use its own month");

        let (start, end) = parse_date_from_range("05 - 07 Jun, 2026");
        assert_eq!(start, Some("05 Jun 2026".to_string()), "should get month from end part");
        assert_eq!(end, Some("07 Jun 2026".to_string()));
    }

    #[test]
    fn test_extract_year() {
        assert_eq!(extract_year("03 Jul, 2026"), Some("2026"));
        assert_eq!(extract_year("2026"), Some("2026"));
        assert_eq!(extract_year("Jul"), None);
    }

    #[test]
    fn test_parse_subscriptions_empty_on_no_needle() {
        let result = parse_bidding_details_json("<html>no data here</html>");
        assert!(result.is_err());
    }
}
