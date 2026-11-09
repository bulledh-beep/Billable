# Billable native port: plan and checklist

Goal: a native Rust (GPUI) Billable that matches the Electron app pixel for
pixel and does everything it does, using the same data files.

## Ground rules

- **Same data.** Reads and writes the same SQLite files at
  `~/Library/Application Support/Billable/profiles/<id>/billable.db`, with the
  same schema and migrations, so either app can open the other's data.
- **Dev never touches real data.** Development runs with
  `BILLABLE_USER_DATA="$HOME/Library/Application Support/Billable Dev"` (a copy
  made 2026-11-08, without the phone-sync token). Tests work on temp copies.
- **Match the original exactly.** The Electron source in `../src` is the
  spec. Sizes come from `index.css` and `tailwind.config.js`. Tailwind spacing
  is rem-based and Billable's rem is **13px**, so one spacing unit is 3.25px
  (`r()` in the app). Arbitrary values like `h-[58px]` stay in px.
- **Check against the original.** Each page is compared side by side with the
  Electron app on the same data before it counts as done.

## Layout

- `core/`: `billable-core`, the data layer and business rules (no UI). Ports
  `src/main/database.ts`, `profiles.ts`, `timer-manager.ts`, `phone-*.ts`, PDF
  HTML, CSV and updater logic. Tested for parity against the TypeScript.
- `app/`: `billable`, the GPUI app (shell, pages, components, platform glue).

## Known GPUI workarounds

- SF Rounded: the system font is variable. We cut 500/600/700 copies at
  launch, one family per weight ("Billable Rounded 600"), because Core Text
  reports every copy as weight 400 (`app/src/fonts.rs`).
- GPUI 0.2.2 swapped red and blue in SVG images; gpui-pre 0.3.11 fixed it (no workaround).
- No letter-spacing, no transforms on divs, no inset shadows.

## Checklist

### Core (data and rules)
- [x] Schema, migrations, backfills (`initDatabase`, `runMigrations`, `createTables`, `seedDefaults`)
- [x] Clients: list/get/create/update/delete/merge, rate cascade
- [x] Projects: list/get/create/update/delete with money aggregates
- [x] Time entries: list/get/create/update/delete, set billable, invoiceable
- [x] Timer: start/stop/pause/resume, rounding, under-a-minute discard, restore
- [x] Invoices: list/get/create/update/delete, mark sent/paid/unpaid, numbering, items, entry links
- [x] Billing overview + attention + dismiss
- [x] Dashboard stats, recent entries, reports (by project/client/month)
- [x] Settings get/update, export/import database
- [x] Tax settings, tax overview, invoices by year, expenses CRUD
- [x] Commissions + commission invoices
- [x] Profiles: list/create/rename/color/delete/switch/avatar, legacy migration
- [x] CSV exports (reports, invoices, expenses)
- [x] PDF HTML (invoice, tax summary, commission invoice)
- [x] Phone snapshot + actions
- [x] Parity tests against the TypeScript

### App shell
- [x] Window: size, min size, inset traffic lights, title
- [x] Sidebar with nav pill animation, badge, wordmark easter egg, profile switcher
- [x] Toolbar: page title/subtitle/back, page actions, TimerControl (idle/running/paused, project picker)
- [x] Navigation (routes, back, scroll to top, rise-in)
- [x] Theme: dark / light / auto
- [x] Toasts (react-hot-toast look), mascot toasts
- [x] App menu (menu.ts), keyboard shortcuts (⌘, etc.)
- [x] Mascot party, burst, update banner, What's New

### Pages
- [x] Dashboard
- [x] Clients, Client detail, Client form, Merge clients
- [x] Projects, Project detail, Project form, Close-out
- [x] Time (week view, day groups, filters, entry editor)
- [x] Billing (pipeline, attention, lists, bulk actions, record payment)
- [x] Invoice detail (paper preview, stamp, actions)
- [x] Invoice create/edit
- [x] Reports (presets, custom range, charts, CSV)
- [x] Tax overview (chart, expenses panel, exports)
- [x] Tax settings
- [x] Commissions (jobs, invoices, bulk actions)
- [x] Settings (general, appearance, rounding, Content HQ, phone sync, data import/export, updates)

### Platform
- [x] Menu bar item (tray): title, timer controls, ready to bill
- [x] Global shortcut ⌘⇧Space
- [x] `billable://` links (switch, connect)
- [ ] Content HQ in the window (WKWebView), app switch, glide (view and switch done; glide in progress)
- [x] PDF export (invoice, tax, commission), save dialog
- [x] File dialogs, open at login
- [x] Phone sync (network, Keychain token, sleep/wake)
- [x] Updater (GitHub releases, DMG install)
- [x] Bundle, icon, Info.plist, DMG, release script

## Status 2026-11-09

Every screen and service is ported. Left: suite glide, shared-component pixel fixes (table, empty state, segmented, TextArea, modal leading, metric, date input, weight 400), a final pixel pass, Google sign-in in Content HQ against the live site, and a first run of the packaged app (scripts/bundle.sh -> release/Billable.app, Billable-<v>-arm64.dmg). Electron's localStorage prefs are imported on first launch (store.rs). Phone connections need reconnecting after the switch (Electron's token is encrypted with its own Keychain key).
