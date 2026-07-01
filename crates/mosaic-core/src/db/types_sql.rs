use rust_decimal::Decimal;

pub fn decimal_to_f64_opt(d: Option<Decimal>) -> Option<f64> {
    d.map(|v| v.as_f64())
}

pub fn f64_to_decimal_opt(v: Option<f64>) -> Option<Decimal> {
    v.and_then(Decimal::from_f64_retain)
}

pub fn f64_to_decimal(v: f64) -> Option<Decimal> {
    Decimal::from_f64_retain(v)
}
