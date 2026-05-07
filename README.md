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

### Time tracking
- One-click start/stop timer with live HH:MM:SS in the menu bar
- Persistent timer survives app restarts
- Quick-start any active project from Dashboard, Projects list, or Project detail
- Manual entries with date/time pickers
- `+15m / +30m / +1h` quick-add buttons on every entry
- Configurable time rounding (none / 6 / 15 / 30 minutes)
- Global shortcut `Cmd+Shift+Space` to stop the active timer

### Clients & projects
- Full CRUD with rate, currency, default billing terms
- Per-project hourly rate override (defaults from client)
- Editing a client's default rate cascades to projects still using the old rate — already-invoiced time is unaffected
- Project status: Active / Paused / Complete / Archived
- Multi-project invoice generation

### Invoicing
- Auto-populate line items from unbilled time entries
- Per-invoice GST/HST toggle with provincial rate suggestions (5% GST / 13–15% HST)
- Separate "other tax" field for PST or non-Canadian sales tax
- Payment tracking: mark invoices paid with date + method
- "PAID" watermark stamp on paid invoice PDFs
- Quick "Mark Sent" / "Mark Paid" actions on the invoice list (no detail-page roundtrip)
- Editable line items and re-edit after creation
- PDF export (`Cmd+P`-style native print pipeline, no Puppeteer)
- CSV export of any filtered list

### Tax tracking — Canadian-aware
- **Tax Settings**: business identity, province selector, GST/HST registration, fiscal year, estimated income tax bracket
- **Tax Overview**: per-year income summary, monthly bar chart of paid income, expenses by category, "Estimated to Set Aside" with both *realized* (paid invoices) and *projected* (if all invoiced gets paid) values
- **Expenses**: full CRUD, category-aware (Equipment / Software / Home Office / Phone / Travel / Meals / Pro Dev / Other), per-category 50% deductible reminder for Meals
- **Tax Summary PDF**: clean single-page summary export for handing to a bookkeeper or accountant
- Year-end **Invoice CSV** and **Expense CSV** exports

### Reports
- Hours by project, hours by client, earnings by month
- Date range filter
- CSV export

### Multiple profiles
- Run separate businesses side-by-side, each with its own clients, projects, invoices, expenses, and tax settings
- Profile picker in the sidebar — switch with one click
- Each profile is its own SQLite database under `profiles/{id}/`
- Theme preference and window size stay global

### Theming
- Dark / Light / Auto (follows your macOS appearance setting)
- Quick toggle in the sidebar footer; full picker in Settings → Appearance

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

No telemetry, no accounts, no internet round-trips except for checking GitHub Releases. The only network call the app makes is to GitHub's public API to look for updates — and only when you launch it or click "Check Now."

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
- DM Mono for numbers, Outfit for everything else

---

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| `Cmd+Shift+Space` | Toggle timer (global) |
| `Cmd+1` … `Cmd+6` | Jump to Dashboard / Clients / Projects / Time / Invoices / Reports |
| `Cmd+,` | Settings |

---

## License

MIT
