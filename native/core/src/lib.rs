//! Billable's data layer and business rules, ported from `src/main` (Electron).
//!
//! - [`Db`]: one open profile database (`database.ts`), with the same schema,
//!   migrations and results as the Electron app.
//! - [`Profiles`]: `profiles.json` and the per-profile folders (`profiles.ts`).
//! - [`TimerManager`]: start/stop/pause/resume rules (`timer-manager.ts`).
//! - [`Clock`]: "now", fixed in tests.
//!
//! Typical start-up, mirroring `main.ts`:
//!
//! ```no_run
//! use billable_core::{Clock, Db, Profiles, TimerManager};
//! let profiles = Profiles::from_env();
//! profiles.migrate_legacy_if_needed()?;
//! let db = Db::open_profile(&profiles, &profiles.get_active_profile_id(), Clock::system())?;
//! let timer = TimerManager::new(&db)?;
//! # anyhow::Ok(())
//! ```

pub mod csv;
pub mod db;
pub mod js;
pub mod models;
pub mod pdf;
pub mod phone;
pub mod profiles;
pub mod time;
pub mod timer;

pub use db::{compute_commission, Db};
pub use models::*;
pub use profiles::{user_data_dir, Profile, ProfileList, Profiles, ProfilesFile};
pub use time::{effective_invoice_status, local_date, Clock};
pub use timer::TimerManager;
