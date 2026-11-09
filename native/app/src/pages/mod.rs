//! The screens, one module per React page in src/renderer/pages.

pub mod billing;
pub mod client_detail;
pub mod clients;
pub mod commissions;
pub mod dashboard;
pub mod invoice_create;
pub mod invoice_detail;
pub mod placeholder;
pub mod project_detail;
pub mod projects;
pub mod reports;
pub mod settings;
pub mod tax_overview;
pub mod tax_settings;
pub mod time;

use gpui::{App, AppContext, Window};

use crate::shell::{app::PageHandle, route::Route};

/// Make the page for a route.
pub fn open(route: &Route, window: &mut Window, cx: &mut App) -> Box<dyn PageHandle> {
    match route {
        Route::Dashboard => Box::new(cx.new(|cx| dashboard::DashboardPage::new(window, cx))),
        Route::Clients { action_new } => Box::new(cx.new(|cx| clients::ClientsPage::new(*action_new, window, cx))),
        Route::Client(id) => Box::new(cx.new(|cx| client_detail::ClientDetailPage::new(*id, window, cx))),
        Route::Projects { action_new } => Box::new(cx.new(|cx| projects::ProjectsPage::new(*action_new, window, cx))),
        Route::Project(id) => Box::new(cx.new(|cx| project_detail::ProjectDetailPage::new(*id, window, cx))),
        Route::Time { action_new } => Box::new(cx.new(|cx| time::TimePage::new(*action_new, window, cx))),
        Route::Billing { status } => Box::new(cx.new(|cx| billing::BillingPage::new(status.clone(), window, cx))),
        Route::Invoice(id) => Box::new(cx.new(|cx| invoice_detail::InvoiceDetailPage::new(*id, window, cx))),
        Route::InvoiceNew { client_id, project_id, project_ids } => {
            Box::new(cx.new(|cx| invoice_create::InvoiceCreatePage::new(None, *client_id, *project_id, project_ids.clone(), window, cx)))
        }
        Route::InvoiceEdit(id) => Box::new(cx.new(|cx| invoice_create::InvoiceCreatePage::new(Some(*id), None, None, vec![], window, cx))),
        Route::Reports => Box::new(cx.new(|cx| reports::ReportsPage::new(window, cx))),
        Route::TaxOverview => Box::new(cx.new(|cx| tax_overview::TaxOverviewPage::new(window, cx))),
        Route::TaxSettings => Box::new(cx.new(|cx| tax_settings::TaxSettingsPage::new(window, cx))),
        Route::Commissions => Box::new(cx.new(|cx| commissions::CommissionsPage::new(window, cx))),
        Route::Settings => Box::new(cx.new(|cx| settings::SettingsPage::new(window, cx))),
        Route::Content => Box::new(cx.new(|_| placeholder::Placeholder::new(Route::Content))),
    }
}
