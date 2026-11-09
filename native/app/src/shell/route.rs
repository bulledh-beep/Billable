//! Routes, using the Electron app's own paths so the menu bar, tray and
//! `billable://` links can keep speaking in paths ("/billing?status=sent").

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Route {
    Dashboard,
    Clients { action_new: bool },
    Client(i64),
    Projects { action_new: bool },
    Project(i64),
    Time { action_new: bool },
    Billing { status: Option<String> },
    Invoice(i64),
    InvoiceNew { client_id: Option<i64>, project_id: Option<i64>, project_ids: Vec<i64> },
    InvoiceEdit(i64),
    Reports,
    Commissions,
    TaxOverview,
    TaxSettings,
    Settings,
    Content,
}

/// The sidebar entry a route belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    Dashboard,
    Clients,
    Projects,
    Time,
    Billing,
    Reports,
    Commissions,
    TaxOverview,
    TaxSettings,
    Settings,
    Content,
}

fn query(q: &str, key: &str) -> Option<String> {
    q.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        (k == key).then(|| v.to_string())
    })
}

impl Route {
    pub fn parse(path: &str) -> Route {
        let (p, q) = path.split_once('?').unwrap_or((path, ""));
        let parts: Vec<&str> = p.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
        let id = |s: &str| s.parse::<i64>().ok();
        let new = query(q, "action").as_deref() == Some("new");
        match parts.as_slice() {
            [] => Route::Dashboard,
            ["clients"] => Route::Clients { action_new: new },
            ["clients", x] => id(x).map(Route::Client).unwrap_or(Route::Clients { action_new: false }),
            ["projects"] => Route::Projects { action_new: new },
            ["projects", x] => id(x).map(Route::Project).unwrap_or(Route::Projects { action_new: false }),
            ["time"] => Route::Time { action_new: new },
            ["billing"] | ["invoices"] => Route::Billing { status: query(q, "status").filter(|s| !s.is_empty()) },
            ["invoices", "new"] => Route::InvoiceNew {
                client_id: query(q, "client_id").and_then(|v| v.parse().ok()),
                project_id: query(q, "project_id").and_then(|v| v.parse().ok()),
                project_ids: query(q, "project_ids").map(|v| v.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default(),
            },
            ["invoices", x, "edit"] => id(x).map(Route::InvoiceEdit).unwrap_or(Route::Billing { status: None }),
            ["invoices", x] => id(x).map(Route::Invoice).unwrap_or(Route::Billing { status: None }),
            ["reports"] => Route::Reports,
            ["commissions"] => Route::Commissions,
            ["tax-overview"] => Route::TaxOverview,
            ["tax-settings"] => Route::TaxSettings,
            ["settings"] => Route::Settings,
            ["content"] => Route::Content,
            _ => Route::Dashboard,
        }
    }

    pub fn path(&self) -> String {
        match self {
            Route::Dashboard => "/".into(),
            Route::Clients { action_new } => if *action_new { "/clients?action=new".into() } else { "/clients".into() },
            Route::Client(id) => format!("/clients/{id}"),
            Route::Projects { action_new } => if *action_new { "/projects?action=new".into() } else { "/projects".into() },
            Route::Project(id) => format!("/projects/{id}"),
            Route::Time { action_new } => if *action_new { "/time?action=new".into() } else { "/time".into() },
            Route::Billing { status } => match status {
                Some(s) => format!("/billing?status={s}"),
                None => "/billing".into(),
            },
            Route::Invoice(id) => format!("/invoices/{id}"),
            Route::InvoiceNew { client_id, project_id, project_ids } => {
                let mut q = Vec::new();
                if let Some(c) = client_id {
                    q.push(format!("client_id={c}"));
                }
                if let Some(p) = project_id {
                    q.push(format!("project_id={p}"));
                }
                if !project_ids.is_empty() {
                    q.push(format!("project_ids={}", project_ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",")));
                }
                if q.is_empty() { "/invoices/new".into() } else { format!("/invoices/new?{}", q.join("&")) }
            }
            Route::InvoiceEdit(id) => format!("/invoices/{id}/edit"),
            Route::Reports => "/reports".into(),
            Route::Commissions => "/commissions".into(),
            Route::TaxOverview => "/tax-overview".into(),
            Route::TaxSettings => "/tax-settings".into(),
            Route::Settings => "/settings".into(),
            Route::Content => "/content".into(),
        }
    }

    pub fn section(&self) -> Section {
        match self {
            Route::Dashboard => Section::Dashboard,
            Route::Clients { .. } | Route::Client(_) => Section::Clients,
            Route::Projects { .. } | Route::Project(_) => Section::Projects,
            Route::Time { .. } => Section::Time,
            Route::Billing { .. } | Route::Invoice(_) | Route::InvoiceNew { .. } | Route::InvoiceEdit(_) => Section::Billing,
            Route::Reports => Section::Reports,
            Route::Commissions => Section::Commissions,
            Route::TaxOverview => Section::TaxOverview,
            Route::TaxSettings => Section::TaxSettings,
            Route::Settings => Section::Settings,
            Route::Content => Section::Content,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for p in [
            "/",
            "/clients",
            "/clients?action=new",
            "/clients/4",
            "/projects/2",
            "/time?action=new",
            "/billing?status=overdue",
            "/invoices/7",
            "/invoices/7/edit",
            "/invoices/new?client_id=3",
            "/invoices/new?project_ids=1,2",
            "/tax-overview",
        ] {
            assert_eq!(Route::parse(p).path(), p);
        }
        assert_eq!(Route::parse("/invoices"), Route::Billing { status: None });
    }
}
