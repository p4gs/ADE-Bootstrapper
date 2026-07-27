//! The fifteen ADE modules (port of `src/modules/`).
pub mod shared;

pub mod approval_gates;
pub mod config_governance;
pub mod context_mgmt;
pub mod cost_governance;
pub mod git_hygiene;
pub mod guardrails;
pub mod injection_defense;
pub mod memory;
pub mod observability;
pub mod reproducibility;
pub mod sandbox;
pub mod scaffolding;
pub mod secrets;
pub mod supply_chain;
pub mod token_efficiency;
