//! Nudox as "Backend v2" ships it: `backend-mcp` over stdio.
//!
//! # Why a third Nudox adapter
//!
//! The two older ones speak to surfaces that no longer exist. `nudox` drives
//! the MCP endpoint of the pre-rewrite engine (`search`/`read`/`refs`/`graph`
//! with Nudox addresses); `nudox-canonical` drives `interface-cli`, which the
//! rewrite removed. On 2026-09-03 every canonical attempt came back
//! `unsupported`, so there is no Nudox number on the current product at all.
//!
//! Backend v2 shares neither the tool names nor the identity spelling with
//! either: tools are `backend.*`, and a declaration is named by a
//! *coordinate* — `/abs/project::file.rs:47::Name` — that only the server can
//! mint. So every symbol question is two steps here: `backend.resolve` turns
//! the scenario's name into a coordinate, and the task-specific tool answers
//! about that coordinate. The lookup is marked as navigation and the second
//! call as the answer, for the reason `docs/DESIGN.md` gives under "The
//! answer, and the looking-around".
//!
//! # One server per package root
//!
//! `backend-mcp --project PATH` attaches a session to one project. The
//! scenarios name a handful of distinct roots, so one session is started per
//! distinct root and reused by every scenario on it. Indexing happens in
//! [`Adapter::preflight`], not inside an attempt: it is a one-off cost per
//! package, and charging it to whichever scenario happened to run first would
//! make that scenario's numbers mean something different from its
//! neighbours'.
//!
//! # What was checked against the Backend source, and what was not
//!
//! Tool names, argument names and the result fields read here come from
//! `crates/present/grammar.rs` (the `GRAMMARS` table), `apps/mcp/src/jsonrpc/
//! tools.rs` (which tools `tools/list` advertises) and the payloads in
//! `apps/mcp/src/jsonrpc/tests.rs`, at Backend commit `a01c2b0`. They have not
//! been exercised against a live server from this repo; every parser below
//! therefore falls back to the Markdown an agent reads when the structured
//! field it expects is absent, rather than scoring a shape mismatch as a
//! wrong answer.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::{Adapter, Attempt, AttemptBuilder};
use crate::mcp::{McpClient, McpTransport};
use crate::scenario::{Scenario, TaskKind};
use crate::tokens::TokenCount;

/// The adapter's report name. A report column, so it must not drift.
pub const NAME: &str = "nudox-v2";

/// Tools reached through `backend.call` when `tools/list` does not advertise
/// them.
///
/// # Why this is not "unsupported"
///
/// `backend.diff` and `backend.related` are registered routes, but
/// `apps/mcp/src/jsonrpc/tools.rs` lists only `SESSION_TOOLS` by default and
/// marks the rest "capability-dependent". The server's own instructions tell
/// an agent what to do about that: pass `{name, arguments}` to `backend.call`
/// when the host exposes only listed tools. An agent following the
/// instructions reaches the route, so the harness does too. Each such step is
/// labelled `backend.call→<tool>` in the transcript so the indirection is
/// visible.
const VIA_NAMED_CALL: &str = "backend.call";

/// Run-time settings, from the config file.
#[derive(Clone, Debug)]
pub struct NudoxV2Settings {
    /// The `backend-mcp` executable.
    pub binary: Option<PathBuf>,
    /// `BACKEND_LOCALD_BIN`, when locald is not built beside `backend-mcp`.
    pub locald: Option<PathBuf>,
    /// `--workspace`, when the server's per-user default should not be used.
    pub workspace: Option<PathBuf>,
    /// Page size for `resolve`, `search`, `outline` and `graph`. The grammar
    /// caps it at 200.
    pub limit: u32,
    /// How long one root may take to index before it is declared not ready.
    pub index_timeout: Duration,
    /// Per-request deadline, so a hung server cannot hang a run.
    pub request_timeout: Duration,
}

impl Default for NudoxV2Settings {
    fn default() -> Self {
        Self {
            binary: None,
            locald: None,
            workspace: None,
            limit: 50,
            index_timeout: Duration::from_secs(900),
            request_timeout: Duration::from_secs(120),
        }
    }
}

/// One `backend-mcp` process attached to one package root.
struct Session {
    client: McpClient,
    /// `Err` holds why this root cannot be answered from: the index never
    /// became ready, or indexing was refused. Every scenario on the root is
    /// then `unsupported` with this reason.
    ready: Result<(), String>,
    /// Coverage notes from the terminal index receipt, e.g. a partial
    /// publication. Reprinted on every attempt against the root.
    notes: Vec<String>,
}

/// Nudox Backend v2 over stdio MCP.
pub struct NudoxV2Adapter {
    settings: NudoxV2Settings,
    keep_responses: bool,
    /// Why nothing can run, decided at construction (no binary, or a binary
    /// that does not complete an MCP handshake).
    unavailable: Option<String>,
    resident: TokenCount,
    tool_count: usize,
    sessions: HashMap<PathBuf, Session>,
    /// Roots whose server never started, with the reason.
    start_failures: HashMap<PathBuf, String>,
}

impl NudoxV2Adapter {
    /// Resolve the binary and read the tool surface once.
    ///
    /// # Why a probe session at construction
    ///
    /// The report header prints each adapter's resident cost before any
    /// scenario runs, and resident cost is a property of the live tool list.
    /// `backend-mcp` answers `initialize` and `tools/list` without touching a
    /// workspace (`stdio_metadata_does_not_initialize_a_new_workspace` in its
    /// own tests), so a short-lived session started from a scratch directory
    /// reads the surface without indexing anything.
    ///
    /// A missing or broken binary is *not* an error here. It makes every
    /// attempt `unsupported` with the reason, so a run that also holds
    /// ripgrep still produces its numbers.
    pub async fn new(settings: NudoxV2Settings, keep_responses: bool) -> Self {
        let mut adapter = Self {
            settings,
            keep_responses,
            unavailable: None,
            resident: TokenCount::default(),
            tool_count: 0,
            sessions: HashMap::new(),
            start_failures: HashMap::new(),
        };
        let Some(binary) = adapter.settings.binary.clone() else {
            adapter.unavailable = Some(
                "no backend-mcp binary: set `binary` in the config or $NUDOX_MCP_BIN \
                 (see docs/RUN-V2-LOCALLY.md)"
                    .to_owned(),
            );
            return adapter;
        };
        if !binary.exists() {
            adapter.unavailable = Some(format!("backend-mcp not found at {}", binary.display()));
            return adapter;
        }
        let scratch = std::env::temp_dir().join("nbench-nudox-v2-probe");
        let _ = std::fs::create_dir_all(&scratch);
        match adapter.spawn(&scratch).await {
            Ok(client) => {
                adapter.resident = client.resident_cost();
                adapter.tool_count = client.tools().len();
            }
            Err(e) => adapter.unavailable = Some(format!("backend-mcp did not start: {e:#}")),
        }
        adapter
    }

    async fn spawn(&self, project: &Path) -> anyhow::Result<McpClient> {
        let binary = self
            .settings
            .binary
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no binary"))?;
        let mut args = vec!["--project".to_owned(), project.display().to_string()];
        if let Some(workspace) = &self.settings.workspace {
            args.push("--workspace".to_owned());
            args.push(workspace.display().to_string());
        }
        let mut env = HashMap::new();
        if let Some(locald) = &self.settings.locald {
            env.insert("BACKEND_LOCALD_BIN".to_owned(), locald.display().to_string());
        }
        McpClient::connect_with_timeout(
            McpTransport::Stdio {
                command: binary.display().to_string(),
                args,
                env,
                cwd: Some(project.to_path_buf()),
            },
            self.settings.request_timeout,
        )
        .await
    }

    /// Open a session on `root`, or record why it could not be started so
    /// every scenario on the root reports the same reason.
    async fn open_or_record(&mut self, root: &Path) -> Option<Session> {
        if self.start_failures.contains_key(root) {
            return None;
        }
        match self.spawn(root).await {
            Ok(client) => Some(self.index(client, root).await),
            Err(e) => {
                self.start_failures
                    .insert(root.to_path_buf(), format!("backend-mcp did not start: {e:#}"));
                None
            }
        }
    }

    /// Index `root` on a started session, waiting on the poll loop.
    async fn index(&self, client: McpClient, root: &Path) -> Session {
        if !client.has_tool("backend.index") {
            return Session {
                client,
                ready: Err("`backend.index` is not in tools/list".to_owned()),
                notes: Vec::new(),
            };
        }
        let deadline = Instant::now() + self.settings.index_timeout;
        let started = client
            .call_tool("backend.index", json!({ "path": root.display().to_string() }))
            .await;
        let mut response = match started {
            Ok(response) => response,
            Err(e) => {
                return Session {
                    client,
                    ready: Err(format!("backend.index failed: {e:#}")),
                    notes: Vec::new(),
                }
            }
        };
        loop {
            match index_verdict(&response.raw, response.is_error, &response.text) {
                IndexVerdict::Ready(notes) => {
                    tracing::info!("{NAME}: {} indexed", root.display());
                    return Session { client, ready: Ok(()), notes };
                }
                IndexVerdict::Failed(reason) => {
                    tracing::warn!("{NAME}: {} not indexed: {reason}", root.display());
                    return Session { client, ready: Err(reason), notes: Vec::new() };
                }
                IndexVerdict::Pending { tool, arguments } => {
                    if Instant::now() >= deadline {
                        return Session {
                            client,
                            ready: Err(format!(
                                "index not ready after {}s (still pending)",
                                self.settings.index_timeout.as_secs()
                            )),
                            notes: Vec::new(),
                        };
                    }
                    // A poll that carries its own conditional wait blocks
                    // server-side; one that does not answers immediately, so
                    // pacing it here keeps the loop from spinning.
                    if arguments.get("wait_millis").is_none() {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                    response = match client.call_tool(&tool, arguments).await {
                        Ok(response) => response,
                        Err(e) => {
                            return Session {
                                client,
                                ready: Err(format!("{tool} failed: {e:#}")),
                                notes: Vec::new(),
                            }
                        }
                    };
                }
            }
        }
    }

    /// Call `tool` directly if listed, through `backend.call` if not, and
    /// report `None` if neither route exists.
    async fn call(
        client: &McpClient,
        builder: &mut AttemptBuilder,
        tool: &str,
        args: Value,
        role: Role,
        error: &mut Option<String>,
    ) -> Option<Value> {
        let (name, label, call_args) = route(client, tool, args)?;
        let started = Instant::now();
        match client.call_tool(&name, call_args.clone()).await {
            Ok(response) => {
                builder.step(label, call_args.to_string(), &response.text, response.elapsed, response.is_error);
                if role == Role::Answer {
                    builder.primary(&response.text);
                }
                if response.is_error {
                    error.get_or_insert_with(|| format!("{tool}: {}", first_line(&response.text)));
                }
                Some(response.raw)
            }
            Err(e) => {
                let message = format!("{tool}: {e:#}");
                builder.step(label, call_args.to_string(), &message, started.elapsed(), true);
                error.get_or_insert(message);
                None
            }
        }
    }
}

/// Whether a call produced the answer or found the subject of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Answer,
    Navigation,
}

/// Decide how to reach `tool` on this surface.
///
/// Returns the tool name to send, the label for the transcript, and the
/// arguments to send — wrapped for `backend.call` when the route is not
/// listed. `None` means the surface cannot reach the tool at all.
fn route(client: &McpClient, tool: &str, args: Value) -> Option<(String, String, Value)> {
    route_for(|name| client.has_tool(name), tool, args)
}

fn route_for(
    listed: impl Fn(&str) -> bool,
    tool: &str,
    args: Value,
) -> Option<(String, String, Value)> {
    if listed(tool) {
        return Some((tool.to_owned(), tool.to_owned(), args));
    }
    if listed(VIA_NAMED_CALL) {
        return Some((
            VIA_NAMED_CALL.to_owned(),
            format!("{VIA_NAMED_CALL}→{tool}"),
            json!({ "name": tool, "arguments": args }),
        ));
    }
    None
}

fn first_line(text: &str) -> String {
    text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").chars().take(200).collect()
}

/// What an index response says about readiness.
#[derive(Clone, Debug, PartialEq)]
enum IndexVerdict {
    /// Copy this call verbatim and ask again.
    Pending { tool: String, arguments: Value },
    /// Ready to answer; carries coverage notes worth reporting.
    Ready(Vec<String>),
    /// Will not become ready; the reason.
    Failed(String),
}

/// Read one `backend.index` / `backend.index_progress` result.
///
/// # The rule, from the server's own instructions
///
/// "copy the returned `index_poll.call` verbatim as your next
/// `backend.index_progress` tool call. Repeat only while `index_poll` is
/// present and inspect the terminal receipt's outcome. Unknown, unresolved
/// and expired observations do not establish publication."
/// (`apps/mcp/src/jsonrpc.rs`, `INSTRUCTIONS`.)
///
/// The receipt is `structuredContent.index_operation`, an
/// `IndexOperationObservation` tagged `state`/`detail` in kebab-case, whose
/// `known` variant wraps an `IndexOperationStatus` whose own `state` is the
/// `IndexOperationState` (`crates/library/surface.rs` lines 1516-1558 and
/// 1660-1680). Only `published` and `partially-published` establish that
/// there is something to answer from.
///
/// A result with neither a poll nor a receipt is read as ready when it is not
/// an error: that is what an already-indexed, unchanged project answers, and
/// refusing it would make every second run of the same root unsupported. It
/// is noted, so a reader can see the receipt was absent.
fn index_verdict(raw: &Value, is_error: bool, text: &str) -> IndexVerdict {
    let structured = raw.get("structuredContent").unwrap_or(&Value::Null);
    if let Some(call) = structured.pointer("/index_poll/call") {
        if let Some(tool) = call.get("name").and_then(Value::as_str) {
            return IndexVerdict::Pending {
                tool: tool.to_owned(),
                arguments: call.get("arguments").cloned().unwrap_or_else(|| json!({})),
            };
        }
    }
    if is_error {
        return IndexVerdict::Failed(format!("index refused: {}", first_line(text)));
    }
    let Some(observation) = structured.get("index_operation") else {
        return IndexVerdict::Ready(vec!["~v2:index-no-receipt".to_owned()]);
    };
    let outer = observation.get("state").and_then(Value::as_str).unwrap_or("");
    if outer != "known" {
        return IndexVerdict::Failed(format!("index observation `{outer}` does not establish publication"));
    }
    let state = observation.pointer("/detail/state");
    let inner = state
        .and_then(|s| s.get("state"))
        .and_then(Value::as_str)
        .unwrap_or("");
    match inner {
        "published" => IndexVerdict::Ready(Vec::new()),
        "partially-published" => IndexVerdict::Ready(vec!["~v2:index-partially-published".to_owned()]),
        "failed" | "unresolved" => {
            let detail = state
                .and_then(|s| s.pointer("/detail/detail"))
                .map(|d| match d.as_str() {
                    Some(text) => text.to_owned(),
                    None => d.to_string(),
                })
                .unwrap_or_default();
            IndexVerdict::Failed(format!("index {inner}: {detail}"))
        }
        // Accepted or active work with no poll offered cannot be waited on.
        other => IndexVerdict::Failed(format!(
            "index operation `{other}` but no index_poll was offered to wait on"
        )),
    }
}

/// One candidate declaration from a `resolve` or `search` page.
#[derive(Clone, Debug, PartialEq)]
struct Candidate {
    coordinate: String,
    name: String,
    /// Symbol path segments, from `identity.segments` or the trail.
    segments: Vec<String>,
    /// Package-relative source path, when given.
    path: Option<String>,
}

/// Pull candidate declarations out of a result, structured first.
///
/// Rows are `structuredContent.records[].identity`, an `IdentityDto`
/// (`crates/present/dto.rs` line 93: `coordinate`, `trail`, `name`, `path`,
/// `segments`, ...). Where that is absent, every backticked span in the text
/// that looks like a coordinate is taken, because the readable block is
/// specified to carry each row's exact coordinate in backticks
/// (`apps/mcp/src/jsonrpc/tests.rs`, the `preview.contains("`{coordinate}`")`
/// assertion).
fn candidates(raw: &Value, text: &str) -> Vec<Candidate> {
    let mut out = Vec::new();
    if let Some(records) = raw.pointer("/structuredContent/records").and_then(Value::as_array) {
        for row in records {
            let identity = row.get("identity").unwrap_or(row);
            let Some(coordinate) = identity.get("coordinate").and_then(Value::as_str) else {
                continue;
            };
            let name = identity
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| coordinate_name(coordinate));
            let mut segments: Vec<String> = identity
                .get("segments")
                .and_then(Value::as_array)
                .map(|s| s.iter().filter_map(Value::as_str).map(str::to_owned).collect())
                .unwrap_or_default();
            if segments.is_empty() {
                if let Some(trail) = identity.get("trail").and_then(Value::as_str) {
                    segments = split_path(trail);
                }
            }
            let path = identity.get("path").and_then(Value::as_str).map(str::to_owned);
            out.push(Candidate { coordinate: coordinate.to_owned(), name, segments, path });
        }
    }
    if out.is_empty() {
        for coordinate in backticked_coordinates(text) {
            out.push(Candidate {
                name: coordinate_name(&coordinate),
                segments: coordinate_segments(&coordinate),
                path: coordinate_file(&coordinate),
                coordinate,
            });
        }
    }
    out
}

fn backticked_coordinates(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, span) in text.split('`').enumerate() {
        // Odd pieces are inside backticks.
        if i % 2 == 1 && span.starts_with('/') && span.contains("::") && !out.iter().any(|c| c == span) {
            out.push(span.to_owned());
        }
    }
    out
}

/// `/root::src/lib.rs:47::Router` → `Router`.
fn coordinate_name(coordinate: &str) -> String {
    coordinate.rsplit("::").next().unwrap_or(coordinate).to_owned()
}

/// The symbol segments after the `file:line` part of a coordinate.
fn coordinate_segments(coordinate: &str) -> Vec<String> {
    let parts: Vec<&str> = coordinate.split("::").collect();
    // [root, file:line, seg, seg...]
    parts.iter().skip(2).map(|s| (*s).to_owned()).collect()
}

fn coordinate_file(coordinate: &str) -> Option<String> {
    coordinate
        .split("::")
        .nth(1)
        .map(|f| f.rsplit_once(':').map_or(f, |(file, _)| file).to_owned())
}

fn split_path(path: &str) -> Vec<String> {
    path.split(['.', '/', ':'])
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Choose the candidate the scenario is asking about.
///
/// The ranking mirrors `pick_hit` in the legacy adapter, for the same
/// measured reasons: a row whose own name *is* the probe name in the same
/// case wins (the `Memchr3` struct versus the `memchr3` function); then the
/// same name ignoring case; then the longest shared symbol-path suffix with
/// `probe.symbol`; then how many of the probe's module segments appear in the
/// row's file path (Backend's segments may omit the module path, which Rust
/// spells in the file layout instead). Ties keep the server's order.
///
/// Unlike the legacy adapter this never falls back to the top row when no
/// row carries the probe's name: following an unrelated coordinate would
/// answer a different question with full confidence. `None` is returned
/// instead, and the attempt is scored on what the lookup itself showed.
fn pick<'a>(candidates: &'a [Candidate], scenario: &Scenario) -> Option<&'a Candidate> {
    let wanted_name = probe_name(scenario)?;
    let wanted: Vec<String> = scenario
        .probe
        .symbol
        .as_deref()
        .map(split_path)
        .unwrap_or_else(|| vec![wanted_name.clone()]);
    let modules: Vec<String> = wanted
        .iter()
        .take(wanted.len().saturating_sub(1))
        .map(|s| s.to_ascii_lowercase())
        .collect();

    let mut best: Option<(&Candidate, (u8, u8, usize, usize))> = None;
    for candidate in candidates {
        let cased = u8::from(candidate.name == wanted_name);
        let folded = u8::from(candidate.name.eq_ignore_ascii_case(&wanted_name));
        if folded == 0 {
            continue;
        }
        let suffix = candidate
            .segments
            .iter()
            .rev()
            .zip(wanted.iter().rev())
            .take_while(|(a, b)| a.eq_ignore_ascii_case(b))
            .count();
        let file_parts: Vec<String> = candidate
            .path
            .as_deref()
            .map(|p| {
                p.split('/')
                    .map(|part| part.trim_end_matches(".rs").trim_end_matches(".py")
                        .trim_end_matches(".go").trim_end_matches(".ts").trim_end_matches(".js")
                        .to_ascii_lowercase())
                    .collect()
            })
            .unwrap_or_default();
        let in_path = modules.iter().filter(|m| file_parts.contains(m)).count();
        let score = (cased, folded, suffix, in_path);
        if best.is_none_or(|(_, current)| score > current) {
            best = Some((candidate, score));
        }
    }
    best.map(|(candidate, _)| candidate)
}

/// The bare name to look a symbol up by.
fn probe_name(scenario: &Scenario) -> Option<String> {
    scenario
        .probe
        .name
        .clone()
        .or_else(|| {
            scenario
                .probe
                .symbol
                .as_deref()
                .and_then(|s| split_path(s).pop())
        })
}

/// A package URL for one version of the scenario's package.
///
/// `backend.diff` takes "a pinned purl or an exact local package label" for
/// each side (`GRAMMARS`, row `diff`). The purl types are the standard ones
/// (`cargo`, `npm`, `pypi`, `golang`, `maven`, `nuget`); the corpus tags are
/// the ones `nix/corpus.nix` and the scenarios use. An npm scope's `@` is
/// percent-encoded as the purl spec requires, and a PyPI name is normalised
/// to lowercase with `-`, which is the spec's canonical form.
pub fn purl(ecosystem: &str, package: &str, version: &str) -> Option<String> {
    let body = match ecosystem {
        "crates.io" | "cargo" => format!("cargo/{package}"),
        "npm" => match package.strip_prefix('@') {
            Some(scoped) => format!("npm/%40{scoped}"),
            None => format!("npm/{package}"),
        },
        "pypi" => format!("pypi/{}", package.to_ascii_lowercase().replace('_', "-")),
        "go" | "golang" => format!("golang/{package}"),
        "maven" => {
            let (group, artifact) = package.split_once(':')?;
            format!("maven/{group}/{artifact}")
        }
        "nuget" => format!("nuget/{package}"),
        _ => return None,
    };
    Some(format!("pkg:{body}@{version}"))
}

/// The tool(s) a task maps onto, in call order, with their role. Symbol
/// tasks take the resolved coordinate as their argument.
fn plan(task: TaskKind) -> &'static [&'static str] {
    match task {
        // The legacy adapter read `format = "source"` for a definition: the
        // declaration *and* its body. v2 splits those across two tools.
        TaskKind::Definition => &["backend.document", "backend.source"],
        // A signature is what `document` leads with; the body is not asked.
        TaskKind::Signature => &["backend.document"],
        // Members are `document`'s member sections, paged.
        TaskKind::Members => &["backend.document"],
        TaskKind::Callers => &["backend.references"],
        // `references` rows carry the relation kind, so `implements` rows
        // are among them; `document` carries the relation sections too.
        TaskKind::Implementors => &["backend.references", "backend.document"],
        TaskKind::Outbound => &["backend.graph"],
        TaskKind::Overview => &["backend.outline"],
        TaskKind::Diff => &["backend.diff"],
        TaskKind::Search | TaskKind::Implement => &["backend.search"],
    }
}

/// The next-page call a result offers for the same tool, if any.
///
/// `document` pages its sections through `section_page.continuation`, a
/// complete argument object, and stops when `section_page.next_cursor` is
/// absent (`apps/mcp/src/jsonrpc/tests/document_section_pages.rs`). Every
/// other paged route offers `structuredContent.continuation_call`, an
/// executable `{name, arguments}` — possibly addressed to `backend.call`.
fn continuation(raw: &Value, tool: &str) -> Option<(String, Value)> {
    let structured = raw.get("structuredContent")?;
    if tool == "backend.document" {
        structured.pointer("/section_page/next_cursor")?.as_str()?;
        let args = structured.pointer("/section_page/continuation")?.clone();
        return Some((tool.to_owned(), args));
    }
    let call = structured.get("continuation_call")?;
    let name = call.get("name")?.as_str()?.to_owned();
    let mut args = call.get("arguments").cloned().unwrap_or_else(|| json!({}));
    // Unwrap a `backend.call` envelope so routing decides the transport
    // again; the target and its arguments are unchanged.
    if name == VIA_NAMED_CALL {
        let inner = args.get("name")?.as_str()?.to_owned();
        args = args.get("arguments").cloned().unwrap_or_else(|| json!({}));
        return Some((inner, args));
    }
    Some((name, args))
}

#[async_trait::async_trait]
impl Adapter for NudoxV2Adapter {
    fn name(&self) -> &str {
        NAME
    }

    fn description(&self) -> String {
        match &self.unavailable {
            Some(why) => format!("Nudox Backend v2 over stdio MCP — unavailable: {why}"),
            None => format!(
                "Nudox Backend v2 (backend-mcp, stdio): resolve → document/source/references/graph/\
                 outline/diff ({} tools listed, {} cl100k resident)",
                self.tool_count, self.resident.cl100k
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
            if self.sessions.contains_key(&root) {
                continue;
            }
            if let Some(session) = self.open_or_record(&root).await {
                self.sessions.insert(root, session);
            }
        }
        Ok(())
    }

    async fn attempt(&mut self, scenario: &Scenario) -> Attempt {
        if let Some(why) = &self.unavailable {
            return Attempt::unsupported(NAME, scenario, why.clone());
        }
        let Some(root) = scenario.target.root.clone() else {
            return Attempt::unsupported(NAME, scenario, "scenario has no target.root to index");
        };
        if !root.is_dir() {
            return Attempt::unsupported(
                NAME,
                scenario,
                format!("target.root does not exist: {}", root.display()),
            );
        }
        if !self.sessions.contains_key(&root) {
            match self.open_or_record(&root).await {
                Some(session) => {
                    self.sessions.insert(root.clone(), session);
                }
                None => {
                    let why = self
                        .start_failures
                        .get(&root)
                        .cloned()
                        .unwrap_or_else(|| "backend-mcp did not start".to_owned());
                    return Attempt::unsupported(NAME, scenario, why);
                }
            }
        }
        let session = &self.sessions[&root];
        if let Err(why) = &session.ready {
            return Attempt::unsupported(NAME, scenario, format!("index not ready: {why}"));
        }
        let client = &session.client;

        let tools = plan(scenario.task);
        // Every route this task needs must be reachable, or the question
        // cannot be expressed on this surface.
        for tool in tools {
            if route(client, tool, json!({})).is_none() {
                return Attempt::unsupported(
                    NAME,
                    scenario,
                    format!("`{tool}` is neither in tools/list nor reachable through backend.call"),
                );
            }
        }

        let mut builder = AttemptBuilder::new(NAME, scenario, self.resident, self.keep_responses);
        for note in &session.notes {
            builder.note_degraded(note.clone());
        }
        let mut error = None;
        let limit = self.settings.limit.clamp(1, 200);
        let budget = scenario.budget.max_tool_calls.max(1);

        // Questions that need no coordinate.
        let unscoped_args = match scenario.task {
            TaskKind::Diff => {
                let (Some(from), Some(to)) = (
                    purl(&scenario.target.ecosystem, &scenario.target.package, &scenario.target.version),
                    scenario.target.to_version.as_deref().and_then(|to| {
                        purl(&scenario.target.ecosystem, &scenario.target.package, to)
                    }),
                ) else {
                    return Attempt::unsupported(
                        NAME,
                        scenario,
                        format!(
                            "no purl spelling for ecosystem `{}` (or no to_version)",
                            scenario.target.ecosystem
                        ),
                    );
                };
                Some(json!({ "from": from, "to": to }))
            }
            TaskKind::Overview => Some(json!({ "path": root.display().to_string(), "limit": limit })),
            // As the legacy adapter: the name when there is one, because
            // matching is by name first, else the scenario's query.
            TaskKind::Search | TaskKind::Implement => {
                let query = scenario
                    .probe
                    .query
                    .clone()
                    .or_else(|| probe_name(scenario))
                    .unwrap_or_else(|| scenario.question.clone());
                Some(json!({ "query": query, "limit": limit }))
            }
            _ => None,
        };
        if let Some(args) = unscoped_args {
            let tool = tools[0];
            let mut next = Some((tool.to_owned(), args));
            while let Some((name, args)) = next.take() {
                if builder.tool_calls() >= budget {
                    break;
                }
                let raw = Self::call(client, &mut builder, &name, args, Role::Answer, &mut error).await;
                next = raw.as_ref().and_then(|raw| continuation(raw, &name));
            }
            return builder.finish(error);
        }

        // Symbol questions: find the coordinate first.
        let Some(name) = probe_name(scenario) else {
            return Attempt::unsupported(NAME, scenario, "probe names no symbol to resolve");
        };
        let mut coordinate = None;
        for lookup in ["backend.resolve", "backend.search"] {
            if route(client, lookup, json!({})).is_none() {
                continue;
            }
            let raw = Self::call(
                client,
                &mut builder,
                lookup,
                json!({ "query": name, "limit": limit }),
                Role::Navigation,
                &mut error,
            )
            .await;
            if let Some(raw) = raw {
                let found = candidates(&raw, &flatten(&raw));
                if let Some(chosen) = pick(&found, scenario) {
                    coordinate = Some(chosen.coordinate.clone());
                    break;
                }
            }
        }
        let Some(coordinate) = coordinate else {
            builder.note_degraded(format!("~v2:no-coordinate-for({name})"));
            return builder.finish(error);
        };

        for tool in tools {
            let mut next = Some(((*tool).to_owned(), json!({ "coordinate": coordinate })));
            if *tool == "backend.graph" {
                if let Some((_, args)) = next.as_mut() {
                    args["limit"] = json!(limit);
                }
            }
            while let Some((name, args)) = next.take() {
                if builder.tool_calls() >= budget.max(tools.len() as u32 + 1) {
                    break;
                }
                let raw = Self::call(client, &mut builder, &name, args, Role::Answer, &mut error).await;
                // Only members and graph pages are worth following: the other
                // tools answer in one page or their later pages are sections
                // the question did not ask about.
                if matches!(scenario.task, TaskKind::Members | TaskKind::Outbound) {
                    next = raw.as_ref().and_then(|raw| continuation(raw, &name));
                }
            }
        }
        builder.finish(error)
    }

    async fn shutdown(&mut self) {
        // Dropping the clients kills the children (`kill_on_drop`).
        self.sessions.clear();
    }
}

fn flatten(raw: &Value) -> String {
    raw.get("content")
        .and_then(Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::{Truth, TruthMode};
    use crate::testing::scenario_with_truth;

    fn scenario(task: TaskKind, name: &str, symbol: Option<&str>) -> Scenario {
        let mut s = scenario_with_truth(Truth {
            mode: TruthMode::SymbolSet,
            all_of: vec!["x".into()],
            any_of: vec![],
            none_of: vec![],
            oracle: None,
            sources: vec![],
        });
        s.task = task;
        s.probe.name = Some(name.into());
        s.probe.symbol = symbol.map(str::to_owned);
        s
    }

    fn row(coordinate: &str, name: &str, segments: &[&str], path: &str) -> Value {
        json!({ "identity": {
            "coordinate": coordinate, "name": name, "trail": segments.join("::"),
            "segments": segments, "path": path, "shape": "declaration"
        }})
    }

    #[test]
    fn a_poll_is_followed_verbatim_while_it_is_offered() {
        let raw = json!({ "structuredContent": {
            "index_operation": { "state": "known", "detail": { "state": { "state": "accepted" } } },
            "index_poll": { "call": { "name": "backend.index_progress",
                "arguments": { "operation_key": "5a".repeat(32), "detail": "summary" } } }
        }});
        assert_eq!(
            index_verdict(&raw, false, ""),
            IndexVerdict::Pending {
                tool: "backend.index_progress".into(),
                arguments: json!({ "operation_key": "5a".repeat(32), "detail": "summary" }),
            }
        );
    }

    #[test]
    fn a_published_receipt_is_ready_and_a_partial_one_says_so() {
        let published = json!({ "structuredContent": { "index_operation": {
            "state": "known", "detail": { "state": { "state": "published", "detail": {} } } } } });
        assert_eq!(index_verdict(&published, false, ""), IndexVerdict::Ready(vec![]));
        let partial = json!({ "structuredContent": { "index_operation": {
            "state": "known", "detail": { "state": { "state": "partially-published", "detail": {} } } } } });
        assert_eq!(
            index_verdict(&partial, false, ""),
            IndexVerdict::Ready(vec!["~v2:index-partially-published".into()])
        );
    }

    #[test]
    fn failed_unknown_and_unpollable_work_never_count_as_ready() {
        let failed = json!({ "structuredContent": { "index_operation": { "state": "known",
            "detail": { "state": { "state": "failed",
                "detail": { "reason": "compiler", "detail": "rustc refused" } } } } } });
        match index_verdict(&failed, false, "") {
            IndexVerdict::Failed(why) => assert!(why.contains("rustc refused"), "{why}"),
            other => panic!("{other:?}"),
        }
        let unknown = json!({ "structuredContent": { "index_operation": {
            "state": "unknown", "detail": { "operation_key": "00" } } } });
        assert!(matches!(index_verdict(&unknown, false, ""), IndexVerdict::Failed(_)));
        let stuck = json!({ "structuredContent": { "index_operation": { "state": "known",
            "detail": { "state": { "state": "active", "detail": {} } } } } });
        assert!(matches!(index_verdict(&stuck, false, ""), IndexVerdict::Failed(_)));
    }

    #[test]
    fn an_error_result_with_no_poll_is_a_refusal() {
        let raw = json!({ "isError": true, "content": [] });
        match index_verdict(&raw, true, "path is not a project\nmore") {
            IndexVerdict::Failed(why) => assert!(why.contains("path is not a project")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_already_indexed_answer_without_a_receipt_is_ready_but_noted() {
        assert_eq!(
            index_verdict(&json!({ "structuredContent": {} }), false, "up to date"),
            IndexVerdict::Ready(vec!["~v2:index-no-receipt".into()])
        );
    }

    #[test]
    fn candidates_come_from_records_first() {
        let raw = json!({ "structuredContent": { "records": [
            row("/p::src/a.rs:3::find", "find", &["memmem", "Finder", "find"], "src/memmem/mod.rs"),
        ]}});
        let found = candidates(&raw, "");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].segments, vec!["memmem", "Finder", "find"]);
    }

    #[test]
    fn candidates_fall_back_to_backticked_coordinates_in_the_text() {
        let text = "## Resolve\n- `memchr3` `/p/memchr-2.8.3::src/memchr.rs:180::memchr3` fn\n\
                    see `backend.document` next";
        let found = candidates(&json!({}), text);
        assert_eq!(found.len(), 1, "a tool name in backticks is not a coordinate");
        assert_eq!(found[0].name, "memchr3");
        assert_eq!(found[0].path.as_deref(), Some("src/memchr.rs"));
    }

    #[test]
    fn the_same_cased_name_beats_a_case_folded_one() {
        let rows = vec![
            Candidate { coordinate: "/p::src/memchr.rs:9::Memchr3".into(), name: "Memchr3".into(),
                        segments: vec!["Memchr3".into()], path: Some("src/memchr.rs".into()) },
            Candidate { coordinate: "/p::src/memchr.rs:180::memchr3".into(), name: "memchr3".into(),
                        segments: vec!["memchr3".into()], path: Some("src/memchr.rs".into()) },
        ];
        let s = scenario(TaskKind::Signature, "memchr3", Some("memchr::memchr3"));
        assert_eq!(pick(&rows, &s).unwrap().name, "memchr3");
    }

    #[test]
    fn the_module_path_separates_same_named_methods() {
        // `find` exists on several types; the probe's path says which.
        let rows = vec![
            Candidate { coordinate: "/p::src/arch/x86_64/avx2/memchr.rs:40::find".into(),
                        name: "find".into(), segments: vec!["One".into(), "find".into()],
                        path: Some("src/arch/x86_64/avx2/memchr.rs".into()) },
            Candidate { coordinate: "/p::src/memmem/mod.rs:700::find".into(), name: "find".into(),
                        segments: vec!["Finder".into(), "find".into()],
                        path: Some("src/memmem/mod.rs".into()) },
        ];
        let s = scenario(TaskKind::Outbound, "find", Some("memchr::memmem::Finder::find"));
        assert!(pick(&rows, &s).unwrap().coordinate.contains("memmem/mod.rs"));
    }

    #[test]
    fn no_row_with_the_probe_name_means_no_coordinate_not_the_top_row() {
        let rows = vec![Candidate { coordinate: "/p::src/x.rs:1::other".into(), name: "other".into(),
                                    segments: vec![], path: None }];
        let s = scenario(TaskKind::Callers, "memchr3", None);
        assert!(pick(&rows, &s).is_none());
    }

    #[test]
    fn purls_follow_the_spec_per_ecosystem() {
        assert_eq!(purl("crates.io", "hashbrown", "0.16.1").unwrap(), "pkg:cargo/hashbrown@0.16.1");
        assert_eq!(purl("npm", "@scope/pkg", "1.0.0").unwrap(), "pkg:npm/%40scope/pkg@1.0.0");
        assert_eq!(purl("pypi", "Typing_Extensions", "4.0").unwrap(), "pkg:pypi/typing-extensions@4.0");
        assert_eq!(purl("go", "github.com/google/uuid", "v1.6.0").unwrap(),
                   "pkg:golang/github.com/google/uuid@v1.6.0");
        assert_eq!(purl("maven", "org.slf4j:slf4j-api", "2.0.0").unwrap(),
                   "pkg:maven/org.slf4j/slf4j-api@2.0.0");
        assert_eq!(purl("cpp", "zlib", "1.3"), None, "no purl type is guessed");
    }

    #[test]
    fn an_unlisted_route_goes_through_the_named_call_tool() {
        let listed = |n: &str| ["backend.call", "backend.document"].contains(&n);
        let (name, label, args) = route_for(listed, "backend.diff", json!({"from": "a", "to": "b"})).unwrap();
        assert_eq!(name, "backend.call");
        assert_eq!(label, "backend.call→backend.diff");
        assert_eq!(args, json!({"name": "backend.diff", "arguments": {"from": "a", "to": "b"}}));
        let (name, _, _) = route_for(listed, "backend.document", json!({})).unwrap();
        assert_eq!(name, "backend.document", "a listed tool is called directly");
        assert!(route_for(|_| false, "backend.diff", json!({})).is_none());
    }

    #[test]
    fn document_pages_follow_section_continuations_until_no_cursor() {
        let next = json!({ "structuredContent": { "section_page": {
            "next_cursor": "c1", "continuation": { "coordinate": "/p::a", "cursor": "c1" } } } });
        assert_eq!(
            continuation(&next, "backend.document"),
            Some(("backend.document".into(), json!({ "coordinate": "/p::a", "cursor": "c1" })))
        );
        let last = json!({ "structuredContent": { "section_page": { "continuation": {} } } });
        assert_eq!(continuation(&last, "backend.document"), None);
    }

    #[test]
    fn a_wrapped_continuation_call_is_unwrapped_to_its_target() {
        let raw = json!({ "structuredContent": { "continuation_call": { "name": "backend.call",
            "arguments": { "name": "backend.graph", "arguments": { "coordinate": "/p::a", "cursor": "n" } } } } });
        assert_eq!(
            continuation(&raw, "backend.graph"),
            Some(("backend.graph".into(), json!({ "coordinate": "/p::a", "cursor": "n" })))
        );
    }

    #[test]
    fn every_task_maps_to_at_least_one_tool() {
        for task in [TaskKind::Definition, TaskKind::Callers, TaskKind::Outbound,
                     TaskKind::Implementors, TaskKind::Signature, TaskKind::Diff,
                     TaskKind::Overview, TaskKind::Members, TaskKind::Search, TaskKind::Implement] {
            assert!(!plan(task).is_empty(), "{task:?}");
        }
    }

    #[tokio::test]
    async fn a_missing_binary_makes_every_attempt_unsupported_with_the_reason() {
        let mut adapter = NudoxV2Adapter::new(
            NudoxV2Settings { binary: Some("/nonexistent/backend-mcp".into()), ..Default::default() },
            false,
        )
        .await;
        let s = scenario(TaskKind::Callers, "memchr3", None);
        let attempt = adapter.attempt(&s).await;
        let why = attempt.unsupported.expect("unsupported, not scored");
        assert!(why.contains("/nonexistent/backend-mcp"), "{why}");
        assert!(attempt.error.is_none(), "a missing tool is not a tool failure");
    }

    #[tokio::test]
    async fn no_binary_at_all_points_at_the_setup_doc() {
        let mut adapter = NudoxV2Adapter::new(NudoxV2Settings::default(), false).await;
        let why = adapter
            .attempt(&scenario(TaskKind::Definition, "memchr3", None))
            .await
            .unsupported
            .unwrap();
        assert!(why.contains("NUDOX_MCP_BIN"), "{why}");
    }
}
