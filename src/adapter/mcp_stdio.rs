//! Any third-party MCP server spoken over stdio, mapped by a named profile.
//!
//! # Why profiles, when the adapters otherwise own their mapping
//!
//! `docs/DESIGN.md` moves the task→tool mapping out of the harness and into
//! each adapter, because ripgrep and an LSP server have no tools to map. For
//! MCP servers the transport, the session handling, the per-root indexing and
//! the cost accounting are identical; only the mapping differs. So the
//! generic parts live here once, and a [`Profile`] is the per-server mapping —
//! the same thing context-bench's `MCP_MAPPINGS` row is, kept in code so it
//! can be tested and can decline what the server cannot express.
//!
//! # What gets indexed, and where
//!
//! Most code-graph servers index a project directory and write their index
//! beside it (CodeGraph writes `.codegraph/`). The scenarios point at
//! registry caches — `~/.cargo/registry/src/…`, site-packages — and a
//! benchmark must not leave files inside those. So by default each root is
//! copied to a scratch directory under the system temp dir, the configured
//! `index_command` runs there, and the server is started on the copy. The
//! copy is made fresh every run, so a stale index from an earlier run can
//! never answer.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::{Adapter, Attempt, AttemptBuilder};
use crate::mcp::{McpClient, McpTransport};
use crate::scenario::{Scenario, TaskKind};
use crate::tokens::TokenCount;

/// Which mapping a configured server uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    /// CodeGraph (`@colbymchenry/codegraph`), `codegraph serve --mcp`.
    CodeGraph,
}

impl Profile {
    pub fn by_name(name: &str) -> anyhow::Result<Self> {
        match name {
            "codegraph" => Ok(Profile::CodeGraph),
            other => anyhow::bail!("unknown mcp-stdio profile `{other}`; known: codegraph"),
        }
    }
}

/// One planned tool call.
#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    pub tool: String,
    pub arguments: Value,
}

/// Map a scenario onto CodeGraph's tools, given which tools are listed.
///
/// Tool names and argument names are read from the installed server's
/// `tools/list` and its source (`dist/mcp/tools.js` in
/// `@colbymchenry/codegraph` 1.6.2): `codegraph_search {query, kind, limit}`,
/// `codegraph_callers`/`codegraph_callees {symbol, file, limit}`,
/// `codegraph_node {symbol, includeCode, file, line}`,
/// `codegraph_explore {query, maxFiles}`.
///
/// # The shipped default lists one tool
///
/// CodeGraph 1.6.2 advertises only `codegraph_explore` unless
/// `CODEGRAPH_MCP_TOOLS` re-enables the rest — its source says the narrower
/// tools "steer mis-picks". The purpose-built tool is used when it is listed;
/// otherwise the question goes to `explore` in natural language, which its
/// description invites ("A natural-language question works too"). That makes
/// the default and the full surface two honest configurations of one server,
/// not one of them hobbled by the harness.
///
/// # What it declines
///
/// A diff: one index holds one checkout, and no tool compares versions.
/// Asking `explore` "what changed between 0.16.1 and 0.17.1" would return
/// whatever the one indexed version contains, which is not an answer to the
/// question but would be scored as one.
pub fn codegraph_plan(scenario: &Scenario, listed: &dyn Fn(&str) -> bool) -> Result<Call, String> {
    let name = scenario
        .probe
        .name
        .clone()
        .or_else(|| {
            scenario
                .probe
                .symbol
                .as_deref()
                .and_then(|s| s.rsplit(['.', ':', '/']).find(|p| !p.is_empty()).map(str::to_owned))
        });
    let explore = || -> Result<Call, String> {
        if listed("codegraph_explore") {
            Ok(Call {
                tool: "codegraph_explore".into(),
                arguments: json!({ "query": scenario.question }),
            })
        } else {
            Err("neither the purpose-built tool nor codegraph_explore is listed".into())
        }
    };
    let dedicated = |tool: &str, arguments: Value| -> Result<Call, String> {
        if listed(tool) {
            Ok(Call { tool: tool.into(), arguments })
        } else {
            explore()
        }
    };
    match scenario.task {
        TaskKind::Diff => Err(
            "one CodeGraph index holds one checkout; no tool compares two versions".into(),
        ),
        // `includeCode` even for a signature. Measured on
        // `rust/memchr/signature-memchr3`: without it, an ambiguous name
        // (`memchr3` is also defined in `src/tests/memchr/naive.rs`) returns
        // only a list of locations and the advice to "re-query with
        // `includeCode: true`" — so the call an agent would actually make is
        // the second one, and that is the one made here.
        TaskKind::Definition | TaskKind::Members | TaskKind::Signature => match name {
            Some(symbol) => dedicated("codegraph_node", json!({ "symbol": symbol, "includeCode": true })),
            None => explore(),
        },
        TaskKind::Callers => match name {
            Some(symbol) => dedicated("codegraph_callers", json!({ "symbol": symbol, "limit": 100 })),
            None => explore(),
        },
        TaskKind::Outbound => match name {
            Some(symbol) => dedicated("codegraph_callees", json!({ "symbol": symbol, "limit": 100 })),
            None => explore(),
        },
        TaskKind::Search => {
            let query = scenario
                .probe
                .query
                .clone()
                .or(name)
                .unwrap_or_else(|| scenario.question.clone());
            dedicated("codegraph_search", json!({ "query": query, "limit": 50 }))
        }
        // No tool answers "who implements" or "list the public surface"
        // directly; `explore` with the question is what the server offers.
        TaskKind::Implementors | TaskKind::Overview | TaskKind::Implement => explore(),
    }
}

/// Run-time settings, from the config file.
#[derive(Clone, Debug)]
pub struct McpStdioSettings {
    pub name: String,
    pub profile: Profile,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub cwd: Option<String>,
    pub index_command: Vec<String>,
    pub copy_root: bool,
    pub index_timeout: Duration,
    pub request_timeout: Duration,
}

/// Replace `{root}` in one configured string.
pub fn substitute(template: &str, root: &Path) -> String {
    template.replace("{root}", &root.display().to_string())
}

/// Where a root's scratch copy lives. Keyed by the adapter name and the
/// whole source path, so two packages with the same directory name, or two
/// adapters indexing the same package, never share a copy.
pub fn scratch_dir(adapter: &str, root: &Path) -> PathBuf {
    let flattened: String = root
        .display()
        .to_string()
        .trim_start_matches('/')
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' || c == '@' { c } else { '_' })
        .collect();
    std::env::temp_dir().join("nbench-mcp").join(adapter).join(flattened)
}

/// Copy a directory tree, files and directories only.
///
/// Symlinks are skipped rather than followed: a registry checkout has none
/// that matter, and following one could copy something far outside the
/// package.
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    for entry in walkdir::WalkDir::new(from).follow_links(false) {
        let entry = entry.map_err(std::io::Error::other)?;
        let relative = entry.path().strip_prefix(from).map_err(std::io::Error::other)?;
        let target = to.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else if entry.file_type().is_file() {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

struct Session {
    client: McpClient,
    /// The directory the server reads — the scratch copy, or the root.
    project: PathBuf,
}

/// A generic stdio MCP server under a [`Profile`].
pub struct McpStdioAdapter {
    settings: McpStdioSettings,
    keep_responses: bool,
    unavailable: Option<String>,
    resident: TokenCount,
    tool_count: usize,
    sessions: HashMap<PathBuf, Session>,
    /// Roots that could not be prepared, with why.
    failures: HashMap<PathBuf, String>,
}

impl McpStdioAdapter {
    /// Start one probe session to read the live tool surface.
    ///
    /// The probe runs against an empty scratch directory: CodeGraph answers
    /// `tools/list` from its static table before any project is open, so
    /// nothing is indexed by this. A server that cannot complete the
    /// handshake makes every attempt `unsupported` with the reason, rather
    /// than stopping a run that also holds other adapters.
    pub async fn new(settings: McpStdioSettings, keep_responses: bool) -> Self {
        let mut adapter = Self {
            settings,
            keep_responses,
            unavailable: None,
            resident: TokenCount::default(),
            tool_count: 0,
            sessions: HashMap::new(),
            failures: HashMap::new(),
        };
        let probe = std::env::temp_dir().join("nbench-mcp").join(&adapter.settings.name).join("_probe");
        let _ = std::fs::create_dir_all(&probe);
        match adapter.spawn(&probe).await {
            Ok(client) => {
                adapter.resident = client.resident_cost();
                adapter.tool_count = client.tools().len();
                tracing::info!(
                    "{}: {} tools listed: {}",
                    adapter.settings.name,
                    adapter.tool_count,
                    client.tools().iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", ")
                );
            }
            Err(e) => {
                adapter.unavailable = Some(format!("`{}` did not start: {e:#}", adapter.settings.command))
            }
        }
        adapter
    }

    async fn spawn(&self, project: &Path) -> anyhow::Result<McpClient> {
        McpClient::connect_with_timeout(
            McpTransport::Stdio {
                command: self.settings.command.clone(),
                args: self.settings.args.iter().map(|a| substitute(a, project)).collect(),
                env: self.settings.env.clone(),
                cwd: self.settings.cwd.as_deref().map(|c| PathBuf::from(substitute(c, project))),
            },
            self.settings.request_timeout,
        )
        .await
    }

    /// Copy (if configured), index, and start a server on one root.
    async fn open(&self, root: &Path) -> Result<Session, String> {
        let project = if self.settings.copy_root {
            let scratch = scratch_dir(&self.settings.name, root);
            if scratch.exists() {
                std::fs::remove_dir_all(&scratch)
                    .map_err(|e| format!("clearing old copy {}: {e}", scratch.display()))?;
            }
            copy_tree(root, &scratch).map_err(|e| format!("copying {}: {e}", root.display()))?;
            scratch
        } else {
            root.to_path_buf()
        };

        if let Some((program, rest)) = self.settings.index_command.split_first() {
            let started = Instant::now();
            let mut command = tokio::process::Command::new(substitute(program, &project));
            command
                .args(rest.iter().map(|a| substitute(a, &project)))
                .envs(&self.settings.env)
                .current_dir(&project)
                .stdin(std::process::Stdio::null())
                .kill_on_drop(true);
            let output = tokio::time::timeout(self.settings.index_timeout, command.output())
                .await
                .map_err(|_| {
                    format!("index command exceeded {}s", self.settings.index_timeout.as_secs())
                })?
                .map_err(|e| format!("index command did not run: {e}"))?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let tail: String = stderr.lines().rev().take(5).collect::<Vec<_>>().join(" | ");
                return Err(format!("index command failed ({}): {tail}", output.status));
            }
            tracing::info!(
                "{}: indexed {} in {:.1}s",
                self.settings.name,
                project.display(),
                started.elapsed().as_secs_f64()
            );
        }

        let client = self.spawn(&project).await.map_err(|e| format!("server did not start: {e:#}"))?;
        Ok(Session { client, project })
    }

    async fn ensure(&mut self, root: &Path) -> Result<(), String> {
        if self.sessions.contains_key(root) {
            return Ok(());
        }
        if let Some(why) = self.failures.get(root) {
            return Err(why.clone());
        }
        match self.open(root).await {
            Ok(session) => {
                self.sessions.insert(root.to_path_buf(), session);
                Ok(())
            }
            Err(why) => {
                tracing::warn!("{}: {}: {why}", self.settings.name, root.display());
                self.failures.insert(root.to_path_buf(), why.clone());
                Err(why)
            }
        }
    }
}

/// Whether a tool's input schema declares a property.
fn accepts(client: &McpClient, tool: &str, property: &str) -> bool {
    client
        .tools()
        .iter()
        .find(|t| t.name == tool)
        .and_then(|t| t.input_schema.pointer(&format!("/properties/{property}")))
        .is_some()
}

#[async_trait::async_trait]
impl Adapter for McpStdioAdapter {
    fn name(&self) -> &str {
        &self.settings.name
    }

    fn description(&self) -> String {
        match &self.unavailable {
            Some(why) => format!("{:?} profile over stdio MCP — unavailable: {why}", self.settings.profile),
            None => format!(
                "{:?} profile over stdio MCP ({} tools listed, {} cl100k resident)",
                self.settings.profile, self.tool_count, self.resident.cl100k
            ),
        }
    }

    fn resident_cost(&self) -> TokenCount {
        self.resident
    }

    async fn preflight(&mut self, scenarios: &[Scenario]) -> anyhow::Result<()> {
        if self.unavailable.is_some() {
            return Ok(());
        }
        let mut roots: Vec<PathBuf> = Vec::new();
        for scenario in scenarios {
            if let Some(root) = &scenario.target.root {
                if root.is_dir() && !roots.contains(root) {
                    roots.push(root.clone());
                }
            }
        }
        for root in roots {
            let _ = self.ensure(&root).await;
        }
        Ok(())
    }

    async fn attempt(&mut self, scenario: &Scenario) -> Attempt {
        let name = self.settings.name.clone();
        if let Some(why) = &self.unavailable {
            return Attempt::unsupported(&name, scenario, why.clone());
        }
        let Some(root) = scenario.target.root.clone() else {
            return Attempt::unsupported(&name, scenario, "scenario has no target.root to index");
        };
        if !root.is_dir() {
            return Attempt::unsupported(
                &name,
                scenario,
                format!("target.root does not exist: {}", root.display()),
            );
        }
        if let Err(why) = self.ensure(&root).await {
            return Attempt::unsupported(&name, scenario, format!("root not ready: {why}"));
        }
        let session = &self.sessions[&root];
        let client = &session.client;

        let plan = match self.settings.profile {
            Profile::CodeGraph => codegraph_plan(scenario, &|tool| client.has_tool(tool)),
        };
        let mut call = match plan {
            Ok(call) => call,
            Err(why) => return Attempt::unsupported(&name, scenario, why),
        };
        // Pin the project explicitly where the tool allows it, so an answer
        // can never come from another index the server happens to find.
        if accepts(client, &call.tool, "projectPath") {
            call.arguments["projectPath"] = json!(session.project.display().to_string());
        }

        let mut builder = AttemptBuilder::new(&name, scenario, self.resident, self.keep_responses);
        let mut error = None;
        let started = Instant::now();
        match client.call_tool(&call.tool, call.arguments.clone()).await {
            Ok(response) => {
                builder.step(
                    &call.tool,
                    call.arguments.to_string(),
                    &response.text,
                    response.elapsed,
                    response.is_error,
                );
                if response.is_error {
                    error = Some(format!(
                        "{}: {}",
                        call.tool,
                        response.text.lines().next().unwrap_or("").chars().take(200).collect::<String>()
                    ));
                }
            }
            Err(e) => {
                let message = format!("{}: {e:#}", call.tool);
                builder.step(&call.tool, call.arguments.to_string(), &message, started.elapsed(), true);
                error = Some(message);
            }
        }
        builder.finish(error)
    }

    async fn shutdown(&mut self) {
        self.sessions.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::{Truth, TruthMode};
    use crate::testing::{diff_scenario, scenario_with_truth};

    fn scenario(task: TaskKind) -> Scenario {
        let mut s = scenario_with_truth(Truth {
            mode: TruthMode::SymbolSet,
            all_of: vec!["x".into()],
            any_of: vec![],
            none_of: vec![],
            oracle: None,
            sources: vec![],
        });
        s.task = task;
        s.probe.symbol = Some("memchr::memchr3".into());
        s
    }

    fn everything(_: &str) -> bool {
        true
    }

    fn explore_only(tool: &str) -> bool {
        tool == "codegraph_explore"
    }

    #[test]
    fn callers_use_the_purpose_built_tool_when_it_is_listed() {
        let call = codegraph_plan(&scenario(TaskKind::Callers), &everything).unwrap();
        assert_eq!(call.tool, "codegraph_callers");
        assert_eq!(call.arguments["symbol"], "memchr3");
    }

    #[test]
    fn the_shipped_default_surface_routes_everything_through_explore() {
        // CodeGraph 1.6.2 lists only `codegraph_explore` by default.
        for task in [TaskKind::Callers, TaskKind::Outbound, TaskKind::Definition,
                     TaskKind::Signature, TaskKind::Members, TaskKind::Search,
                     TaskKind::Implementors, TaskKind::Overview] {
            let call = codegraph_plan(&scenario(task), &explore_only).unwrap();
            assert_eq!(call.tool, "codegraph_explore", "{task:?}");
            assert_eq!(call.arguments["query"], "who calls this?", "the question goes verbatim");
        }
    }

    #[test]
    fn a_diff_is_declined_rather_than_answered_from_one_version() {
        let why = codegraph_plan(&diff_scenario(), &everything).unwrap_err();
        assert!(why.contains("two versions"), "{why}");
    }

    #[test]
    fn nothing_listed_means_unsupported_not_a_guess() {
        assert!(codegraph_plan(&scenario(TaskKind::Callers), &|_| false).is_err());
    }

    #[test]
    fn a_signature_asks_for_the_body_because_a_bare_listing_carries_no_signature() {
        let call = codegraph_plan(&scenario(TaskKind::Signature), &everything).unwrap();
        assert_eq!(call.tool, "codegraph_node");
        assert_eq!(call.arguments["includeCode"], true);
    }

    #[test]
    fn root_substitution_replaces_every_occurrence() {
        assert_eq!(
            substitute("--path={root} --cache={root}/.x", Path::new("/tmp/a")),
            "--path=/tmp/a --cache=/tmp/a/.x"
        );
    }

    #[test]
    fn scratch_copies_never_land_inside_the_source_root() {
        let root = Path::new("/root/.cargo/registry/src/index/memchr-2.8.3");
        let scratch = scratch_dir("codegraph", root);
        assert!(!scratch.starts_with(root));
        assert!(scratch.starts_with(std::env::temp_dir()));
        assert_ne!(
            scratch_dir("codegraph", Path::new("/a/memchr-2.8.3")),
            scratch_dir("codegraph", Path::new("/b/memchr-2.8.3")),
            "same basename, different packages, different copies"
        );
    }

    #[test]
    fn a_copy_holds_the_files_and_skips_nothing_it_should_keep() {
        let base = std::env::temp_dir().join(format!("nbench-copytest-{}", std::process::id()));
        let from = base.join("src-root");
        std::fs::create_dir_all(from.join("src/inner")).unwrap();
        std::fs::write(from.join("src/inner/lib.rs"), "fn a() {}").unwrap();
        let to = base.join("copy");
        copy_tree(&from, &to).unwrap();
        assert_eq!(std::fs::read_to_string(to.join("src/inner/lib.rs")).unwrap(), "fn a() {}");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn an_unknown_profile_is_an_error() {
        assert!(Profile::by_name("context7").is_err());
    }

    #[tokio::test]
    async fn a_server_that_cannot_start_makes_attempts_unsupported() {
        let mut adapter = McpStdioAdapter::new(
            McpStdioSettings {
                name: "ghost".into(),
                profile: Profile::CodeGraph,
                command: "/nonexistent/server".into(),
                args: vec![],
                env: HashMap::new(),
                cwd: None,
                index_command: vec![],
                copy_root: true,
                index_timeout: Duration::from_secs(1),
                request_timeout: Duration::from_secs(1),
            },
            false,
        )
        .await;
        let attempt = adapter.attempt(&scenario(TaskKind::Callers)).await;
        assert!(attempt.unsupported.unwrap().contains("did not start"));
    }
}
