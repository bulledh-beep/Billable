# Billable

A local-first freelance time tracker, invoice generator, and tax estimator for macOS.

[![Latest Release](https://img.shields.io/github/v/release/bulledh-beep/Billable?label=latest&color=F5A623)](https://github.com/bulledh-beep/Billable/releases/latest)
[![Download DMG](https://img.shields.io/badge/Download-DMG-F5A623?logo=apple&logoColor=white)](https://github.com/bulledh-beep/Billable/releases/latest)

> Track time. Invoice clients. Estimate your tax set-aside. All offline. All yours.

---

## Quick install

Open Terminal and paste:

```bash
curl -fsSL https://raw.githubusercontent.com/bulledh-beep/Billable/main/install.sh | bash
```

That's it. The script downloads the latest DMG, installs **Billable.app** to `/Applications`, and launches it. Your existing data (if any) is preserved — Billable stores everything in `~/Library/Application Support/billable/`, separate from the app bundle.

To install somewhere other than `/Applications`:

```bash
INSTALL_DIR=~/Applications curl -fsSL https://raw.githubusercontent.com/bulledh-beep/Billable/main/install.sh | bash
```

## Manual install

Prefer to do it yourself?

1. Download the latest `Billable-*-arm64.dmg` from [Releases](https://github.com/bulledh-beep/Billable/releases/latest)
2. Open the DMG and drag **Billable** into your Applications folder
3. Right-click the app → **Open** → **Open** (one-time bypass for Gatekeeper, since the build is unsigned)

## Updating

Once you're on **v1.0.1 or newer**, you don't need to do any of this again. The app checks GitHub Releases on launch and shows an in-app banner when a new version is available. One click downloads and installs it.

For older installs, re-run the quick-install command — it always grabs the latest version, and your data is preserved.

## Uninstall

```bash
curl -fsSL https://raw.githubusercontent.com/bulledh-beep/Billable/main/uninstall.sh | bash
```

The uninstaller removes the app bundle and asks (with a confirmation prompt) before deleting any local data.

---

## Features

### Billing you can trust
- **Every time entry knows its invoice.** Each entry shows where its money is: Unbilled, Draft, Sent, Overdue, or Paid, with the invoice number
- **Billing page** with the whole pipeline at a glance: ready to bill, drafts, awaiting payment, overdue, and paid this year
- **Needs attention** list that catches forgotten money: unbilled work older than 30 days, leftover time on completed projects, overdue invoices, stale drafts, accidental timers, and duplicate clients
- Invoices turn overdue on their own once the due date passes
- Record several payments at once with the real date the money arrived, and undo a payment recorded by mistake
- Deleting an invoice puts its time back to unbilled instead of losing it
- Menu bar shows how much is ready to bill

### Invoicing
- Build an invoice from exactly the time you tick, across all of a client's projects
- "Bill through" date to invoice up to a cut-off
- Line styles: one line per entry, per project, or per day, with editable descriptions and rates
- Marking a project complete with unbilled time asks whether to invoice it, keep it, or not bill it
- Per-invoice GST/HST toggle with provincial rate suggestions (5% GST / 13–15% HST), plus an "other tax" field
- Clean PDF export that matches the in-app preview, with a Paid tag on paid invoices
- CSV export of any filtered list

### Time tracking
- Start a timer for any project from the toolbar on every page, or from the menu bar
- Pause and resume, with the timer surviving app restarts
- Week view with daily totals, entries grouped by day, and a filter for unbilled or invoiced time
- One editor for adding and editing time, where start, end, and duration stay in sync
- Editing time that is already on a sent or paid invoice asks first
- Timers that run under a minute are discarded instead of rounded up into billable time
- Configurable rounding (none, or round up to 6 / 15 / 30 minutes)

### Clients & projects
- Money at a glance on every client and project: unbilled, outstanding, invoiced, and paid
- Per-project hourly rate override (defaults from client)
- Editing a client's default rate moves projects still on the old rate, while invoiced time keeps its rate
- Merge duplicate clients, with their projects and invoices moving over
- Project status: Active / Paused / Complete / Archived

### Tax tracking — Canadian-aware
- **Tax Settings**: business identity, province selector, GST/HST registration, fiscal year, estimated income tax bracket
- **Tax Overview**: per-year income summary, monthly bar chart of paid income, expenses by category, "Estimated to Set Aside" with both *realized* (paid invoices) and *projected* (if all invoiced gets paid) values
- **Expenses**: full CRUD, category-aware (Equipment / Software / Home Office / Phone / Travel / Meals / Pro Dev / Other), per-category 50% deductible reminder for Meals
- **Tax Summary PDF**: clean single-page summary export for handing to a bookkeeper or accountant
- Year-end **Invoice CSV** and **Expense CSV** exports

### Reports
- Time by project and by client, with billable value
- Income by month, split into invoiced and paid
- Presets for this week, month, quarter, and year, or a custom range
- CSV export

### Multiple profiles
- Run separate businesses side-by-side, each with its own clients, projects, invoices, expenses, and tax settings
- Profile picker in the sidebar — switch with one click
- Each profile is its own SQLite database under `profiles/{id}/`
- Theme preference and window size stay global

### Theming
- Dark / Light / Auto (follows your macOS appearance setting), set in Settings → Appearance

### Self-updating
- Checks GitHub Releases on launch (and on demand from Settings)
- One-click download → drag-replace install (unsigned, but works without Gatekeeper drama because the script-installed copy isn't quarantine-flagged)

---

## Data & privacy

Everything lives locally:

```
~/Library/Application Support/billable/
├── profiles.json                    ← profile registry + active profile
└── profiles/
    ├── default/billable.db          ← Default profile data
    └── {other-id}/billable.db       ← Other profiles
```

No telemetry and no Billable accounts. Unless you turn on phone sync (below), Billable itself makes one network call: GitHub's public API to look for updates, when you launch it or click "Check Now."

The **Billable | Content HQ** switch at the top left opens your Content HQ workspace (a separate web app) inside Billable's window. It only connects when you switch to it. It runs sandboxed with its own sign-in, can't see your Billable data, and any link outside Content HQ opens in your browser. Change its address or sign out in Settings → Content HQ.

**Billable on your phone** is off until you connect it. Turn it on from Content HQ inside Billable: Settings → Billable → Connect this Mac, then confirm in Billable. After that:

- Billable sends a summary of the connected profile to your Content HQ account: the timer, totals, clients (names and rates), jobs, the last 60 days of time plus anything unbilled, invoice statuses, and expenses from this year and last. Client emails, addresses, invoice lines and payment details stay on the Mac.
- Changes you make on the phone (timer, logged time, new jobs, expenses, sent and paid invoices) wait in Content HQ until Billable picks them up. That happens within about 20 seconds while Billable is running, and right away when the Mac wakes. Each one is checked before it's applied, and Settings → Billable on your phone lists them.
- The connection key is stored encrypted with the macOS Keychain (`profiles/{id}/phone-sync.json`) and is only ever sent to the Content HQ address that issued it. Disconnect in Settings to delete it and have Content HQ delete its copy.

---

## Build from source

If you'd rather build the DMG yourself (e.g. you're on Intel, or want to modify the code):

```bash
git clone https://github.com/bulledh-beep/Billable.git
cd Billable
npm install
npm run build         # produces release/Billable-*-arm64.dmg
```

Or run in dev mode with hot reload:

```bash
npm run dev
```

This launches the Vite dev server and Electron together.

---

## Releasing (maintainers)

```bash
npm run release -- patch    # 1.0.x → 1.0.(x+1)
npm run release -- minor    # 1.x.0 → 1.(x+1).0
npm run release -- major    # x.0.0 → (x+1).0.0
```

The script bumps the version, builds the DMG, pushes the tag, and publishes a GitHub Release with auto-generated notes. Within minutes every other Mac with Billable installed sees the update banner.

---

## Tech

- Electron 33 + React 18 + Vite 6 + TypeScript
- better-sqlite3 (one DB per profile)
- Tailwind CSS with CSS-variable-driven theming
- Framer Motion for transitions
- Recharts for the bar charts
- The system font (SF Pro) with tabular figures, so nothing loads from the network

---

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| `Cmd+Shift+Space` | Toggle timer (global) |
| `Cmd+Shift+P` | Pause or resume the active timer |
| `Cmd+Shift+S` | Stop the active timer |
| `Cmd+1` … `Cmd+7` | Jump to Dashboard / Clients / Projects / Time / Billing / Reports / Content HQ |
| `Cmd+N` | New client |
| `Cmd+Shift+N` | New project |
| `Cmd+T` | Add time |
| `Cmd+Shift+I` | New invoice |
| `Cmd+,` | Settings |

---

## License

MIT
