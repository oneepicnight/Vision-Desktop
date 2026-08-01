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
mod receipt;
mod recovery;
mod secrets;
mod session;
mod storage_security;
mod submission;
mod transaction;
mod vault;

pub use account::VisionAccountIdentity;
pub use contract::{
    wallet_contract_gate, WalletCompatibilityGate, WalletContractRequirement, WalletPublicMetadata,
};
pub use secrets::WalletSeed;
