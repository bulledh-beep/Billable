use super::*;

/// `Intl.NumberFormat` output recorded from Node (ICU 78) for both locales
/// the generators use: `[locale, currency, value, formatted]`.
#[test]
fn intl_currency_matches_icu() {
    let cases: Vec<(String, String, f64, String)> = serde_json::from_str(include_str!("intl_fixture.json")).unwrap();
    assert!(cases.len() > 100);
    for (locale, currency, value, expected) in cases {
        let loc = if locale == "en-CA" { Locale::EnCa } else { Locale::EnUs };
        assert_eq!(intl_currency(value, loc, &currency), expected, "{locale} {currency} {value}");
    }
}

#[test]
fn to_fixed_matches_js() {
    for (x, s) in [
        (1.005, "1.00"),
        (0.125, "0.13"),
        (2.675, "2.67"),
        (1.5, "1.50"),
        (-1.5, "-1.50"),
        (-0.001, "-0.00"),
        (10.235, "10.23"),
        (1e-7, "0.00"),
        (123.456, "123.46"),
        (0.0, "0.00"),
    ] {
        assert_eq!(to_fixed(x, 2), s, "{x}");
    }
}

#[test]
fn format_day_matches_js() {
    for (input, out) in [
        ("2026-05-07", "May 7, 2026"),
        ("2026-13-01", "undefined 1, 2026"),
        ("", ""),
        ("2026-05", "2026-05"),
        ("2026-5-7T10:00", "2026-5-7T10:00"),
        ("abc", "abc"),
        ("2026-00-05", "2026-00-05"),
        ("2026-01-1.5", "Jan 1, 2026"),
    ] {
        assert_eq!(format_day(input), out, "{input:?}");
    }
}

#[test]
fn escapes_like_escape_html() {
    assert_eq!(esc(r#"<a href="x">Tom & Jerry's</a>"#), "&lt;a href=&quot;x&quot;&gt;Tom &amp; Jerry&#039;s&lt;/a&gt;");
}

fn temp_db() -> (std::path::PathBuf, Db) {
    let dir = std::env::temp_dir().join(format!("billable-pdf-test-{}-{}", std::process::id(), rand_suffix()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = Db::open(dir.join("billable.db"), crate::Clock::fixed_at("2026-11-09T18:00:00.000Z").unwrap()).unwrap();
    (dir, db)
}

fn rand_suffix() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}

#[test]
fn documents_render_from_a_fresh_database() {
    let (dir, db) = temp_db();
    assert!(db.invoice_pdf_doc(999).unwrap_err().to_string().contains("Invoice not found"));
    assert!(db.commission_invoice_pdf_doc(999).unwrap_err().to_string().contains("Commission invoice not found"));
    let tax = db.tax_summary_pdf_doc(2026).unwrap();
    assert_eq!(tax.file_name, "tax-summary-2026.pdf");
    assert!(tax.html.contains("For the 2026 tax year"));
    assert!(tax.html.contains("No expenses logged for 2026."));
    assert!(tax.html.contains("Prepared Nov 9, 2026"));
    assert!(tax.html.ends_with("</html>"));
    drop(db);
    let _ = std::fs::remove_dir_all(dir);
}
