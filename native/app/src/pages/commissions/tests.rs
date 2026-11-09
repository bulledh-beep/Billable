//! The payout rules, and the writes the screen makes (add, quick and bulk
//! status changes, invoice create / mark paid / delete). The data test only
//! runs against a scratch copy: set BILLABLE_USER_DATA to a folder under a
//! `scratchpad` directory.

use billable_core::{
    db::Db,
    models::{CommissionInput, CommissionInvoiceInput, CommissionPatch, CommissionStatusPatch},
    profiles::{user_data_dir, Profiles},
    time::Clock,
};

use super::model::*;

#[test]
fn number_inputs() {
    for ok in ["6", "6.16", "-1", ".5", "1e3", "25000", "0"] {
        assert!(valid_number(ok), "{ok}");
    }
    for bad in ["", "6.", "-", "+6", "1e", "6.1.2", "abc", "1,000"] {
        assert!(!valid_number(bad), "{bad}");
    }
    assert!(number_chars("12.5e-3") && !number_chars("12a"));
}

#[test]
fn preview() {
    assert_eq!(preview_commission("solar", "6.16", "", ""), (Some(308.0), false));
    assert_eq!(preview_commission("solar", "", "", ""), (None, false));
    assert_eq!(preview_commission("roofing", "", "20000", ""), (Some(250.0), false));
    assert_eq!(preview_commission("roofing", "", "30000", ""), (Some(500.0), false));
    assert_eq!(preview_commission("roofing", "", "25000", ""), (None, true));
    assert_eq!(preview_commission("roofing", "", "25000", "400"), (Some(400.0), false));
    assert_eq!(preview_commission("roofing", "", "", ""), (None, false));
}

#[test]
fn writes_against_scratch_copy() {
    let dir = user_data_dir();
    if !dir.to_string_lossy().contains("scratchpad") {
        eprintln!("skipped: BILLABLE_USER_DATA isn't a scratch copy");
        return;
    }
    let profiles = Profiles::new(dir, Clock::system());
    let db = Db::open_profile(&profiles, &profiles.get_active_profile_id(), Clock::system()).unwrap();
    let today = today_iso();

    // Add job (as the form sends it)
    let add = |name: &str, job_type: &str, status: &str, kw: Option<f64>, contract: Option<f64>, manual: Option<f64>| {
        db.create_commission(&CommissionInput {
            client_name: name.into(),
            job_type: Some(job_type.into()),
            appointment_date: Some(today.clone()),
            closer_name: Some("Sam".into()),
            status: Some(status.into()),
            payment_status: Some("unpaid".into()),
            system_size_kw: kw,
            contract_amount: contract,
            manual_override: manual,
            notes: Some(String::new()),
        })
        .unwrap()
    };
    let solar = add("Test Solar", "solar", "closed_waiting", Some(6.16), None, None);
    assert_eq!(solar.calculated_commission, 308.0);
    assert!(is_owed(&solar) && payout_info(&solar) == ("Owed", Some(308.0), Tone::Owed));
    let roof = add("Test Roof", "roofing", "closed_waiting", None, Some(25000.0), None);
    assert_eq!(payout_info(&roof), ("Needs review", None, Tone::Review));
    let roof_big = add("Test Roof Big", "roofing", "appointment_set", None, Some(32000.0), None);
    assert!(is_active(&roof_big) && effective(&roof_big) == Some(500.0));

    // Edit: add an override to the review job
    let patch = CommissionPatch {
        client_name: Some("Test Roof".into()),
        job_type: Some("roofing".into()),
        appointment_date: Some(Some(today.clone())),
        closer_name: Some(Some("Sam".into())),
        status: Some("closed_waiting".into()),
        payment_status: Some("unpaid".into()),
        system_size_kw: Some(None),
        contract_amount: Some(Some(25000.0)),
        manual_override: Some(Some(375.0)),
        notes: Some(Some("override".into())),
    };
    let roof = db.update_commission(roof.id, &patch).unwrap().unwrap();
    assert_eq!(payout_info(&roof), ("Owed", Some(375.0), Tone::Owed));

    // Quick "Mark closed", then bulk "Mark closed"
    let closed = CommissionStatusPatch { status: Some(Some("closed_waiting".into())), ..Default::default() };
    let roof_big = db.patch_commission(roof_big.id, &closed).unwrap().unwrap();
    assert!(is_owed(&roof_big));
    db.bulk_patch_commissions(&[roof_big.id], &closed).unwrap();

    // Invoice the three closed jobs
    let created = db
        .create_commission_invoice(&CommissionInvoiceInput {
            job_ids: vec![solar.id, roof.id, roof_big.id],
            category: Some("both".into()),
            date_from: None,
            date_to: None,
            notes: None,
        })
        .unwrap()
        .unwrap();
    assert_eq!((created.category.as_str(), created.job_count, created.total), ("mixed", 3, 308.0 + 375.0 + 500.0));
    let jobs = db.list_commissions().unwrap();
    assert!(jobs.iter().filter(|c| c.invoice_id == Some(created.id)).all(is_invoiced));

    // Quick "Paid" on an invoiced job also marks its invoice line paid
    let job = jobs.iter().find(|c| c.id == solar.id).unwrap();
    let paid = CommissionStatusPatch {
        payment_status: Some(Some("paid".into())),
        status: Some(Some("paid".into())),
        paid_at: Some(Some(today.clone())),
        invoice_status: Some(if job.invoice_status.as_deref() == Some("invoiced") { Some("paid".into()) } else { job.invoice_status.clone() }),
        ..Default::default()
    };
    let job = db.patch_commission(solar.id, &paid).unwrap().unwrap();
    assert!(is_paid(&job) && job.invoice_status.as_deref() == Some("paid"));

    // Delete the invoice: unpaid jobs go back to Owed, the paid one stays paid
    db.delete_commission_invoice(created.id).unwrap();
    let jobs = db.list_commissions().unwrap();
    assert!(is_owed(jobs.iter().find(|c| c.id == roof.id).unwrap()));
    assert!(is_paid(jobs.iter().find(|c| c.id == solar.id).unwrap()));

    // Invoice again and mark the invoice paid: every job on it is paid
    let again = db
        .create_commission_invoice(&CommissionInvoiceInput { job_ids: vec![roof.id, roof_big.id], category: Some("roofing".into()), ..Default::default() })
        .unwrap()
        .unwrap();
    db.update_commission_invoice_status(again.id, "paid").unwrap();
    let detail = db.get_commission_invoice(again.id).unwrap().unwrap();
    assert_eq!(detail.status, "paid");
    assert!(detail.jobs.iter().all(is_paid));

    // Leave an owed job, an active one and an open invoice for the screen
    add("Rivera Family", "solar", "closed_waiting", Some(8.4), None, None);
    add("Nguyen Residence", "roofing", "appointment_attended", None, Some(18000.0), None);
    let owed = add("Patel Home", "solar", "closed_waiting", Some(5.0), None, None);
    db.create_commission_invoice(&CommissionInvoiceInput { job_ids: vec![owed.id], category: Some("both".into()), ..Default::default() }).unwrap();
    add("Lost Deal", "solar", "lost", Some(4.0), None, None);
}
