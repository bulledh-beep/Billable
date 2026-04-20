# Billable

Professional freelance time tracking and invoice generation for macOS.

Built with Electron, React, Vite, Tailwind CSS, and better-sqlite3.

## Setup

```bash
npm install
```

## Development

```bash
npm run dev
```

This launches both the Vite dev server and Electron simultaneously.

## Build

```bash
npm run build
```

Produces a macOS `.dmg` in the `release/` directory.

## Features

- **Dashboard** — Summary cards (hours this week/month, unbilled, outstanding, paid), recent activity feed, quick-start timer from any project
- **Client Management** — Full CRUD with company, email, address, default rate, currency. Client profile with lifetime billing history
- **Project Management** — Projects linked to clients with rate override, color tags, and status tracking (Active/Paused/Complete/Archived)
- **Time Tracking** — One-click start/stop timer with live HH:MM:SS display, manual time entry, edit/delete entries, configurable rounding (6/15/30 min)
- **Invoice Generation** — Auto-populate from unbilled time entries, editable line items, configurable payment terms (Net 15/30/45/custom), PDF export
- **Reports** — Hours by project, hours by client, earnings by month (bar charts), CSV export, date range filtering
- **Settings** — Business profile, invoice numbering, default rate/currency, time rounding, database export/import

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Cmd+Shift+Space` | Toggle timer start/stop |
| `Cmd+1-6` | Navigate between sections |
| `Cmd+N` | New client |
| `Cmd+Shift+N` | New project |
| `Cmd+T` | New time entry |
| `Cmd+,` | Settings |

## Tech Stack

- **Electron** — Desktop shell with native macOS integration (traffic light buttons, tray icon, menu bar)
- **React 18** — Renderer with React Router for navigation
- **Vite** — Build tool with HMR
- **Tailwind CSS** — Utility-first styling with custom dark theme
- **Framer Motion** — Smooth animations and page transitions
- **better-sqlite3** — Local SQLite database (no cloud dependency)
- **Recharts** — Charts for reports
- **Lucide React** — Icon library

## Data

All data is stored locally in a SQLite database at `~/Library/Application Support/billable/billable.db`. No internet connection required. Use Settings > Export Database to create backups.
