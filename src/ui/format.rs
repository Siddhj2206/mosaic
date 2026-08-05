//! Indian-market number formatting: ₹ with Indian digit grouping, crores,
//! dates, times-subscribed, percentages.

use jiff::civil::Date;
use rust_decimal::Decimal;

/// Indian grouping: 12345678 → "1,23,45,678".
fn indian_grouping(digits: &str) -> String {
    let bytes = digits.as_bytes();
    if bytes.len() <= 3 {
        return digits.to_string();
    }
    let last3 = &digits[digits.len() - 3..];
    let rest = &digits[..digits.len() - 3];
    let mut groups: Vec<String> = rest.chars().rev().collect::<Vec<_>>().chunks(2).map(|c| c.iter().rev().collect::<String>()).collect();
    groups.reverse();
    let mut out = groups.join(",");
    if !out.is_empty() {
        out.push(',');
    }
    out.push_str(last3);
    out
}

/// ₹ with Indian grouping, up to 2 decimals (trimmed).
pub fn rupees(d: Decimal) -> String {
    let rounded = d.round_dp(2);
    let s = rounded.to_string();
    let (int_part, frac_part) = match s.split_once('.') {
        Some((i, f)) => (i.to_string(), Some(f.to_string())),
        None => (s.clone(), None),
    };
    let grouped = indian_grouping(&int_part);
    match frac_part {
        Some(f) if f != "00" => format!("₹{grouped}.{f}"),
        _ => format!("₹{grouped}"),
    }
}

/// Amount in ₹ crore: "₹3,066.89 Cr".
pub fn crores(d: Decimal) -> String {
    format!("{} Cr", rupees(d))
}

/// Integer with Indian grouping.
pub fn int(n: i64) -> String {
    indian_grouping(&n.to_string())
}

/// Shares count like "15,72,33,715".
pub fn shares(n: i64) -> String {
    int(n)
}

/// Date → "5 Aug 2026".
pub fn date(d: Date) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!("{} {} {}", d.day(), MONTHS[(d.month() - 1) as usize], d.year())
}

/// Times subscribed: "12.34×".
pub fn times(d: Decimal) -> String {
    format!("{}×", d.round_dp(2))
}

/// Signed percent with sign: "+8.5%".
pub fn signed_pct(d: Decimal) -> String {
    if d >= Decimal::ZERO {
        format!("+{}%", d.round_dp(2))
    } else {
        format!("{}%", d.round_dp(2))
    }
}

/// Percent change between two prices, as Decimal (not yet scaled by 100).
pub fn pct_change(from: Decimal, to: Decimal) -> Option<Decimal> {
    if from.is_zero() {
        return None;
    }
    Some(((to - from) / from) * Decimal::from(100))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouping_is_indian() {
        assert_eq!(indian_grouping("157233715"), "15,72,33,715");
        assert_eq!(indian_grouping("999"), "999");
        assert_eq!(indian_grouping("12345678"), "1,23,45,678");
    }

    #[test]
    fn rupees_formats() {
        assert_eq!(rupees(Decimal::from(50)), "₹50");
        assert_eq!(rupees(Decimal::from(306689)), "₹3,06,689");
        assert_eq!(
            rupees(Decimal::from_str_exact("3066.89").unwrap()),
            "₹3,066.89"
        );
    }
}
