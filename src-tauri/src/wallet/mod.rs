//! Rust-only custody boundary for Vision Desktop.
//!
//! This module deliberately exposes public wallet metadata separately from
//! secret material. Secret-bearing types must never cross a Tauri command,
//! derive Serde traits, or enter the general Desktop event/state pipeline.

mod account;
mod amount;
mod contract;
mod device_protection;
mod journal;
#[cfg(windows)]
mod lifecycle;
mod onboarding;
mod receipt;
mod recovery;
#[cfg(windows)]
mod recovery_selection;
mod runtime;
mod secret_input;
mod secrets;
#[cfg(windows)]
mod secure_filesystem;
mod session;
mod storage_security;
mod submission;
mod transaction;
mod vault;
#[cfg(windows)]
mod windows_lifecycle;

pub use account::VisionAccountIdentity;
pub use contract::{
    wallet_contract_gate, WalletAccountSummary, WalletCompatibilityGate, WalletContractRequirement,
    WalletLifecycleStatus, WalletPublicMetadata,
};
#[cfg(windows)]
pub(crate) use lifecycle::WalletLifecycleAdapters;
pub(crate) use runtime::WalletRuntimeState;
pub use secrets::WalletSeed;
#[cfg(windows)]
pub(crate) use windows_lifecycle::WindowsWalletLifecycle;
