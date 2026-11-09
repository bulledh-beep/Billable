//! HTML → A4 PDF with the system WebKit, standing in for Electron's
//! `webContents.printToPDF({ pageSize: 'A4', printBackground: true, margins: 0 })`.
//!
//! Chromium lays a print page out at 96 CSS px per inch (an A4 page is
//! 794.56 × 1123.84 px, drawn at 0.75 pt/px onto 595.92 × 842.88 pt), while
//! WebKit's printing lays out at 1.25 px per point of paper and draws at 0.8.
//! So WebKit prints onto paper 1/0.9375 the size of Chromium's A4, which gives
//! it the same CSS page box (same line breaks and page breaks), and each page
//! is then drawn at 0.9375 into the A4 page Chromium produces.
//!
//! Everything runs on the main thread without blocking it: the page loads
//! and prints asynchronously while a GPUI task polls for completion.

use std::{
    cell::Cell,
    ffi::c_void,
    path::{Path, PathBuf},
    ptr::NonNull,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Context as _, Result};
use gpui::{App, Task};
use objc2::{define_class, msg_send, rc::Retained, runtime::{AnyObject, Bool, NSObject, NSObjectProtocol, ProtocolObject}, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSBackingStoreType, NSPrintInfo, NSPrintJobSavingURL, NSPrintOperation, NSPrintSaveJob, NSWindow, NSWindowStyleMask};
use objc2_core_foundation::{CFRetained, CFURL, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGContext, CGPDFContextClose, CGPDFContextCreateWithURL, CGPDFDocument};
use objc2_foundation::{NSError, NSPoint, NSRect, NSSize, NSString, NSURL};
use objc2_web_kit::{WKNavigation, WKNavigationDelegate, WKWebView, WKWebViewConfiguration};

/// Chromium's A4 page in points (what Electron's PDFs measure).
const A4: (f64, f64) = (595.92, 842.88);
/// Chromium's 0.75 pt/px over WebKit's 0.8.
const FIT: f64 = 0.9375;
/// Electron's hidden BrowserWindow was 800 × 1130.
const VIEW: (f64, f64) = (800.0, 1130.0);
/// After the load finishes, let layout and fonts settle (Electron slept
/// 600–1000 ms; WebKit has laid out by the time it reports the load done).
const SETTLE: Duration = Duration::from_millis(250);
const LOAD_TIMEOUT: Duration = Duration::from_secs(20);
const PRINT_TIMEOUT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(15);

#[derive(Default)]
struct State {
    /// `Some(Ok)` when the page finished loading, `Some(Err)` when it failed.
    loaded: Cell<Option<std::result::Result<(), String>>>,
    /// Whether the print operation succeeded, once it has run.
    printed: Cell<Option<bool>>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; the methods below
    // match the selectors' signatures.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BillablePdfRenderDelegate"]
    #[ivars = State]
    struct RenderDelegate;

    unsafe impl NSObjectProtocol for RenderDelegate {}

    unsafe impl WKNavigationDelegate for RenderDelegate {
        #[unsafe(method(webView:didFinishNavigation:))]
        fn did_finish(&self, _web_view: &WKWebView, _navigation: Option<&WKNavigation>) {
            self.ivars().loaded.set(Some(Ok(())));
        }

        #[unsafe(method(webView:didFailNavigation:withError:))]
        fn did_fail(&self, _web_view: &WKWebView, _navigation: Option<&WKNavigation>, error: &NSError) {
            self.ivars().loaded.set(Some(Err(error.localizedDescription().to_string())));
        }

        #[unsafe(method(webView:didFailProvisionalNavigation:withError:))]
        fn did_fail_provisional(&self, _web_view: &WKWebView, _navigation: Option<&WKNavigation>, error: &NSError) {
            self.ivars().loaded.set(Some(Err(error.localizedDescription().to_string())));
        }
    }

    impl RenderDelegate {
        /// NSPrintOperation's `didRunSelector`.
        #[unsafe(method(printOperationDidRun:success:contextInfo:))]
        fn print_operation_did_run(&self, _op: &NSPrintOperation, success: Bool, _context: *mut c_void) {
            self.ivars().printed.set(Some(success.as_bool()));
        }
    }
);

impl RenderDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(State::default());
        unsafe { msg_send![super(this), init] }
    }
}

/// A temporary file that's removed when dropped.
struct TempFile(PathBuf);

impl TempFile {
    fn new(tag: &str) -> TempFile {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        TempFile(std::env::temp_dir().join(format!("billable-{}-{n}-{tag}.pdf", std::process::id())))
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Render `html` to the PDF Electron's `printToPDF` made: A4, no margins,
/// backgrounds printed. Resolves with the file's bytes.
pub fn render_a4(html: String, cx: &mut App) -> Task<Result<Vec<u8>>> {
    cx.spawn(async move |cx| {
        let mtm = MainThreadMarker::new().ok_or_else(|| anyhow!("PDF rendering must start on the main thread"))?;
        let delegate = RenderDelegate::new(mtm);

        // A hidden web view in an offscreen window, like Electron's `show: false` window
        let config = unsafe { WKWebViewConfiguration::new(mtm) };
        unsafe { config.preferences().setShouldPrintBackgrounds(true) };
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(VIEW.0, VIEW.1));
        let web = unsafe { WKWebView::initWithFrame_configuration(WKWebView::alloc(mtm), frame, &config) };
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                NSRect::new(NSPoint::new(-20000.0, -20000.0), NSSize::new(VIEW.0, VIEW.1)),
                NSWindowStyleMask::Borderless,
                NSBackingStoreType::Buffered,
                true,
            )
        };
        unsafe { window.setReleasedWhenClosed(false) };
        window.setContentView(Some(&web));
        unsafe {
            web.setNavigationDelegate(Some(ProtocolObject::from_ref(&*delegate)));
            web.loadHTMLString_baseURL(&NSString::from_str(&html), None);
        }

        let result = async {
            let started = Instant::now();
            loop {
                cx.background_executor().timer(POLL).await;
                if let Some(loaded) = delegate.ivars().loaded.take() {
                    loaded.map_err(|e| anyhow!("Couldn't load the page: {e}"))?;
                    break;
                }
                if started.elapsed() > LOAD_TIMEOUT {
                    bail!("The page took too long to load");
                }
            }
            cx.background_executor().timer(SETTLE).await;

            let raw = TempFile::new("print");
            print_to_file(&web, &window, &delegate, &raw.0)?;
            let started = Instant::now();
            let success = loop {
                cx.background_executor().timer(POLL).await;
                if let Some(ok) = delegate.ivars().printed.take() {
                    break ok;
                }
                if started.elapsed() > PRINT_TIMEOUT {
                    bail!("Printing took too long");
                }
            };
            if !success {
                bail!("Printing failed");
            }
            let fitted = TempFile::new("a4");
            fit_to_a4(&raw.0, &fitted.0)?;
            std::fs::read(&fitted.0).context("Couldn't read the PDF")
        }
        .await;

        unsafe {
            web.stopLoading();
            web.setNavigationDelegate(None);
        }
        window.setContentView(None);
        result
    })
}

/// Start WebKit printing into `out` (Chromium's A4 enlarged by 1/FIT, no
/// margins, no panels); `delegate` hears when it's done.
fn print_to_file(web: &WKWebView, window: &NSWindow, delegate: &RenderDelegate, out: &Path) -> Result<()> {
    let url = NSURL::from_file_path(out).context("Bad temporary path")?;
    let info = NSPrintInfo::new();
    info.setPaperSize(NSSize::new(A4.0 / FIT, A4.1 / FIT));
    info.setTopMargin(0.0);
    info.setBottomMargin(0.0);
    info.setLeftMargin(0.0);
    info.setRightMargin(0.0);
    info.setHorizontallyCentered(false);
    info.setVerticallyCentered(false);
    info.setScalingFactor(1.0);
    unsafe {
        info.setJobDisposition(NSPrintSaveJob);
        info.dictionary().insert(NSPrintJobSavingURL, &*url as &AnyObject);
    }
    let op = unsafe { web.printOperationWithPrintInfo(&info) };
    op.setShowsPrintPanel(false);
    op.setShowsProgressPanel(false);
    // WebKit's printing view starts out zero-sized; it needs the web view's size
    if let Some(view) = op.view() {
        view.setFrame(web.frame());
    }
    unsafe {
        op.runOperationModalForWindow_delegate_didRunSelector_contextInfo(
            window,
            Some(delegate as &AnyObject),
            Some(sel!(printOperationDidRun:success:contextInfo:)),
            std::ptr::null_mut(),
        );
    }
    Ok(())
}

/// Draw each page of `src` at FIT onto an A4 page of `dst`.
fn fit_to_a4(src: &Path, dst: &Path) -> Result<()> {
    let src_url: CFRetained<CFURL> = CFURL::from_file_path(src).context("Bad temporary path")?;
    let dst_url: CFRetained<CFURL> = CFURL::from_file_path(dst).context("Bad temporary path")?;
    let doc = CGPDFDocument::with_url(Some(&src_url)).context("WebKit didn't produce a PDF")?;
    let pages = CGPDFDocument::number_of_pages(Some(&doc));
    if pages == 0 {
        bail!("WebKit produced an empty PDF");
    }
    let media = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(A4.0, A4.1));
    let ctx = unsafe { CGPDFContextCreateWithURL(Some(&dst_url), &media, None) }.context("Couldn't create the PDF")?;
    for i in 1..=pages {
        let Some(page) = CGPDFDocument::page(Some(&doc), i) else { continue };
        unsafe { CGContext::begin_page(Some(&ctx), NonNull::from(&media).as_ptr()) };
        CGContext::save_g_state(Some(&ctx));
        CGContext::scale_ctm(Some(&ctx), FIT, FIT);
        CGContext::draw_pdf_page(Some(&ctx), Some(&page));
        CGContext::restore_g_state(Some(&ctx));
        CGContext::end_page(Some(&ctx));
    }
    CGPDFContextClose(Some(&ctx));
    Ok(())
}
