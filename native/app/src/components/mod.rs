//! Billable's UI components, matching the React components in
//! src/renderer/components.

pub mod badge;
pub mod button;
pub mod dial;
pub mod empty_state;
pub mod fields;
pub mod mascot;
pub mod metric;
pub mod money;

pub use badge::{badge, billing_chip, count_badge, status_badge};
pub use button::{Button, Variant};
pub use empty_state::EmptyState;
pub use mascot::{Mascot, Mood, Motion};
pub use metric::{metric_strip, Metric, Tint};
pub use money::{money, money_counting};
pub mod menu;
pub mod modal;
pub mod segmented;
pub mod toast;

pub use menu::{menu_list, popover, Align, MenuEntry, MenuItem};
pub use modal::{Modal, ModalSize};
pub use segmented::{segmented, Segment};
pub use toast::{toast, toast_error, toast_success, ToastIcon};
pub mod confirm;
pub mod date_input;
pub mod select;
pub mod table;

pub use confirm::confirm_dialog;
pub use date_input::{DateChanged, DateInput};
pub use select::select;
pub mod avatar;
pub use avatar::client_avatar;
pub mod client_form;
pub mod merge_clients;
pub use client_form::{ClientFormEvent, ClientFormModal};
pub use merge_clients::{MergeClientsModal, MergeEvent};
pub mod entry_editor;
pub mod project_form;
pub mod record_payment;
pub mod profile_modals;
pub use entry_editor::{EntryEditorEvent, EntryEditorMode, EntryEditorModal};
pub use profile_modals::{ManageProfilesEvent, ManageProfilesModal, NewProfileEvent, NewProfileModal};
pub use project_form::{ProjectFormEvent, ProjectFormModal, ProjectFormValues};
pub use record_payment::{RecordPaymentEvent, RecordPaymentModal};
pub mod update_banner;
pub mod whats_new;
