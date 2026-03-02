//! # mail-smtp-mcp-rs
//!
//! Secure SMTP MCP server over stdio.
//!
//! This crate provides configuration, validation, policy enforcement, and server logic for a secure SMTP MCP server.
//!
//! ## Modules
//! - `config`: Configuration structures and environment loading.
//! - `errors`: Error types and codes.
//! - `policy`: Policy enforcement for recipients and sending.
//! - `server`: MCP server implementation.
//! - `startup`: Startup checks and orchestration.
//! - `validation`: Email and input validation utilities.

/// Configuration structures and environment loading.
pub mod config;
/// Error types and codes.
pub mod errors;
/// Policy enforcement for recipients and sending.
pub mod policy;
/// MCP server implementation.
pub mod server;
/// Startup checks and orchestration.
pub mod startup;
/// Email and input validation utilities.
pub mod validation;
