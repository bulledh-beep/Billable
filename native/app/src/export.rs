//! PDF and CSV exports (ipc.ts's export handlers, pdf.ts, tax-pdf.ts,
//! commission-pdf.ts). Pages call these; each renders or builds the file,
//! asks where to save with the Mac's save panel like the app, then shows the
//! toast the React page showed after the call.
//!
//! Dev/test hook, as in Electron: `BILLABLE_EXPORT_DIR=<dir>` writes straight
//! into that folder without asking (and, like Electron, skips the extra
//! steps after a save).

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use billable_core::{csv::csv_text, InvoicePatch};
use gpui::{App, AsyncApp, Window};
use gpui_component::Root;

use crate::{
    components::{toast_error, toast_success},
    platform::pdf::render_a4,
    shell::{BillableApp, Route},
    store,
};

fn export_dir() -> Option<PathBuf> {
    std::env::var_os("BILLABLE_EXPORT_DIR").filter(|d| !d.is_empty()).map(PathBuf::from)
}

/// `app.getPath('downloads')`
fn downloads() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/")).join("Downloads")
}

/// The save panel. An empty `dir` leaves the folder to macOS (its last-used
/// one), as Electron did for a bare file name.
async fn ask_save_path(dir: PathBuf, name: String, cx: &mut AsyncApp) -> Result<Option<PathBuf>> {
    let rx = cx.update(|cx| cx.prompt_for_new_path(&dir, Some(&name)));
    rx.await.context("The save panel closed unexpectedly")?
}

/// The route on screen in `window` (to tell which page asked for an export).
fn current_route(window: &Window, cx: &App) -> Option<Route> {
    let root = window.root::<Root>()??;
    let app = root.read(cx).view().clone().downcast::<BillableApp>().ok()?;
    Some(app.read(cx).route().clone())
}

/// Render `html`, then save it the way the generators did: straight into
/// `BILLABLE_EXPORT_DIR`, or where the save panel says (starting in
/// Downloads). `Ok(None)` when the panel was cancelled; the bool says
/// whether it went to the dev folder.
async fn render_and_save(html: String, file_name: String, cx: &mut AsyncApp) -> Result<Option<(PathBuf, Vec<u8>, bool)>> {
    let task = cx.update(|cx| render_a4(html, cx));
    let pdf = task.await?;
    if let Some(dir) = export_dir() {
        let out = dir.join(&file_name);
        std::fs::write(&out, &pdf).with_context(|| format!("Couldn't write {}", out.display()))?;
        return Ok(Some((out, pdf, true)));
    }
    let Some(path) = ask_save_path(downloads(), file_name, cx).await? else { return Ok(None) };
    std::fs::write(&path, &pdf).with_context(|| format!("Couldn't write {}", path.display()))?;
    Ok(Some((path, pdf, false)))
}

fn open(path: &Path, cx: &mut AsyncApp) {
    let path = path.to_path_buf();
    cx.update(|cx| cx.open_with_system(&path));
}

/// `invoices:export-pdf` (pdf.ts `generateInvoicePDF`): also keeps a copy in
/// `<userData>/invoices`, records the saved path on the invoice and opens it.
pub fn invoice_pdf(invoice_id: i64, window: &mut Window, cx: &mut App) {
    // InvoiceDetail.tsx and Billing.tsx toasted differently
    let from_detail = current_route(window, cx) == Some(Route::Invoice(invoice_id));
    let error = |e: anyhow::Error, cx: &mut App| toast_error(format!("Couldn't create the PDF: {e:#}"), cx);
    let (invoice, doc) = match store::db(cx).invoice_pdf_doc(invoice_id) {
        Ok(d) => d,
        Err(e) => return error(e, cx),
    };
    let number = invoice.invoice_number.clone();
    let user_data = store::store(cx).dir.clone();
    cx.spawn(async move |cx| {
        let result: Result<bool> = async {
            let Some((path, pdf, dev)) = render_and_save(doc.html, doc.file_name, cx).await? else { return Ok(false) };
            if dev {
                return Ok(true);
            }
            let pdf_dir = user_data.join("invoices");
            std::fs::create_dir_all(&pdf_dir).with_context(|| format!("Couldn't create {}", pdf_dir.display()))?;
            let internal = pdf_dir.join(format!("{number}.pdf"));
            if path != internal {
                std::fs::write(&internal, &pdf).with_context(|| format!("Couldn't write {}", internal.display()))?;
            }
            let saved = path.to_string_lossy().into_owned();
            cx.update(|cx| {
                store::write(cx, |db| db.update_invoice(invoice_id, &InvoicePatch { pdf_path: Some(Some(saved)), ..Default::default() }))
            })?;
            open(&path, cx);
            Ok(true)
        }
        .await;
        cx.update(|cx| match result {
            Ok(true) if from_detail => toast_success("PDF saved", cx),
            Ok(true) => toast_success(format!("{number} saved as PDF"), cx),
            Ok(false) => {}
            Err(e) => error(e, cx),
        });
    })
    .detach();
}

/// `commission-invoices:export-pdf` (commission-pdf.ts): save, then open.
pub fn commission_invoice_pdf(commission_invoice_id: i64, _window: &mut Window, cx: &mut App) {
    let error = |e: anyhow::Error, cx: &mut App| toast_error(format!("Export failed: {e:#}"), cx);
    let doc = match store::db(cx).commission_invoice_pdf_doc(commission_invoice_id) {
        Ok(d) => d,
        Err(e) => return error(e, cx),
    };
    cx.spawn(async move |cx| {
        let result = save_and_open(doc.html, doc.file_name, cx).await;
        cx.update(|cx| match result {
            Ok(true) => toast_success("Invoice PDF saved", cx),
            Ok(false) => {}
            Err(e) => error(e, cx),
        });
    })
    .detach();
}

/// `tax:export-summary-pdf` (tax-pdf.ts): save, then open.
pub fn tax_summary_pdf(tax_year: i64, _window: &mut Window, cx: &mut App) {
    let error = |e: anyhow::Error, cx: &mut App| toast_error(format!("Failed: {e:#}"), cx);
    let doc = match store::db(cx).tax_summary_pdf_doc(tax_year) {
        Ok(d) => d,
        Err(e) => return error(e, cx),
    };
    cx.spawn(async move |cx| {
        let result = save_and_open(doc.html, doc.file_name, cx).await;
        // TaxOverview.tsx: no toast when the PDF save was cancelled
        cx.update(|cx| match result {
            Ok(true) => toast_success("Exported", cx),
            Ok(false) => {}
            Err(e) => error(e, cx),
        });
    })
    .detach();
}

/// Render, save and (unless it went to the dev folder) open. `Ok(false)` if cancelled.
async fn save_and_open(html: String, file_name: String, cx: &mut AsyncApp) -> Result<bool> {
    let Some((path, _, dev)) = render_and_save(html, file_name, cx).await? else { return Ok(false) };
    if !dev {
        open(&path, cx);
    }
    Ok(true)
}

/// Ask where to save `text` (into `BILLABLE_EXPORT_DIR` when set) and write it.
/// `Ok(None)` when cancelled.
async fn save_text(text: String, file_name: String, cx: &mut AsyncApp) -> Result<Option<PathBuf>> {
    let path = match export_dir() {
        Some(dir) => dir.join(&file_name),
        None => match ask_save_path(PathBuf::new(), file_name, cx).await? {
            Some(p) => p,
            None => return Ok(None),
        },
    };
    std::fs::write(&path, text).with_context(|| format!("Couldn't write {}", path.display()))?;
    Ok(Some(path))
}

/// `reports:export-csv`: `headers` then one row per record, quoted like the app.
pub fn csv(headers: Vec<String>, rows: Vec<Vec<String>>, filename: &str, _window: &mut Window, cx: &mut App) {
    // Reports.tsx, Billing.tsx and Commissions.tsx each had their own toast
    let success = if filename == "commissions.csv" {
        "Exported commissions.csv"
    } else if filename.starts_with("invoices") {
        "Invoices exported"
    } else {
        "CSV exported"
    };
    let text = csv_text(&headers, &rows);
    let filename = filename.to_string();
    cx.spawn(async move |cx| {
        let result = save_text(text, filename, cx).await;
        cx.update(|cx| match result {
            Ok(Some(_)) => toast_success(success, cx),
            Ok(None) => {}
            Err(e) => toast_error(format!("{e:#}"), cx),
        });
    })
    .detach();
}

/// `tax:export-invoices-csv`
pub fn invoices_csv(tax_year: i64, _window: &mut Window, cx: &mut App) {
    let data = store::db(cx).invoices_csv(tax_year);
    tax_csv(data, format!("invoices-{tax_year}.csv"), cx);
}

/// `tax:export-expenses-csv`
pub fn expenses_csv(tax_year: i64, _window: &mut Window, cx: &mut App) {
    let data = store::db(cx).expenses_csv(tax_year);
    tax_csv(data, format!("expenses-{tax_year}.csv"), cx);
}

/// TaxOverview.tsx's `handleExport` for the CSVs: "Exported", and (as the
/// app did) "No data to export" whenever nothing was written, cancelling
/// included.
fn tax_csv(data: Result<Option<String>>, file_name: String, cx: &mut App) {
    let text = match data {
        Ok(Some(t)) => t,
        Ok(None) => return toast_error("No data to export", cx),
        Err(e) => return toast_error(format!("Failed: {e:#}"), cx),
    };
    cx.spawn(async move |cx| {
        let result = save_text(text, file_name, cx).await;
        cx.update(|cx| match result {
            Ok(Some(_)) => toast_success("Exported", cx),
            Ok(None) => toast_error("No data to export", cx),
            Err(e) => toast_error(format!("Failed: {e:#}"), cx),
        });
    })
    .detach();
}
