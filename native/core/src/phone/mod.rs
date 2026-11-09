//! Phone sync with Content HQ: the pure logic of `phone-snapshot.ts` and
//! `phone-actions.ts`, plus the `phone_actions` bookkeeping from
//! `phone-sync.ts` (applying a batch, acknowledgements, the activity list).
//! The network loop lives in the app (`services/phone.rs`).
//!
//! The JSON is a contract with Content HQ's Zod schemas
//! (`src/lib/billable/contract.ts`, version 1): field names, nullability and
//! limits match what the Electron app sends. Numbers serialize as JavaScript
//! prints them (`100`, not `100.0`), and lengths are counted in UTF-16 units
//! like `String.length`.

pub mod actions;
pub mod snapshot;

pub use actions::{
    apply_action, apply_incoming, list_activity, mark_acked, unacked_acks, Ack, ActionResult, Activity, AppliedSummary,
    IncomingAction,
};
pub use snapshot::{
    build_snapshot, iso_local, payment_method_names, snapshot_currency, ExpenseCategory, Snapshot, EXPENSE_CATEGORIES,
};

use serde::{Serialize, Serializer};

/// A number that serializes like `JSON.stringify`: integral values without
/// a fraction, `NaN`/`Infinity` as `null`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JsNum(pub f64);

impl Serialize for JsNum {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let x = self.0;
        if !x.is_finite() {
            s.serialize_none()
        } else if x.fract() == 0.0 && x.abs() < 9_007_199_254_740_992.0 {
            s.serialize_i64(x as i64)
        } else {
            s.serialize_f64(x)
        }
    }
}

/// `s.length` (UTF-16 code units).
pub fn len16(s: &str) -> usize {
    s.encode_utf16().count()
}

/// `s.slice(0, n)` by UTF-16 code units. Where JS would split a surrogate
/// pair (leaving a lone half Rust strings can't hold) the pair is dropped,
/// so the result is never longer than JS's.
pub fn slice16(s: &str, n: usize) -> &str {
    let mut units = 0;
    for (i, c) in s.char_indices() {
        units += c.len_utf16();
        if units > n {
            return &s[..i];
        }
    }
    s
}

/// `/^\d{4}-\d{2}-\d{2}$/`
pub(crate) fn is_ymd(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_print_like_json_stringify() {
        let j = |x: f64| serde_json::to_string(&JsNum(x)).unwrap();
        assert_eq!(j(100.0), "100");
        assert_eq!(j(-0.0), "0");
        assert_eq!(j(12.5), "12.5");
        assert_eq!(j(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(j(f64::NAN), "null");
    }

    #[test]
    fn utf16_slicing() {
        assert_eq!(slice16("abc", 2), "ab");
        assert_eq!(len16("a😀"), 3);
        assert_eq!(slice16("a😀b", 2), "a");
        assert_eq!(slice16("a😀b", 3), "a😀");
        assert!(is_ymd("2026-01-05") && !is_ymd("2026-1-05") && !is_ymd("2026-01-05T"));
    }
}
