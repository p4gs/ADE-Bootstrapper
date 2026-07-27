//! Module registry — port of `src/registry.ts`.
//! Order is significant: it is the apply order AND the instruction-composition
//! order. No dynamic plugin loading — deliberate (supply-chain surface).

use crate::modules;
use crate::types::AdeModule;

/// The fifteen modules in canonical order (mirrors the TS registry).
pub fn modules() -> Vec<&'static dyn AdeModule> {
    vec![
        &modules::guardrails::MODULE,
        &modules::supply_chain::MODULE,
        &modules::sandbox::MODULE,
        &modules::context_mgmt::MODULE,
        &modules::scaffolding::MODULE,
        &modules::memory::MODULE,
        &modules::injection_defense::MODULE,
        &modules::config_governance::MODULE,
        &modules::observability::MODULE,
        &modules::approval_gates::MODULE,
        &modules::secrets::MODULE,
        &modules::git_hygiene::MODULE,
        &modules::cost_governance::MODULE,
        &modules::reproducibility::MODULE,
        &modules::token_efficiency::MODULE,
    ]
}

pub fn module_ids() -> Vec<&'static str> {
    modules().iter().map(|module| module.id()).collect()
}

pub fn get_module(id: &str) -> Option<&'static dyn AdeModule> {
    modules().into_iter().find(|module| module.id() == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifteen_modules_in_canonical_order() {
        let ids = module_ids();
        assert_eq!(
            ids,
            vec![
                "guardrails",
                "supply-chain",
                "sandbox",
                "context",
                "scaffolding",
                "memory",
                "injection-defense",
                "config-governance",
                "observability",
                "approval-gates",
                "secrets",
                "git-hygiene",
                "cost-governance",
                "reproducibility",
                "token-efficiency",
            ]
        );
        assert!(get_module("secrets").is_some());
        assert!(get_module("nope").is_none());
        for module in modules() {
            assert!(
                module.default_enabled(),
                "{} must be default-enabled",
                module.id()
            );
            assert!(!module.title().is_empty());
            assert!(!module.spec().is_empty());
        }
    }
}
