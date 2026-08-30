//! A minimal MCP client, written here rather than taken from `rmcp`.
//!
//! # Why hand-rolled
//!
//! Three reasons, all of them about measurement rather than taste.
//!
//! 1. **The payload is the measurement.** This harness exists to count what a
//!    tool response costs an agent's context. A client library that
//!    deserializes straight into typed structs throws away the exact text
//!    that was on the wire, which is the only thing worth counting.
//! 2. **The server is not the only server.** The comparison set is meant to
//!    grow to third-party MCP servers, which ship as stdio processes, while
//!    Nudox is loopback streamable-HTTP only (it is hosted inside a GUI
//!    process that does not own its stdin/stdout). One small client that
//!    speaks both transports keeps the adapters symmetric.
//! 3. **Version independence.** Linking the server's own `rmcp` would couple
//!    this repo's build to the Backend workspace's, which is a heavy build
//!    behind a nix flake. The benchmark has to stay cheap to run or it will
//!    not be run.

pub mod client;

pub use client::{McpClient, McpTransport, ToolDescriptor, ToolResponse};
