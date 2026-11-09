//! Background services the Electron main process ran (src/main).

pub mod updater;
pub mod phone;
// The app-wide connect confirmation (PhoneConnectModal.tsx). It belongs with
// the components; declared here so phone sync builds as one piece.
#[path = "../components/phone_connect.rs"]
pub mod phone_connect;
