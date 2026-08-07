//! Rust-only custody boundary for Vision Desktop.
//!
//! This module deliberately exposes public wallet metadata separately from
//! secret material. Secret-bearing types must never cross a Tauri command,
//! derive Serde traits, or enter the general Desktop event/state pipeline.

mod account;
mod activation;
mod amount;
mod contract;
#[cfg(windows)]
mod core_client;
mod device_protection;
mod journal;
mod kdf;
#[cfg(windows)]
mod lifecycle;
#[cfg(windows)]
mod native_secret_buffer;
mod onboarding;
mod panic_policy;
#[cfg(windows)]
mod preview;
mod public_request;
mod receipt;
mod recovery;
#[cfg(windows)]
mod recovery_ceremony;
#[cfg(windows)]
mod recovery_selection;
mod runtime;
mod secret_input;
mod secrets;
#[cfg(windows)]
mod secure_filesystem;
mod session;
#[cfg(windows)]
mod signing;
mod storage_security;
mod submission;
mod transaction;
#[cfg(windows)]
mod transaction_confirmation;
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
pub(crate) use panic_policy::install_production_panic_policy;
#[cfg(windows)]
pub(crate) use recovery_ceremony::NativeRecoveryCredentialCeremony;
#[cfg(windows)]
pub(crate) use recovery_ceremony::NativeWalletSecretCeremony;
pub(crate) use runtime::WalletRuntimeState;
pub use secrets::WalletSeed;
#[cfg(windows)]
pub(crate) use windows_lifecycle::WindowsWalletLifecycle;
