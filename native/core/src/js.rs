//! JavaScript semantics the Electron code relies on, reproduced so the Rust
//! port computes the same numbers and strings (`Math.round`, `parseInt`,
//! `String(n)`, truthiness, `trim`).

use serde_json::Value;

/// `Math.round`: rounds half toward +∞ (so `-2.5` → `-2`, `2.5` → `3`).
pub fn js_round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let f = x.floor();
    if x - f >= 0.5 {
        f + 1.0
    } else {
        f
    }
}

/// `Math.round(x * 100) / 100`, the app's money/minutes rounding.
pub fn round2(x: f64) -> f64 {
    js_round(x * 100.0) / 100.0
}

/// `Math.round(x)` as an integer, `None` when JS would produce `NaN`/`Infinity`.
pub fn js_round_i64(x: f64) -> Option<i64> {
    let r = js_round(x);
    r.is_finite().then_some(r as i64)
}

/// `String(n)` / template-literal formatting of a JS number.
pub fn js_number_to_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if x == 0.0 {
        return "0".into();
    }
    // Rust's `{:e}` gives the shortest round-trip digits, same as JS.
    let e = format!("{:e}", x.abs());
    let (mant, exp) = e.split_once('e').unwrap_or((&e, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exp + 1;
    let body = if k <= n && n <= 21 {
        format!("{}{}", digits, "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{}", "0".repeat((-n) as usize), digits)
    } else {
        let sign = if n - 1 < 0 { '-' } else { '+' };
        if k == 1 {
            format!("{}e{}{}", digits, sign, (n - 1).abs())
        } else {
            format!("{}.{}e{}{}", &digits[..1], &digits[1..], sign, (n - 1).abs())
        }
    };
    if x < 0.0 {
        format!("-{body}")
    } else {
        body
    }
}

/// JS `WhiteSpace` and `LineTerminator` (what `String.prototype.trim` strips).
pub fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{9}' | '\u{a}' | '\u{b}' | '\u{c}' | '\u{d}' | ' ' | '\u{a0}' | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}

/// `String.prototype.trim`.
pub fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_whitespace)
}

/// `parseInt(s)` (no radix): leading whitespace, sign, `0x` hex, decimal digits.
pub fn js_parse_int(s: &str) -> f64 {
    let s = s.trim_start_matches(is_js_whitespace);
    let (neg, s) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let (radix, s) = if s.starts_with("0x") || s.starts_with("0X") { (16, &s[2..]) } else { (10, s) };
    let digits: String = s.chars().take_while(|c| c.is_digit(radix)).collect();
    if digits.is_empty() {
        return f64::NAN;
    }
    let mut v = 0f64;
    for c in digits.chars() {
        v = v * radix as f64 + c.to_digit(radix).unwrap_or(0) as f64;
    }
    if neg {
        -v
    } else {
        v
    }
}

/// Length of the longest `StrDecimalLiteral` prefix (`1.5e3`, `.5`, `Infinity`).
fn decimal_prefix_len(s: &str) -> usize {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    if s[i..].starts_with("Infinity") {
        return i + 8;
    }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut mantissa_digits = i - int_start;
    if i < b.len() && b[i] == b'.' {
        let frac_start = i + 1;
        let mut j = frac_start;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if mantissa_digits > 0 || j > frac_start {
            mantissa_digits += j - frac_start;
            i = j;
        }
    }
    if mantissa_digits == 0 {
        return 0;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let exp_start = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp_start {
            i = j;
        }
    }
    i
}

fn parse_decimal(lit: &str) -> f64 {
    let unsigned = lit.trim_start_matches(['+', '-']);
    let v = if unsigned == "Infinity" {
        f64::INFINITY
    } else {
        let t = unsigned.strip_suffix('.').unwrap_or(unsigned);
        t.parse::<f64>().unwrap_or(f64::NAN)
    };
    if lit.starts_with('-') {
        -v
    } else {
        v
    }
}

/// `parseFloat(s)`.
pub fn js_parse_float(s: &str) -> f64 {
    let s = s.trim_start_matches(is_js_whitespace);
    let n = decimal_prefix_len(s);
    if n == 0 {
        return f64::NAN;
    }
    parse_decimal(&s[..n])
}

/// `Number(s)` for a string: whole-string numeric literal, empty → 0, otherwise `NaN`.
pub fn js_string_to_number(s: &str) -> f64 {
    let s = js_trim(s);
    if s.is_empty() {
        return 0.0;
    }
    for (prefix, radix) in [("0x", 16), ("0X", 16), ("0o", 8), ("0O", 8), ("0b", 2), ("0B", 2)] {
        if let Some(rest) = s.strip_prefix(prefix) {
            if rest.is_empty() || !rest.chars().all(|c| c.is_digit(radix)) {
                return f64::NAN;
            }
            return rest.chars().fold(0f64, |v, c| v * radix as f64 + c.to_digit(radix).unwrap_or(0) as f64);
        }
    }
    if decimal_prefix_len(s) == s.len() {
        parse_decimal(s)
    } else {
        f64::NAN
    }
}

/// JS truthiness of a JSON value.
pub fn js_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0 && !f.is_nan()).unwrap_or(false),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// `String(value)` for a JSON value (what `updateSettings` stores).
pub fn js_string(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => js_number_to_string(n.as_f64().unwrap_or(f64::NAN)),
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .map(|i| if i.is_null() { String::new() } else { js_string(i) })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}

/// `String(s).slice(0, n)` (by UTF-16 code units, approximated by chars).
pub fn js_slice(s: &str, n: usize) -> &str {
    match s.char_indices().nth(n) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// Truthiness of an optional string (`null`/`''` are falsy).
pub fn str_truthy(s: Option<&str>) -> bool {
    matches!(s, Some(v) if !v.is_empty())
}

/// Small base64 encoder (standard alphabet, padded), used for avatars.
pub fn base64_encode(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_matches_math_round() {
        assert_eq!(js_round(2.5), 3.0);
        assert_eq!(js_round(-2.5), -2.0);
        assert_eq!(js_round(0.49999999999999994), 0.0);
        assert_eq!(round2(1.005), 1.0); // 100.49999999999999 → 100
        assert_eq!(round2(411.665), 411.67);
    }

    #[test]
    fn number_formatting_matches_js() {
        assert_eq!(js_number_to_string(1001.0), "1001");
        assert_eq!(js_number_to_string(1e21), "1e+21");
        assert_eq!(js_number_to_string(1e-7), "1e-7");
        assert_eq!(js_number_to_string(0.000001), "0.000001");
        assert_eq!(js_number_to_string(123.456), "123.456");
        assert_eq!(js_number_to_string(-0.0), "0");
        assert_eq!(js_number_to_string(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(js_number_to_string(-1.5), "-1.5");
        assert_eq!(js_number_to_string(1.2345e25), "1.2345e+25");
    }

    #[test]
    fn parsing_matches_js() {
        assert_eq!(js_parse_int("0x1A"), 26.0);
        assert_eq!(js_parse_int(" 12ab"), 12.0);
        assert!(js_parse_int("abc").is_nan());
        assert_eq!(js_parse_float("1e3x"), 1000.0);
        assert_eq!(js_parse_float(".5"), 0.5);
        assert_eq!(js_parse_float("100"), 100.0);
        assert!(js_parse_float("").is_nan());
        assert_eq!(js_string_to_number(" 12 "), 12.0);
        assert_eq!(js_string_to_number("0x10"), 16.0);
        assert_eq!(js_string_to_number("5."), 5.0);
        assert_eq!(js_string_to_number(""), 0.0);
        assert!(js_string_to_number("-").is_nan());
        assert!(js_string_to_number("5a").is_nan());
    }

    #[test]
    fn string_conversion_matches_js() {
        assert_eq!(js_string(&serde_json::json!(null)), "null");
        assert_eq!(js_string(&serde_json::json!(100)), "100");
        assert_eq!(js_string(&serde_json::json!(1.5)), "1.5");
        assert_eq!(js_string(&serde_json::json!([1, null, "a"])), "1,,a");
        assert_eq!(js_string(&serde_json::json!({"a": 1})), "[object Object]");
    }

    #[test]
    fn base64_works() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }
}
