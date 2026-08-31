//! The Nudox MCP adapter, and the ablations run against it.
//!
//! # What an ablation is here
//!
//! The first question this benchmark has to answer is not "is Nudox better
//! than Context7" — it is "which parts of Nudox are carrying the result".
//! That is answered by running the *same* server under different [`Policy`]
//! settings and diffing the numbers. Every policy below is a client-side
//! restriction: none of them needs a rebuilt server, so an ablation sweep
//! costs one `cargo run`, not seven.
//!
//! The policies map onto claims in `Backend/docs/MCP-SURFACE-PLAN.md`:
//!
//! | policy | claim it tests |
//! |---|---|
//! | `full` | the shipped surface, as the baseline every other row is read against |
//! | `oneshot` | §1 — how much of the answer is in the *first* response, which is all a one-call harness like context-bench ever sees |
//! | `search-only` | §2.3 — the unified search really does subsume the structural/semantic split |
//! | `no-followup` | how much value `read`/`refs` add over the search hit alone |
//! | `graph-first` | §1.2 — whether the escape hatch plus its schema card beats the purpose-built tools, and what the card costs |
//! | `keys-not-addresses` | §1.3 — the 54-token hash against the 20-token address, measured end to end rather than on a single string |

use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{Adapter, Attempt, AttemptBuilder};
use crate::mcp::{McpClient, McpTransport};
use crate::scenario::{Scenario, TaskKind};
use crate::tokens::TokenCount;

/// Which identity spelling the adapter feeds back into follow-up calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AddressMode {
    /// Prefer the rendered address (`cargo:memchr@2.8.3::memchr::memchr3[function]`).
    #[default]
    Address,
    /// Force the 64-hex stable key, even where an address is available.
    Key,
}

/// A client-side restriction on how the Nudox surface may be used.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Policy {
    pub name: String,
    pub description: String,
    /// Tools this policy may call. `None` means every tool.
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,
    /// Stop after the first tool call, whatever it returned.
    #[serde(default)]
    pub oneshot: bool,
    /// Route structural questions through `graph` (after `schema`) instead of
    /// the purpose-built tool.
    #[serde(default)]
    pub graph_first: bool,
    #[serde(default)]
    pub address_mode: AddressMode,
    /// `limit` passed to `search`. Lower is cheaper and riskier.
    #[serde(default)]
    pub search_limit: Option<u32>,
}

impl Policy {
    /// The shipped surface, unrestricted.
    pub fn full() -> Self {
        Self {
            name: "nudox-full".into(),
            description: "All nine tools, natural search→read/refs routing".into(),
            allowed_tools: None,
            oneshot: false,
            graph_first: false,
            address_mode: AddressMode::Address,
            search_limit: None,
        }
    }

    /// One tool call and stop — the context-bench comparison shape.
    pub fn oneshot() -> Self {
        Self {
            name: "nudox-oneshot".into(),
            description: "A single tool call per scenario, context-bench style".into(),
            allowed_tools: None,
            oneshot: true,
            graph_first: false,
            address_mode: AddressMode::Address,
            search_limit: None,
        }
    }

    /// `search` and nothing else, however many calls.
    pub fn search_only() -> Self {
        Self {
            name: "nudox-search-only".into(),
            description: "Only `search`; no read, refs, graph or diff".into(),
            allowed_tools: Some(vec!["search".into()]),
            oneshot: false,
            graph_first: false,
            address_mode: AddressMode::Address,
            search_limit: None,
        }
    }

    /// Structural questions go through the escape hatch.
    pub fn graph_first() -> Self {
        Self {
            name: "nudox-graph-first".into(),
            description: "schema + graph for structural questions".into(),
            allowed_tools: None,
            oneshot: false,
            graph_first: true,
            address_mode: AddressMode::Address,
            search_limit: None,
        }
    }

    /// Identical to `full` except identities are echoed as 64-hex keys.
    pub fn keys_not_addresses() -> Self {
        Self {
            name: "nudox-keys".into(),
            description: "Full surface, but follow-ups use 64-hex keys not addresses".into(),
            allowed_tools: None,
            oneshot: false,
            graph_first: false,
            address_mode: AddressMode::Key,
            search_limit: None,
        }
    }

    /// Every built-in ablation, in report order.
    pub fn all() -> Vec<Policy> {
        vec![
            Policy::full(),
            Policy::oneshot(),
            Policy::search_only(),
            Policy::graph_first(),
            Policy::keys_not_addresses(),
        ]
    }

    fn allows(&self, tool: &str) -> bool {
        match &self.allowed_tools {
            None => true,
            Some(list) => list.iter().any(|t| t == tool),
        }
    }
}

/// One identity pulled out of a search response.
#[derive(Clone, Debug)]
struct Hit {
    key: Option<String>,
    address: Option<String>,
}

impl Hit {
    fn identity(&self, mode: AddressMode) -> Option<&str> {
        match mode {
            AddressMode::Address => self.address.as_deref().or(self.key.as_deref()),
            AddressMode::Key => self.key.as_deref().or(self.address.as_deref()),
        }
    }

    /// Whether the requested spelling was actually available.
    ///
    /// The `keys` ablation exists to price the 64-hex key against the
    /// readable address. It can only do that if the response hands back a
    /// key — and in practice `search` renders an *address* per hit and the
    /// hash never crosses the wire at all. When that happens the ablation
    /// silently falls back to the address and reports a row identical to
    /// `nudox-full`, which reads as "the address scheme saves nothing"
    /// when what actually happened is "the counterfactual was never run".
    ///
    /// So it is reported rather than papered over.
    fn honoured(&self, mode: AddressMode) -> bool {
        match mode {
            AddressMode::Address => self.address.is_some(),
            AddressMode::Key => self.key.is_some(),
        }
    }
}

/// Nudox, reached over its loopback streamable-HTTP endpoint.
pub struct NudoxAdapter {
    client: McpClient,
    policy: Policy,
    resident: TokenCount,
    keep_responses: bool,
    /// Index each scenario's `target.root` before running, so the corpus and
    /// the file-based adapters answer about the same code.
    ensure_indexed: bool,
    /// Deadline handed to `index`, which covers the whole batch.
    index_wait_seconds: u32,
    /// Lineages the corpus actually holds, read once at preflight.
    ///
    /// `None` means the check could not be made and every scenario is
    /// attempted — never silently skipped.
    loaded: Option<Vec<String>>,
}

impl NudoxAdapter {
    /// Connect to a running endpoint.
    ///
    /// Nudox has no standalone server binary: the MCP endpoint is started
    /// inside the `lindsey` GUI process (`McpHost::start`), which mints a
    /// fresh launch token each time and binds an ephemeral loopback port. So
    /// the URL and token are inputs to this harness, not something it can
    /// discover — `docs/RUNNING.md` records how to read them off
    /// the running host.
    pub async fn connect(
        url: &str,
        token: Option<String>,
        policy: Policy,
        keep_responses: bool,
        ensure_indexed: bool,
    ) -> anyhow::Result<Self> {
        let client = McpClient::connect(McpTransport::Http {
            url: url.to_owned(),
            token,
        })
        .await?;
        let resident = client.resident_cost();
        Ok(Self {
            client,
            policy,
            resident,
            keep_responses,
            ensure_indexed,
            index_wait_seconds: 600,
            loaded: None,
        })
    }

    /// The `search` call every task starts from.
    ///
    /// # Why `overview` does not use the prose query
    ///
    /// `search` matches by *name and kind* — its own description says so, and
    /// says it is "not full-text over documentation". Semantic ranking covers
    /// the rest, but only when an embedder is configured; without one, a prose
    /// query like "memchr public exports" matches no symbol name and returns
    /// zero rows. Measured: it did exactly that, scoring 13% on a scenario
    /// ripgrep passed.
    ///
    /// That was the harness using the tool wrong, not the tool failing. An
    /// agent enumerating a package's surface searches the package *name* —
    /// which prefix-matches the family of symbols named after it — and asks
    /// for a large page. So `overview` does that, and the prose stays in the
    /// scenario for the judge and for adapters that do read prose.
    fn search_args(&self, scenario: &Scenario) -> Value {
        let query = if scenario.task == TaskKind::Overview {
            scenario
                .probe
                .name
                .clone()
                .unwrap_or_else(|| scenario.target.package.clone())
        } else {
            self.free_text_query(scenario)
        };
        let mut args = json!({
            "query": query,
            "packages": [scenario.target.lineage()],
        });
        // Enumerating a surface needs a page big enough to hold it; the
        // default is 50 and the cap is 500.
        if scenario.task == TaskKind::Overview {
            args["limit"] = json!(200);
        }
        if let Some(limit) = self.policy.search_limit {
            args["limit"] = json!(limit);
        }
        args
    }

    /// The query for every task that is looking for one named thing.
    ///
    /// # Why the last segment, not the whole path
    ///
    /// `search` matches by **name and kind** — its own description says so.
    /// A dotted or `::`-separated path is therefore not a better query than
    /// the bare identifier, it is a query that matches nothing: no
    /// declaration is *named* `requests.api.request`.
    ///
    /// Measured on 2026-08-30. The Python scenario carries
    /// `query = "requests.api.request"`, and sending it verbatim returned
    /// `## Search · 0 … (no matches)` in 54 tokens — scored as a 0% failure
    /// against a package Nudox had loaded and indexed perfectly well. The
    /// Rust scenarios only escaped this because their queries were already
    /// bare (`search_slice_with_raw`).
    ///
    /// This is the same mistake `overview` already documents above, and it is
    /// the harness's, not the tool's: an agent looking for a symbol searches
    /// its name. The full path stays in the scenario for the judge and for
    /// adapters that do read paths.
    fn free_text_query(&self, scenario: &Scenario) -> String {
        let full = scenario
            .probe
            .query
            .clone()
            .or_else(|| scenario.probe.name.clone())
            .or_else(|| scenario.probe.symbol.clone())
            .unwrap_or_else(|| scenario.question.clone());
        last_segment(&full)
    }

    /// Pull identities out of a search response, preferring the structured
    /// result and falling back to the Markdown projection.
    ///
    /// Both paths exist because the Markdown *is* the agent-facing surface —
    /// if an identity cannot be recovered from it, an agent could not have
    /// used it either, and that is worth knowing rather than papering over
    /// with the structured copy.
    fn hits_from(raw: &Value, markdown: &str) -> Vec<Hit> {
        let mut out = Vec::new();
        if let Some(hits) = raw
            .pointer("/structuredContent/hits")
            .and_then(Value::as_array)
        {
            for hit in hits {
                let key = hit
                    .pointer("/hit/key")
                    .or_else(|| hit.get("key"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                let address = hit.get("address").and_then(Value::as_str).map(str::to_owned);
                if key.is_some() || address.is_some() {
                    out.push(Hit { key, address });
                }
            }
        }
        if out.is_empty() {
            out = parse_identities(markdown);
        }
        out
    }
}

/// Recover identities from the Markdown search projection.
///
/// The renderer writes each hit as an identity line at column zero followed
/// by a two-space-indented declaration, so an unindented line carrying an
/// `ecosystem:` prefix is an identity and nothing else is.
fn parse_identities(markdown: &str) -> Vec<Hit> {
    let mut out = Vec::new();
    for line in markdown.lines() {
        if line.starts_with(' ') || line.starts_with('#') || line.starts_with('~') {
            continue;
        }
        let line = line.trim_end();
        if line.is_empty() || !line.contains(':') {
            continue;
        }
        // A key is `ecosystem:name#<64 hex>`; an address is
        // `ecosystem:name[@version][::path][kind]`.
        if let Some((_, hex)) = line.rsplit_once('#') {
            if hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                out.push(Hit {
                    key: Some(line.to_owned()),
                    address: None,
                });
                continue;
            }
        }
        if line.contains("::") || line.ends_with(']') {
            out.push(Hit {
                key: None,
                address: Some(line.to_owned()),
            });
        }
    }
    out
}

#[async_trait::async_trait]
impl Adapter for NudoxAdapter {
    fn name(&self) -> &str {
        &self.policy.name
    }

    fn description(&self) -> String {
        format!(
            "{} ({} tools resident, {} cl100k)",
            self.policy.description,
            self.client.tools().len(),
            self.resident.cl100k
        )
    }

    fn resident_cost(&self) -> TokenCount {
        self.resident
    }

    async fn preflight(&mut self, scenarios: &[Scenario]) -> anyhow::Result<()> {
        if self.ensure_indexed {
            self.index_roots(scenarios).await;
        }

        // Census the corpus after any indexing, so a package this run just
        // added is counted as present.
        if self.policy.allows("packages") {
            match self.client.call_tool("packages", json!({})).await {
                Ok(response) => {
                    let loaded = parse_lineages(&response.raw, &response.text);
                    tracing::info!("{}: corpus holds {:?}", self.policy.name, loaded);
                    self.loaded = Some(loaded);
                }
                Err(e) => {
                    // Attempt everything rather than skip everything.
                    tracing::warn!("{}: package census failed: {e:#}", self.policy.name);
                }
            }
        }
        Ok(())
    }

    async fn attempt(&mut self, scenario: &Scenario) -> Attempt {
        // A package this corpus does not hold is out of reach, not answered
        // wrongly.
        //
        // This is the same distinction the harness already draws for
        // rust-analyzer declining Go, and it has to be drawn here too or the
        // comparison is rigged in the other direction: an unindexed package
        // would score Nudox a confident 0% at ~100 tokens, which reads on the
        // chart as "answered cheaply and badly" when what happened is
        // "was never asked to hold this". `packages` is explicit that an
        // unloaded package returns an empty version list rather than an
        // error, precisely so a caller can tell those apart.
        if let Some(loaded) = &self.loaded {
            let lineage = scenario.target.lineage();
            if !loaded.contains(&lineage) {
                return Attempt::unsupported(
                    &self.policy.name,
                    scenario,
                    format!(
                        "`{lineage}` is not loaded in this corpus (loaded: {})",
                        if loaded.is_empty() {
                            "nothing".to_owned()
                        } else {
                            loaded.join(", ")
                        }
                    ),
                );
            }
        }

        let mut builder =
            AttemptBuilder::new(&self.policy.name, scenario, self.resident, self.keep_responses);
        let mut error = None;

        // `diff` and `packages` answer directly; nothing else needs a search
        // to find the subject first.
        match scenario.task {
            TaskKind::Diff => {
                if !self.policy.allows("diff") {
                    return Attempt::unsupported(
                        &self.policy.name,
                        scenario,
                        "policy forbids `diff`",
                    );
                }
                let Some(to) = scenario.target.to_version.as_ref() else {
                    return Attempt::unsupported(
                        &self.policy.name,
                        scenario,
                        "scenario has no to_version",
                    );
                };
                let args = json!({
                    "package": scenario.target.lineage(),
                    "from_version": scenario.target.version,
                    "to_version": to,
                    // Ask for the whole diff, not the default 50 rows.
                    //
                    // A cross-version diff lists changes ordered by path, and
                    // a question about one module is answered on whichever
                    // page that module falls on. Taking the default page and
                    // then paging is how `hashbrown` reached 75% — three of
                    // four methods, with `insert_entry_with_key` still past
                    // the cut. The right correction is to ask for a bigger
                    // page rather than to raise the scenario's call budget
                    // until it passes: 500 is the tool's documented cap, and
                    // asking for it is what an agent that wants a complete
                    // answer does.
                    "limit": 500,
                });
                self.call(&mut builder, "diff", args.clone(), &mut error, Evidence::Answer)
                    .await;

                // Follow the cursor the tool hands back.
                //
                // A real agent that did not find its answer on page one asks
                // for page two — `diff` renders a `next:` line precisely so it
                // can. Stopping at page one measured the harness's patience
                // rather than the tool's reach: on
                // `rust/hashbrown/diff-0.16.1-to-0.17.1` the 2026-08-30 run
                // took a single 10,000-character page of 50 changes, ordered
                // by path, which `alloc` and `control` internals filled
                // entirely — all four expected `HashMap` methods were absent
                // from it, and the scenario scored 0%.
                //
                // Bounded by the scenario's own `max_tool_calls`, so a large
                // diff cannot spend an unbounded budget, and by a cursor that
                // must actually advance, so a server repeating itself cannot
                // loop here.
                if !self.policy.oneshot {
                    let budget = scenario.budget.max_tool_calls.max(1);
                    let mut seen: Option<String> = None;
                    while builder.tool_calls() < budget {
                        let Some(cursor) = next_cursor(builder.context()) else {
                            break;
                        };
                        if seen.as_deref() == Some(cursor.as_str()) {
                            break;
                        }
                        let mut paged = args.clone();
                        paged["cursor"] = json!(cursor);
                        seen = Some(cursor);
                        self.call(&mut builder, "diff", paged, &mut error, Evidence::Answer)
                            .await;
                    }
                }
                return builder.finish(error);
            }
            TaskKind::Overview if self.policy.allows("packages") => {
                let args = json!({ "package": scenario.target.lineage() });
                self.call(&mut builder, "packages", args, &mut error, Evidence::Answer)
                    .await;
                if self.policy.oneshot {
                    return builder.finish(error);
                }
            }
            _ => {}
        }

        // The structural questions the escape hatch is for.
        if self.policy.graph_first
            && matches!(scenario.task, TaskKind::Implementors | TaskKind::Callers)
            && self.policy.allows("graph")
        {
            if self.policy.allows("schema") {
                self.call(&mut builder, "schema", json!({}), &mut error, Evidence::Navigation)
                    .await;
                if self.policy.oneshot {
                    return builder.finish(error);
                }
            }
            if let Some((query, query_args)) = graph_query_for(scenario) {
                let args = json!({ "query": query, "args": query_args, "limit": 500 });
                self.call(&mut builder, "graph", args, &mut error, Evidence::Answer)
                        .await;
                return builder.finish(error);
            }
        }

        // Everything else starts with a search.
        if !self.policy.allows("search") {
            return Attempt::unsupported(&self.policy.name, scenario, "policy forbids `search`");
        }
        let search_args = self.search_args(scenario);
        // Is this search the answer, or the way to it?
        //
        // It is the answer when nothing can follow it — a one-call policy, or
        // one restricted to `search` — and when the question is itself a
        // listing (`Overview` enumerates a surface; `Search`/`Implement` hand
        // the hits straight to a judge). Otherwise it is the directory lookup
        // before `refs` or `read`, and scoring it as the reply would charge
        // the adapter for every near-name the index correctly offered.
        let search_role = if self.policy.oneshot
            || !self.policy.allows("refs") && !self.policy.allows("read")
            || matches!(
                scenario.task,
                TaskKind::Search | TaskKind::Implement | TaskKind::Overview
            ) {
            Evidence::Answer
        } else {
            Evidence::Navigation
        };
        let hits = self
            .call_search(&mut builder, search_args, &mut error, search_role)
            .await;

        if self.policy.oneshot || matches!(scenario.task, TaskKind::Search | TaskKind::Implement) {
            return builder.finish(error);
        }

        let Some(hit) = pick_hit(&hits, scenario) else {
            return builder.finish(error);
        };
        let Some(identity) = hit.identity(self.policy.address_mode) else {
            return builder.finish(error);
        };
        let identity = identity.to_owned();
        if !hit.honoured(self.policy.address_mode) {
            builder.note_degraded(format!(
                "~ablation:not-exercised(address_mode={:?}; search returned no {} to send back)",
                self.policy.address_mode,
                match self.policy.address_mode {
                    AddressMode::Key => "64-hex key",
                    AddressMode::Address => "address",
                }
            ));
        }

        match scenario.task {
            TaskKind::Callers | TaskKind::Outbound => {
                if self.policy.allows("refs") {
                    let direction = if scenario.task == TaskKind::Callers {
                        "in"
                    } else {
                        "out"
                    };
                    let args = json!({ "key": identity, "direction": direction });
                    self.call(&mut builder, "refs", args, &mut error, Evidence::Answer)
                        .await;
                }
            }
            TaskKind::Definition | TaskKind::Signature | TaskKind::Members => {
                if self.policy.allows("read") {
                    let format = if scenario.task == TaskKind::Signature {
                        "signature"
                    } else {
                        "source"
                    };
                    let args = json!({ "keys": [identity], "format": format });
                    self.call(&mut builder, "read", args, &mut error, Evidence::Answer)
                        .await;
                }
            }
            // Enumerating a surface is not a search question.
            //
            // `search` ranks a *name* section built from "exact and prefix
            // name matches", so a query can only ever reach symbols sharing a
            // prefix with it. Asking it for `memchr`'s public surface returns
            // `memchr`, `memchr2`, `memchr_iter` … and can never return
            // `memrchr`, because that name does not start with the query.
            // Measured: 60% strict recall on every Nudox policy across every
            // run, with all six `memrchr*` facts absent and all nine
            // `memchr*` ones found — a shape that is the query's, not the
            // index's.
            //
            // Following it with `read` made that worse rather than better:
            // an overview scenario names no `probe.symbol`, so the read
            // landed on whichever hit ranked first — in
            // `run-2026-08-30-225507` that was
            // `arch::x86_64::avx2::memchr[module]`, an internal module, and
            // the call bought nothing.
            //
            // `graph` is the tool that can enumerate, and the schema card
            // exposes `isPublic` for exactly this. The search listing stays —
            // for a package whose exports do share the package's prefix it is
            // a complete answer on its own, and it is charged for either way.
            TaskKind::Overview => {
                if self.policy.allows("graph") {
                    if let Some((query, query_args)) = graph_query_for(scenario) {
                        // Every arg value is a string: `GraphQueryArgs.args`
                        // is a `BTreeMap<String, String>` and the engine
                        // coerces to the property's declared type — a JSON
                        // `true` is a type error, not a boolean. And
                        // enumerating needs a page big enough to enumerate
                        // into; 500 is the tool's documented cap.
                        let args = json!({ "query": query, "args": query_args, "limit": 500 });
                        self.call(&mut builder, "graph", args, &mut error, Evidence::Answer)
                            .await;
                    }
                } else if self.policy.allows("read") {
                    let args = json!({ "keys": [identity], "format": "source" });
                    self.call(&mut builder, "read", args, &mut error, Evidence::Answer)
                        .await;
                }
            }
            TaskKind::Implementors => {
                if self.policy.allows("graph") {
                    if self.policy.allows("schema") {
                        self.call(&mut builder, "schema", json!({}), &mut error, Evidence::Navigation)
                    .await;
                    }
                    if let Some((query, query_args)) = graph_query_for(scenario) {
                        let args = json!({ "query": query, "args": query_args, "limit": 500 });
                        self.call(&mut builder, "graph", args, &mut error, Evidence::Answer)
                        .await;
                    }
                }
            }
            TaskKind::Diff | TaskKind::Search | TaskKind::Implement => {}
        }

        builder.finish(error)
    }

    async fn shutdown(&mut self) {}
}

impl NudoxAdapter {
    async fn index_roots(&mut self, scenarios: &[Scenario]) {
        if !self.policy.allows("index") {
            tracing::warn!("{}: policy forbids `index`; skipping indexing", self.policy.name);
            return;
        }

        // One `index` call per distinct root. Targets are deduplicated
        // because indexing runs a real compiler front end and costs seconds
        // to minutes per package.
        let mut roots: Vec<String> = Vec::new();
        for scenario in scenarios {
            if let Some(root) = &scenario.target.root {
                let rendered = root.display().to_string();
                if !roots.contains(&rendered) {
                    roots.push(rendered);
                }
            }
        }
        if roots.is_empty() {
            return;
        }

        let _ = &self.loaded;
        tracing::info!(
            "{}: indexing {} root(s) so the corpus matches what the file-based \
             adapters read",
            self.policy.name,
            roots.len()
        );
        let args = json!({ "targets": roots, "wait_seconds": self.index_wait_seconds });
        match self.client.call_tool("index", args).await {
            Ok(response) => {
                // `index` reports every target separately and returns
                // `running` rather than failing when the deadline expires, so
                // a non-error response is not proof everything landed. The
                // response is logged whole rather than parsed into a verdict
                // the tool did not give.
                tracing::info!("index preflight:\n{}", response.text);
                if response.is_error {
                    tracing::warn!("index preflight reported an error; \
                                    scenarios may answer from a different version");
                }
            }
            Err(e) => {
                tracing::warn!("index preflight failed: {e:#}");
            }
        }
    }
}

impl NudoxAdapter {
    async fn call(
        &self,
        builder: &mut AttemptBuilder,
        tool: &str,
        args: Value,
        error: &mut Option<String>,
        role: Evidence,
    ) {
        let started = Instant::now();
        match self.client.call_tool(tool, args.clone()).await {
            Ok(response) => {
                builder.step(
                    tool,
                    args.to_string(),
                    &response.text,
                    response.elapsed,
                    response.is_error,
                );
                if role == Evidence::Answer {
                    builder.primary(&response.text);
                }
            }
            Err(e) => {
                let message = format!("{tool}: {e}");
                builder.step(tool, args.to_string(), &message, started.elapsed(), true);
                error.get_or_insert(message);
            }
        }
    }

    async fn call_search(
        &self,
        builder: &mut AttemptBuilder,
        args: Value,
        error: &mut Option<String>,
        role: Evidence,
    ) -> Vec<Hit> {
        let started = Instant::now();
        match self.client.call_tool("search", args.clone()).await {
            Ok(response) => {
                let hits = Self::hits_from(&response.raw, &response.text);
                builder.step(
                    "search",
                    args.to_string(),
                    &response.text,
                    response.elapsed,
                    response.is_error,
                );
                if role == Evidence::Answer {
                    builder.primary(&response.text);
                }
                hits
            }
            Err(e) => {
                let message = format!("search: {e}");
                builder.step("search", args.to_string(), &message, started.elapsed(), true);
                error.get_or_insert(message);
                Vec::new()
            }
        }
    }
}

/// Whether a call produced the adapter's **answer**, or the looking-around it
/// did to find it.
///
/// # Why this distinction has to exist
///
/// Scoring reads everything an adapter surfaced, concatenated. For a one-shot
/// tool that is the same thing as its answer. For a tool that navigates —
/// `search` to locate a symbol, then `refs` to answer about it — it is not,
/// and conflating the two scores the directory lookup as if it were the reply.
///
/// Measured on `go/uuid/callers-parse` in `run-2026-08-30-225507`: Nudox's
/// `refs` response listed 21 inbound callers and **no** `ParseBytes`, which is
/// correct — `ParseBytes` has its own parsing path and never calls `Parse`
/// (verified at `uuid.go:120`). But the `search` step that found the symbol
/// listed `ParseBytes`, correctly, as a declaration matching the query
/// "Parse". The scenario's distractor rule then fired against the union, and
/// the run recorded a distractor for an answer that never contained one.
///
/// Penalising that punishes exactly the multi-step behaviour a resolved index
/// exists to enable, and rewards a tool that discards its working. So the
/// answer is marked, the navigation is not, and the report shows both. What
/// `passed` means is deliberately unchanged — see `docs/DESIGN.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Evidence {
    /// This response is what the adapter is claiming as its answer.
    Answer,
    /// This response is how it found the thing it then answered about.
    Navigation,
}

/// Choose the search hit the scenario is actually asking about.
///
/// # Why not simply the top hit
///
/// Searching a bare name is what makes `search` match at all (see
/// [`NudoxAdapter::free_text_query`]), but a bare name is ambiguous: querying
/// `request` in `pypi:requests` returns 50 hits, and the first is
/// `'requests.sessions'.Session.request` — the method on `Session`, not
/// `requests.api.request`, the module-level function the scenario names.
/// Following the wrong identity then made `refs` return `(none)`, and the
/// scenario scored 29% on evidence that came only from the search listing.
///
/// A scenario that names a qualified `probe.symbol` has already said which
/// one it means, so the hit whose address ends with that path wins. Ranking
/// by *most trailing segments in common* rather than exact equality is what
/// makes this work across ecosystems, since an address carries an ecosystem
/// prefix, a version and a `[kind]` suffix that the scenario's spelling does
/// not.
///
/// With no `probe.symbol`, the server's own ranking is respected.
fn pick_hit<'a>(hits: &'a [Hit], scenario: &Scenario) -> Option<&'a Hit> {
    let Some(symbol) = scenario.probe.symbol.as_deref() else {
        return hits.first();
    };
    let wanted: Vec<String> = segments(symbol);
    if wanted.is_empty() {
        return hits.first();
    }
    let name = wanted.last().cloned().unwrap_or_default();

    // Score, then take the FIRST best rather than the last.
    //
    // Two things go wrong otherwise, and semantic ranking surfaced both.
    //
    // `max_by_key` returns the *last* maximum, so when nothing matches every
    // candidate ties at zero and the adapter follows whichever hit happens to
    // be last. Measured: searching `search_slice_with_raw` with the embedder
    // on returned fifty semantically-ranked hits that did not include the
    // exact match, and the adapter went to
    // `memmem::'impl Iterator for FindIter'` — an arbitrary tie-break — whose
    // `refs` were empty, scoring 0% on a scenario that passes structurally.
    //
    // So: a hit whose final segment *is* the name searched for wins outright
    // (if you asked for a name and something is called that, it is your
    // symbol), then the longest shared path suffix, and ties fall back to the
    // server's own ranking, which is the one piece of judgement the harness
    // has no business overriding.
    let mut best: Option<(&Hit, (u8, usize))> = None;
    for hit in hits {
        let text = hit
            .address
            .as_deref()
            .or(hit.key.as_deref())
            .unwrap_or_default();
        let parts = segments(text);
        let exact = u8::from(parts.last().is_some_and(|last| *last == name));
        let score = (exact, common_suffix_len(&parts, &wanted));
        if best.is_none_or(|(_, current)| score > current) {
            best = Some((hit, score));
        }
    }
    best.map(|(hit, _)| hit).or_else(|| hits.first())
}

/// Split an address or a qualified path into lowercase segments, dropping the
/// decoration an address carries and a scenario's spelling does not: the
/// `[kind]` suffix, a `@version`, and any quoting the renderer adds.
fn segments(text: &str) -> Vec<String> {
    // The `@version` run has to go *before* splitting, not after: a version
    // contains dots, so `memchr@2.8.3` would otherwise contribute the
    // segments `8` and `3` and wreck the suffix comparison.
    let text = text.split('[').next().unwrap_or(text);
    let mut cleaned = String::with_capacity(text.len());
    let mut in_version = false;
    for c in text.chars() {
        match c {
            '@' => in_version = true,
            ':' | '/' if in_version => {
                in_version = false;
                cleaned.push(c);
            }
            _ if in_version => {}
            _ => cleaned.push(c),
        }
    }

    cleaned
        .split(['.', '/', ':'])
        .map(|segment| {
            segment
                .trim_matches(|c: char| c == '\'' || c == '"' || c == '`')
                .to_ascii_lowercase()
        })
        .filter(|segment| !segment.is_empty())
        .collect()
}

/// How many trailing segments two paths share.
fn common_suffix_len(a: &[String], b: &[String]) -> usize {
    a.iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(x, y)| x == y)
        .count()
}

/// The bare identifier at the end of a qualified path.
///
/// `::`, `.` and `/` are all segment separators — the same three Nudox's own
/// address grammar treats as interchangeable, and the three that Rust, Python
/// and Go respectively spell a path with. A query with no separator is
/// returned unchanged, and free text (which contains spaces) is left alone
/// rather than being truncated to its last word.
fn last_segment(query: &str) -> String {
    if query.contains(char::is_whitespace) {
        return query.to_owned();
    }
    query
        .rsplit(['.', '/', ':'])
        .find(|segment| !segment.is_empty())
        .unwrap_or(query)
        .to_owned()
}

/// The pagination cursor a response offers, if it offers one.
///
/// The renderer writes it as a trailing ``next: `50` `` line, and the value is
/// opaque — it is copied back verbatim and never parsed or used to infer a
/// page size, which is the contract the tool's own error text states.
/// Scanning from the end matters: the accumulated context holds every page
/// fetched so far, and only the newest cursor is live.
fn next_cursor(text: &str) -> Option<String> {
    text.lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix("next:"))
        .map(|rest| rest.trim().trim_matches('`').trim().to_owned())
        .filter(|cursor| !cursor.is_empty())
}

/// Which package lineages the corpus holds, from a `packages` response.
///
/// Reads the Markdown, because that is what an agent sees. The renderer
/// writes each package as ``package: `cargo:memchr` `` — a version list that
/// is empty still produces that line, which is exactly the signal `packages`
/// documents for telling "not indexed here" apart from "does not exist".
fn parse_lineages(raw: &Value, markdown: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(entries) = raw
        .pointer("/structuredContent/packages")
        .and_then(Value::as_array)
    {
        for entry in entries {
            if let Some(lineage) = entry
                .get("lineage")
                .or_else(|| entry.get("package"))
                .and_then(Value::as_str)
            {
                out.push(lineage.to_owned());
            }
        }
    }
    if out.is_empty() {
        for line in markdown.lines() {
            let line = line.trim();
            let Some(rest) = line.strip_prefix("package: ") else {
                continue;
            };
            let lineage = rest.trim().trim_matches('`');
            if !lineage.is_empty() {
                out.push(lineage.to_owned());
            }
        }
    }
    out
}

/// A Trustfall query for the structural tasks.
///
/// These are written against the type, property and edge names the `schema`
/// card returns. The tool's own description is explicit that a guessed field
/// name is an error rather than an empty result, so a scenario whose query
/// does not compile shows up as a tool error in the transcript — visible,
/// not silently scored zero.
/// The bare name a structural query filters on.
fn probe_name(scenario: &Scenario) -> String {
    scenario
        .probe
        .name
        .clone()
        .or_else(|| {
            scenario
                .probe
                .symbol
                .as_ref()
                .and_then(|s| s.rsplit("::").next().map(str::to_owned))
        })
        .unwrap_or_default()
}

/// The Trustfall query for a task, together with the arguments it binds.
///
/// The two travel together because the engine rejects a call whose `args`
/// contain a name the query does not use — *"One or more of the provided
/// arguments are not used in this query"* — so a query and its bindings
/// cannot be maintained apart without drifting into that error.
fn graph_query_for(scenario: &Scenario) -> Option<(String, Value)> {
    match scenario.task {
        // `Trait.implementors` exists only on the concrete `Trait` type, so
        // the inline fragment is load-bearing, not decoration.
        TaskKind::Implementors => Some((
            r#"{
  Symbols {
    ... on Trait {
      name @filter(op: "=", value: ["$name"])
      implementors { key @output name @output path @output }
    }
  }
}"#
            .to_owned(),
            json!({ "name": probe_name(scenario) }),
        )),
        // Who references this declaration.
        //
        // `usages` is the reverse edge and lands on `Symbol` directly: the
        // declarations that use this one, which for a function are its
        // callers. Two wrong turns are recorded here so they are not retaken:
        // `occurrencesOf` is the *forward* edge and returns only the
        // declaration itself ("references I contain"), and `usages` has no
        // `owner` hop — that belongs to `Occurrence`, which this edge does
        // not yield.
        TaskKind::Callers => Some((
            r#"{
  Symbols {
    name @filter(op: "=", value: ["$name"])
    usages { key @output name @output kind @output path @output }
  }
}"#
            .to_owned(),
            json!({ "name": probe_name(scenario) }),
        )),
        // Every public declaration in the package.
        //
        // `search` cannot answer this: it ranks a name section built from
        // exact-and-prefix matches, so one query only ever reaches names
        // sharing a prefix with it — `memchr` finds `memchr2` and
        // `memchr_iter` and can never find `memrchr`. Entering at `Packages`
        // and walking `members` enumerates instead of matching.
        TaskKind::Overview => Some((
            r#"{
  Packages {
    name @filter(op: "=", value: ["$package"])
    members {
      isPublic @filter(op: "=", value: ["$public"])
      name @output
      kind @output
      path @output
    }
  }
}"#
            .to_owned(),
            json!({ "package": scenario.target.package.clone(), "public": "true" }),
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_identities_parse_back_out() {
        let markdown = "\
## Search (2)
~sem:ready
~scope:default (excluded Field, Variant, Param; pass `kinds` to include them)
cargo:memchr@2.8.3::memchr::memchr3[function]
  pub fn memchr3(n1: u8, n2: u8, n3: u8, haystack: &[u8]) -> Option<usize>
cargo:memchr#0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
  pub fn memchr3_iter(...) [semantic]
";
        let hits = parse_identities(markdown);
        assert_eq!(hits.len(), 2);
        assert_eq!(
            hits[0].address.as_deref(),
            Some("cargo:memchr@2.8.3::memchr::memchr3[function]")
        );
        assert!(hits[1].key.as_deref().unwrap().ends_with("abcdef"));
    }

    #[test]
    fn signals_and_headings_are_not_identities() {
        assert!(parse_identities("## Search (0)\n~sem:ready\n(no matches)\n").is_empty());
    }

    #[test]
    fn address_mode_picks_the_spelling_under_test() {
        let hit = Hit {
            key: Some("cargo:memchr#deadbeef".into()),
            address: Some("cargo:memchr@2.8.3::memchr::memchr3[function]".into()),
        };
        assert!(hit.identity(AddressMode::Address).unwrap().contains("::"));
        assert!(hit.identity(AddressMode::Key).unwrap().contains('#'));
    }

    #[test]
    fn loaded_lineages_parse_from_the_markdown() {
        let markdown = "\
## Packages · 2

package: `cargo:memchr`
| version |  | symbols |
| --- | --- | --- |
| 2.8.3 | current | 2129 |

package: `npm:commander`
| version |  | symbols |
| --- | --- | --- |
| 0.0.0 | current | 945 |
";
        let loaded = parse_lineages(&Value::Null, markdown);
        assert_eq!(loaded, vec!["cargo:memchr", "npm:commander"]);
    }

    #[test]
    fn an_unloaded_package_is_simply_absent_from_the_census() {
        let loaded = parse_lineages(&Value::Null, "## Packages · 0\n\n(none)\n");
        assert!(loaded.is_empty());
    }

    /// The cursor is read off the *last* `next:` line, because the context
    /// accumulates every page fetched so far and only the newest is live.
    /// Shape taken verbatim from the hashbrown diff payload in
    /// run-2026-08-30-165221.
    #[test]
    fn the_live_cursor_is_the_last_one_offered() {
        let one_page = "| decl | path | added |\n\nnext: `50`";
        assert_eq!(next_cursor(one_page).as_deref(), Some("50"));

        let two_pages = "page one\n\nnext: `50`\n\npage two\n\nnext: `100`";
        assert_eq!(next_cursor(two_pages).as_deref(), Some("100"));

        // A final page offers nothing, and neither does an empty marker.
        assert_eq!(next_cursor("summary: 50 changed\n| a | b |"), None);
        assert_eq!(next_cursor("next: ``"), None);
        assert_eq!(next_cursor(""), None);
    }

    /// `search` matches names, so a path must be reduced to the name at its
    /// end. The Python case is the one that was measured failing.
    #[test]
    fn a_path_query_is_reduced_to_its_name() {
        assert_eq!(last_segment("requests.api.request"), "request");
        assert_eq!(last_segment("memchr::arch::generic::search_slice_with_raw"),
                   "search_slice_with_raw");
        assert_eq!(last_segment("github.com/google/uuid/Parse"), "Parse");
        // Already bare: unchanged.
        assert_eq!(last_segment("search_slice_with_raw"), "search_slice_with_raw");
        // Free text is a question, not a path; truncating it would be worse.
        assert_eq!(last_segment("memchr public exports"), "memchr public exports");
    }

    /// The measured case: `request` in `pypi:requests` returns the `Session`
    /// method first, but the scenario names the module-level function.
    #[test]
    fn the_hit_matching_the_probe_symbol_wins_over_the_top_hit() {
        let hit = |address: &str| Hit {
            address: Some(address.to_owned()),
            key: None,
        };
        let hits = vec![
            hit("pypi:requests::'requests.sessions'.Session.request[function]"),
            hit("pypi:requests::'requests.api'.request[function]"),
        ];

        let mut scenario = crate::testing::scenario_with_truth(crate::scenario::Truth {
            mode: crate::scenario::TruthMode::SymbolSet,
            all_of: vec!["get".into()],
            any_of: Vec::new(),
            none_of: Vec::new(),
            oracle: None,
            sources: Vec::new(),
        });
        scenario.probe.symbol = Some("requests.api.request".into());
        let picked = pick_hit(&hits, &scenario).unwrap();
        assert_eq!(
            picked.address.as_deref(),
            Some("pypi:requests::'requests.api'.request[function]"),
            "the module-level function is the one the scenario names"
        );

        // With no qualified symbol, the server's own ranking stands.
        scenario.probe.symbol = None;
        assert_eq!(
            pick_hit(&hits, &scenario).unwrap().address.as_deref(),
            hits[0].address.as_deref()
        );
    }

    /// With the embedder on, a search for a name can come back ranked by
    /// similarity with the exact match nowhere in it. Every candidate then
    /// ties, and `max_by_key` would hand back the *last* one — which is how a
    /// scenario that passes structurally scored 0% on `refs` against
    /// `memmem::'impl Iterator for FindIter'`.
    #[test]
    fn a_tie_falls_back_to_the_servers_ranking() {
        let hit = |address: &str| Hit {
            address: Some(address.to_owned()),
            key: None,
        };
        let hits = vec![
            hit("cargo:memchr::arch::all::rabinkarp::'impl Finder'::find_raw[function]"),
            hit("cargo:memchr::memmem::'impl Iterator for FindIter'[impl]"),
        ];
        let mut scenario = crate::testing::scenario_with_truth(crate::scenario::Truth {
            mode: crate::scenario::TruthMode::SymbolSet,
            all_of: vec!["memchr::memchr".into()],
            any_of: Vec::new(),
            none_of: Vec::new(),
            oracle: None,
            sources: Vec::new(),
        });
        scenario.probe.symbol =
            Some("memchr::arch::generic::memchr::search_slice_with_raw".into());

        assert_eq!(
            pick_hit(&hits, &scenario).unwrap().address.as_deref(),
            hits[0].address.as_deref(),
            "nothing matches, so the server's first hit stands"
        );

        // And an exact name match beats everything, wherever it ranks.
        let mut with_exact = hits.clone();
        with_exact.push(hit("cargo:memchr::arch::generic::memchr::search_slice_with_raw[function]"));
        assert_eq!(
            pick_hit(&with_exact, &scenario).unwrap().address.as_deref(),
            with_exact[2].address.as_deref(),
            "the symbol actually named in the probe wins from last place"
        );
    }

    #[test]
    fn segments_drop_address_decoration() {
        assert_eq!(
            segments("pypi:requests::'requests.api'.request[function]"),
            vec!["pypi", "requests", "requests", "api", "request"]
        );
        assert_eq!(
            segments("cargo:memchr@2.8.3::memchr::memchr3[function]"),
            vec!["cargo", "memchr", "memchr", "memchr3"]
        );
    }

    #[test]
    fn policies_have_distinct_names() {
        let mut names: Vec<String> = Policy::all().into_iter().map(|p| p.name).collect();
        names.sort();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "ablation names collide in the report");
    }
}
