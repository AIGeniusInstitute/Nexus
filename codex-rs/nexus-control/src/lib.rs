//! nexus-control: Nexus control-plane crate for driving the Codex app-server.
//!
//! This crate provides a stdio JSON-RPC client for the Codex app-server and an
//! SQLite event store for persisting the event stream. It is the PoC building
//! block for the Nexus control plane (FP1 + FP2 + FP3 of M0).

pub mod auth;
pub mod audit;
pub mod db;
pub mod eval;
pub mod event_store;
pub mod execpolicy_rules;
pub mod http_server;
pub mod metering;
pub mod model_gateway;
pub mod policy;
pub mod rbac;
pub mod runtime;
pub mod stdio_client;
pub mod timeline;
pub mod ws;
