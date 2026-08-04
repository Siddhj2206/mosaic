//! Tolerant parsing helpers shared by the scrapers: Indian number formats,
//! NSE/Chittorgarh/IPO Watch date formats, rupees amounts.

use jiff::civil::Date;
use rust_decimal::Decimal;

use mosaic_core::Error;

/// Parse an integer with Indian digit grouping from the leading number of a
/// cell: "15,72,33,715 shares (agg. up to ₹9,275 Cr)" → 157233715.
/// Trailing amounts ("₹9,275") are ignored.
pub fn parse_int(s: &str) -> Option<i64> {
    let s = s.trim();
    let start = s.find(|c: char| c.is_ascii_digit())?;
    let mut end = start;
    for c in s[start..].chars() {
        if c.is_ascii_digit() || c == ',' {
            end += c.len_utf8();
        } else {
            break;
        }
    }
    let cleaned: String = s[start..end].chars().filter(|c| *c != ',').collect();
    if cleaned.is_empty() {
        return None;
    }
    cleaned.parse().ok()
}

/// Parse "Rs. 50/- to Rs. 53/-", "₹560 to ₹590", "₹3,066.89 Cr." into a
/// Decimal. Handles optional "Cr." / "L" / "Lakh" multipliers.
pub fn parse_rupees(s: &str) -> Option<Decimal> {
    let s = s.replace('₹', "Rs.").replace("Rs.", "Rs.");
    // Find the first number (possibly with thousands separators and decimals).
    let s = s.trim();
    let start = s.find(|c: char| c.is_ascii_digit())?;
    let mut end = start;
    for c in s[start..].chars() {
        if c.is_ascii_digit() || c == ',' || c == '.' {
            end += c.len_utf8();
        } else {
            break;
        }
    }
    let num: String = s[start..end].chars().filter(|c| *c != ',').collect();
    let mut value = Decimal::from_str_radix(&num, 10).ok()?;
    let rest = s[end..].to_ascii_lowercase();
    if rest.contains("cr") {
        value *= Decimal::from(1000_0000u64); // 1 Cr = 10^7
    } else if rest.contains("lakh") || rest.contains('l') {
        value *= Decimal::from(100_000u64); // 1 L = 10^5
    }
    Some(value)
}

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

fn month_index(name: &str) -> Option<u8> {
    let n = name.to_ascii_lowercase();
    MONTHS.iter().position(|m| n.starts_with(m)).map(|i| i as u8 + 1)
}

/// Parse "05-Aug-2026" (NSE) or "5 Aug 2026" style dates.
pub fn parse_day_month_year(s: &str) -> Option<Date> {
    let s = s.trim();
    // Strip weekday prefix: "Wed, Aug 5, 2026"
    let s = s
        .split_once(',')
        .map(|(_, rest)| rest.trim())
        .unwrap_or(s);

    let tokens: Vec<&str> = s
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.len() < 3 {
        return None;
    }
    // Try formats: [day, month, year] or [day, month, year, ...]
    let (day, month, year) = if let Ok(day) = tokens[0].parse::<i8>() {
        // day first: "5 Aug 2026" / "05-Aug-2026"
        (day, month_index(tokens[1])?, tokens[2].parse::<i16>().ok()?)
    } else {
        // month first: "Aug 5, 2026"
        let month = month_index(tokens[0])?;
        (tokens[1].parse::<i8>().ok()?, month, tokens[2].parse::<i16>().ok()?)
    };
    Date::new(year, month as i8, day).ok()
}

/// Parse a date like "Aug 5, 2026" (month first, possibly with year).
pub fn parse_month_day(s: &str) -> Option<Date> {
    let s = s.trim();
    let s = s
        .split_once(',')
        .map(|(_, rest)| rest.trim())
        .unwrap_or(s);
    let tokens: Vec<&str> = s
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.len() < 2 {
        return None;
    }
    let month = month_index(tokens[0])?;
    let day: i8 = tokens[1].parse().ok()?;
    let year: i16 = tokens.get(2).and_then(|t| t.parse().ok()).unwrap_or_else(|| {
        let now = jiff::Timestamp::now().to_zoned(jiff::tz::TimeZone::system());
        now.year()
    });
    Date::new(year, month as i8, day).ok()
}

/// Parse an issue window into (open, close): "29 to 31 Jul, 2026",
/// "10 - 12 Aug", "30-3 August", "05-Aug-2026 to 07-Aug-2026". Year-less
/// periods default to `today`'s year.
pub fn parse_period(s: &str, today: Date) -> Option<(Date, Date)> {
    let s = s.trim();

    // NSE style: "05-Aug-2026 to 07-Aug-2026"
    if let Some((a, b)) = s.split_once(" to ") {
        if let (Some(da), Some(db)) = (parse_day_month_year(a), parse_day_month_year(b)) {
            return Some((da, db));
        }
    }

    // Split on separators, keeping month tokens.
    let tokens: Vec<&str> = s
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    // Find the month token.
    let month_pos = tokens.iter().position(|t| month_index(t).is_some())?;
    let month = month_index(tokens[month_pos])?;

    let day_tokens: Vec<i8> = tokens[..month_pos]
        .iter()
        .filter_map(|t| t.parse::<i8>().ok())
        .collect();
    if day_tokens.len() < 2 {
        return None;
    }
    let year: i16 = tokens
        .get(month_pos + 1)
        .and_then(|t| t.parse().ok())
        .unwrap_or(today.year());

    let (d1, d2) = (day_tokens[0], day_tokens[1]);
    // Cross-month window: "30-3 August" = Jul 30 – Aug 3, so when day2 <
    // day1 the open falls in month-1 and the close in `month`.
    let (m1, m2) = if d2 < d1 { (month - 1, month) } else { (month, month) };
    if m1 < 1 || m2 > 12 {
        return None; // year-boundary crossings not handled; rare in IPO data
    }
    Some((
        Date::new(year, m1 as i8, d1).ok()?,
        Date::new(year, m2 as i8, d2).ok()?,
    ))
}

/// Convenience wrapper using today's date for year defaults.
pub fn parse_period_now(s: &str) -> Option<(Date, Date)> {
    let today = jiff::Timestamp::now().to_zoned(jiff::tz::TimeZone::system()).date();
    parse_period(s, today)
}

/// Parse a "Rs. 50 to Rs. 53" style price band into (low, high).
pub fn parse_band(s: &str) -> Option<(Decimal, Decimal)> {
    let nums: Vec<Decimal> = s
        .split(" to ")
        .filter_map(parse_rupees)
        .collect();
    if nums.len() >= 2 {
        Some((nums[0], nums[1]))
    } else {
        None
    }
}

/// Parse "Minimum 281 Equity shares and in multiples thereof" → (281, 281).
/// Some issues have different multiple (rare); both default to the min.
pub fn parse_lot(s: &str) -> Option<(i64, i64)> {
    let nums: Vec<i64> = s
        .split_whitespace()
        .filter_map(|t| parse_int(t))
        .collect();
    let min = *nums.first()?;
    let multiple = *nums.get(1).unwrap_or(&min);
    Some((min, multiple))
}

/// Error helper for parse failures.
pub fn parse_err(msg: impl Into<String>) -> Error {
    Error::Parse(msg.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ints_with_indian_grouping() {
        assert_eq!(parse_int("15,72,33,715"), Some(157233715));
        assert_eq!(parse_int("58422516"), Some(58422516));
        assert_eq!(parse_int("abc"), None);
    }

    #[test]
    fn rupees_parsing() {
        assert_eq!(parse_rupees("Rs. 50"), Some(Decimal::from(50)));
        assert_eq!(parse_rupees("₹560 to ₹590"), Some(Decimal::from(560)));
        assert_eq!(parse_rupees("₹3,066.89 Cr."), Some(Decimal::from_str_radix("3066.89", 10).unwrap() * Decimal::from(1000_0000u64)));
        assert_eq!(parse_rupees("₹[.]"), None);
    }

    #[test]
    fn nse_dates() {
        assert_eq!(parse_day_month_year("05-Aug-2026"), Some(Date::new(2026, 8, 5).unwrap()));
        assert_eq!(parse_day_month_year("Wed, Aug 5, 2026"), Some(Date::new(2026, 8, 5).unwrap()));
    }

    #[test]
    fn chittorgarh_periods() {
        let today = Date::new(2026, 8, 5).unwrap();
        assert_eq!(
            parse_period("29 to 31 Jul, 2026", today),
            Some((Date::new(2026, 7, 29).unwrap(), Date::new(2026, 7, 31).unwrap()))
        );
        assert_eq!(
            parse_period("05-Aug-2026 to 07-Aug-2026", today),
            Some((Date::new(2026, 8, 5).unwrap(), Date::new(2026, 8, 7).unwrap()))
        );
        // Cross-month: "30-3 August" → Jul 30 – Aug 3 (same year)
        let today = Date::new(2026, 8, 5).unwrap();
        assert_eq!(
            parse_period("30-3 August", today),
            Some((Date::new(2026, 7, 30).unwrap(), Date::new(2026, 8, 3).unwrap()))
        );
    }

    #[test]
    fn band_and_lot() {
        assert_eq!(parse_band("Rs. 50 to Rs. 53"), Some((Decimal::from(50), Decimal::from(53))));
        assert_eq!(parse_lot("Minimum 281 Equity shares and in multiples thereof"), Some((281, 281)));
    }
}
