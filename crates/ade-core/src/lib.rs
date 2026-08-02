//! ade-core — the ADE Bootstrapper domain engine (Rust port of the TS v0.1
//! reference implementation, which remains in-repo as the executable spec).
//!
//! Layering:
//! - `types` / `fsutil` / `version`: frozen contracts + deterministic IO
//! - `config` / `context` / `managed` / `audit` / `lockfile`: core surfaces
//! - `instructions` / `translate` / `harness`: canonical-instruction pipeline
//! - `modules`: the fifteen spec modules
//! - `run`: init/plan/apply/verify pipelines
//! - `report`: doctor/status
//! - `gui`: capability inventory, jobs, machine state for the native apps

pub mod config;
pub mod context;
pub mod envpath;
pub mod exec;
pub mod fsutil;
pub mod harness;
pub mod managed;
pub mod testutil;
pub mod types;
pub mod version;

pub mod audit;
pub mod instructions;
pub mod lockfile;
pub mod modules;
pub mod translate;

pub mod gui;
pub mod hook;
pub mod registry;
pub mod remove;
pub mod report;
pub mod run;
