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
    ///
    /// **Legacy.** This is the pre-rewrite engine's surface; Backend v2 does
    /// not serve it. Use [`AdapterConfig::NudoxV2`].
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
    /// Nudox as it exists on `canonical`, over the `interface-cli`
    /// application protocol.
    ///
    /// **Legacy.** `interface-cli` no longer exists in Backend v2; use
    /// [`AdapterConfig::NudoxV2`]. Separate from `Nudox` because canonical shares
    /// neither the transport nor the tool names of the MCP engine.
    NudoxCanonical {
        /// Path to the built `interface-cli`. Defaults to
        /// `$NUDOX_CANONICAL_CLI`.
        #[serde(default)]
        binary: Option<String>,
        /// Snapshot name to pin. Defaults to `primary`.
        #[serde(default)]
        snapshot: Option<String>,
        /// Result limit. Canonical caps this at 4 and rejects anything
        /// larger before the query reaches a capability.
        #[serde(default)]
        limit: Option<u32>,
    },
    /// Nudox Backend v2: `backend-mcp --project <root>` over stdio, one
    /// session per package root.
    NudoxV2 {
        /// Path to `backend-mcp`. Defaults to `$NUDOX_MCP_BIN`.
        #[serde(default)]
        binary: Option<String>,
        /// Path to `backend-locald`, exported as `BACKEND_LOCALD_BIN`. Only
        /// needed when locald is not built beside `backend-mcp`.
        #[serde(default)]
        locald: Option<String>,
        /// `--workspace` for every session. Defaults to the server's own
        /// per-user state directory.
        #[serde(default)]
        workspace: Option<String>,
        /// Page size for resolve/search/outline/graph; the server caps it
        /// at 200. Defaults to 50.
        #[serde(default)]
        limit: Option<u32>,
        /// Seconds one root may take to index. Defaults to 900.
        #[serde(default)]
        index_timeout_secs: Option<u64>,
        /// Seconds one request may take. Defaults to 120.
        #[serde(default)]
        request_timeout_secs: Option<u64>,
    },
    /// Text search over the package checkout.
    Ripgrep,
    /// rust-analyzer over LSP.
    RustAnalyzer,
    /// Any third-party MCP server spoken over stdio, with its task→tool
    /// mapping taken from a named profile (see `adapter::mcp_stdio`).
    ///
    /// `args`, `cwd` and `index_command` may contain `{root}`, replaced per
    /// package root with the directory the server should read — a scratch
    /// copy of `target.root` unless `copy_root = false`.
    McpStdio {
        /// Report column name.
        name: String,
        /// Mapping profile, e.g. `codegraph`.
        profile: String,
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        /// Working directory for the server; may contain `{root}`.
        #[serde(default)]
        cwd: Option<String>,
        /// Command run once per root before the server starts, e.g. to build
        /// an index. Its first element is the program.
        #[serde(default)]
        index_command: Vec<String>,
        /// Index a scratch copy of each root rather than the root itself, so
        /// a server that writes its index beside the source never writes into
        /// a registry cache. Defaults to true.
        #[serde(default = "default_true")]
        copy_root: bool,
        /// Seconds allowed for `index_command` per root. Defaults to 600.
        #[serde(default)]
        index_timeout_secs: Option<u64>,
        /// Seconds one request may take. Defaults to 120.
        #[serde(default)]
        request_timeout_secs: Option<u64>,
    },
}

fn default_true() -> bool {
    true
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
    fn a_v2_entry_parses_with_every_field_optional() {
        let config: RunConfig = toml::from_str(
            "name = \"v2\"\n[[adapter]]\nkind = \"nudox-v2\"\n",
        )
        .expect("parses");
        assert!(matches!(
            config.adapters[0],
            AdapterConfig::NudoxV2 { binary: None, limit: None, .. }
        ));
    }

    #[test]
    fn a_stdio_entry_copies_roots_unless_told_not_to() {
        let config: RunConfig = toml::from_str(
            "name = \"c\"\n[[adapter]]\nkind = \"mcp-stdio\"\nname = \"codegraph\"\n\
             profile = \"codegraph\"\ncommand = \"codegraph\"\nargs = [\"serve\", \"--path\", \"{root}\"]\n",
        )
        .expect("parses");
        let AdapterConfig::McpStdio { copy_root, args, .. } = &config.adapters[0] else {
            panic!("wrong kind");
        };
        assert!(*copy_root, "copying is the default, so registry caches stay untouched");
        assert_eq!(args[2], "{root}");
    }

    #[test]
    fn the_shipped_configs_all_parse() {
        for entry in std::fs::read_dir("configs").expect("configs dir") {
            let path = entry.expect("entry").path();
            RunConfig::load(&path).unwrap_or_else(|e| panic!("{}: {e:#}", path.display()));
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
