//! Run configuration: which adapters take part.
//!
//! # Why tokens come from the environment, never the file
//!
//! Nudox mints a fresh launch token every time its host starts, and that
//! token is a credential. A config file is committed; a credential must not
//! be. So the file names an *environment variable* and the harness reads it
//! at run time — which also means a config can be shared and re-run by
//! someone whose endpoint is on a different port with a different token.

use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::adapter::nudox::Policy;

/// One entry in a run config.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AdapterConfig {
    /// Nudox over its loopback streamable-HTTP endpoint.
    Nudox {
        /// Which built-in ablation, e.g. `full`, `oneshot`, `search-only`,
        /// `graph-first`, `keys`. Omit for `full`.
        #[serde(default)]
        policy: Option<String>,
        /// Endpoint URL. Defaults to `$NUDOX_MCP_URL`.
        #[serde(default)]
        url: Option<String>,
        /// Environment variable holding the bearer token. Defaults to
        /// `NUDOX_MCP_TOKEN`.
        #[serde(default)]
        token_env: Option<String>,
    },
    /// Text search over the package checkout.
    Ripgrep,
    /// rust-analyzer over LSP.
    RustAnalyzer,
    /// Any third-party MCP server spoken over stdio. Not wired into the
    /// default configs yet — the comparison set starts with the systems an
    /// agent already has, and grows outward once those numbers exist.
    McpStdio {
        name: String,
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
    },
}

/// A whole run config.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunConfig {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "adapter")]
    pub adapters: Vec<AdapterConfig>,
}

impl RunConfig {
    pub fn load(path: &std::path::Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        let config: RunConfig =
            toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        if config.adapters.is_empty() {
            bail!("{}: config declares no adapters", path.display());
        }
        Ok(config)
    }
}

/// Resolve a policy name to a [`Policy`].
pub fn policy_by_name(name: &str) -> Result<Policy> {
    Ok(match name {
        "full" => Policy::full(),
        "oneshot" => Policy::oneshot(),
        "search-only" | "search_only" => Policy::search_only(),
        "graph-first" | "graph_first" => Policy::graph_first(),
        "keys" | "keys-not-addresses" => Policy::keys_not_addresses(),
        other => bail!(
            "unknown nudox policy `{other}`; known: full, oneshot, search-only, \
             graph-first, keys"
        ),
    })
}

/// Every ablation, for `--all-ablations`.
pub fn all_policies() -> Vec<Policy> {
    Policy::all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_policy_name_resolves() {
        for policy in all_policies() {
            // Each built-in policy's own name must be a name the resolver
            // accepts, or `--all-ablations` and `--policy` disagree.
            let short = policy.name.trim_start_matches("nudox-");
            assert!(
                policy_by_name(short).is_ok(),
                "policy `{}` has no resolvable short name (`{short}`)",
                policy.name
            );
        }
    }

    #[test]
    fn unknown_policy_is_an_error_not_a_default() {
        assert!(policy_by_name("turbo").is_err());
    }

    #[test]
    fn address_mode_default_is_the_shipped_behaviour() {
        assert_eq!(crate::adapter::nudox::AddressMode::default(), crate::adapter::nudox::AddressMode::Address);
    }
}
