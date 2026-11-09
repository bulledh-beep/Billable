# Parity harness

Proves `billable-core` behaves like the Electron app by running the same
scenarios through both and comparing everything.

- **Node side** (`node/`): bundles the app's own `src/main/database.ts`,
  `profiles.ts` and `timer-manager.ts` with esbuild (`electron` replaced by a
  stub), fixes `Date` to the scenario's clock, runs each scenario and writes
  `out/<name>/results.json` plus the final data folder `out/<name>/userData`.
  It uses better-sqlite3 11.10.0, the version in the app's lockfile.
- **Rust side** (`../tests/parity.rs`): replays the same scenarios through the
  crate and compares every step's result, the files left in the data folder,
  every SQLite file (schema text and every row of every table, backups and
  exports included) and `profiles.json` byte for byte.

## Run it

```sh
cd native/core/parity/node
npm ci                              # once: better-sqlite3 + esbuild, in this folder only
node build.mjs                      # after any change to src/main/*.ts
node run.mjs ../scenarios/*.json    # sets TZ from each scenario itself
cd ../../..                         # native/
CARGO_TARGET_DIR=target-core cargo test -p billable-core -- --nocapture
```

Without Node results the parity test skips each scenario with a message. Rerun
`run.mjs` after changing a scenario or the dev data.

## Scenarios

| file | covers |
| --- | --- |
| `fresh.json` | New data folder: schema, defaults, then every write path: clients (rate cascade, merge), projects, the timer manager (pause/resume, rounding none/6/15, sub-minute discard, start-while-paused, resync), manual entries, billable flags, invoices (numbering and collisions, prefixes, items, entry links, double-billing rollback, sent/paid/unpaid), billing overview and attention (overdue, stale, closed, drafts, accidental timers, duplicate clients, dismissals incl. odd stored values), dashboard, reports, tax settings/overview, expenses, commissions and commission invoices, backups, export, settings stringification, and error messages. |
| `dev.json` | A copy of `~/Library/Application Support/Billable Dev` (or `$BILLABLE_DEV_DATA`): migrations on real Electron-made files (both profiles), every read view, then writes. Skipped if the folder is missing. Never reads the real `Billable` folder. |
| `legacy.json` | The original single-database layout and old schema: move into `profiles/default`, all migrations, the invoice-link backfill and its backup file, `overdue` → `sent`, and picking up a timer left running. |
| `phone/phone.json` | Phone sync (run with `node phone.mjs`, checked by `tests/phone_parity.rs`): three snapshots compared as exact JSON text, every action kind applied and rejected, redelivery, acks, activity and the rows they leave. Uses America/Toronto because this Mac's tzdata and Node's bundled ICU disagree about BC and Alberta from November 2026. |
| `profiles.json` | `profiles.json` management: create (id collisions, unicode names), rename, colour, switch, delete rules, avatars cleared, a database in a new profile. |

Steps are `["op", ...args]` or `{"op", "args", "as"}`; `{"$": "name.path"}`
in args refers to an earlier result saved with `as`. Ops are the TypeScript
function names plus a few harness ops (`now`, `advance`, `open`, `close`,
`sql`, `exec`, `export`, `files`, `timer.*`, `profile.list`).

## Comparison rules

- Numbers compare by value (`5` = `5.0`), exactly.
- Values SQLite writes with `datetime('now')` (real time, not the scenario
  clock) match when each falls inside its own run's wall-clock window.
- Profile ids that differ only by their random suffix are mapped.
- Paths become `<userData>`; settings whose key contains `token`/`secret`
  are redacted in the Node output (the dev copy has some).
- Known, deliberate differences are listed in `KNOWN_DIFFERENCES` in
  `tests/parity.rs` and printed on every run:
  - after a resync (launch or profile switch) the Rust `TimerManager` holds
    the full `time:list` entry shape (a superset of `getActiveTimer`'s);
  - a `NULL` in a column the app always fills (here `tax_settings.province`
    after an explicit `null`) reads as `""`/`0` instead of `null`.

`out/` is git-ignored: it holds copies of the dev data.
