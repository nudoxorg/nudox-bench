//! The scenario model.
//!
//! A scenario is a question about a real, indexed package plus enough
//! machine-checkable truth to score an answer without an LLM in the loop.
//!
//! # Why scenarios are tool-agnostic
//!
//! The thing under test is not "does `search` work" — it is "can a coding
//! agent get this answer, and what does the answer cost". `ripgrep`,
//! rust-analyzer and Nudox reach the same fact by completely different
//! routes, so a scenario may not name a tool. It names the *question*
//! ([`TaskKind`]) and the *subject* ([`Probe`]); each adapter owns the
//! mapping from that pair onto its own surface. That mapping is the
//! per-adapter analogue of context-bench's hard-coded `MCP_MAPPINGS`, moved
//! out of the harness and into the thing being measured, which is where it
//! belongs when the adapters are not all MCP servers.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// What family of question this is. Reported separately, because a harness
/// that averages a retrieval score against an implementation score is
/// reporting a number with no referent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    /// "Where is X / who calls X / what implements X" — checkable exactly.
    Retrieval,
    /// "Using package X, build Y" — context-bench's shape. Judged.
    Implementation,
    /// Same fact, several routes; the score is cost-to-correct, not pass/fail.
    Efficiency,
    /// Retrieval, but deliberately outside Rust, to exercise other producers.
    Polyglot,
}

impl Family {
    /// Whether scoring this family needs a judge model.
    pub fn needs_judge(self) -> bool {
        matches!(self, Family::Implementation)
    }

    /// Stable short label for report columns.
    pub fn label(self) -> &'static str {
        match self {
            Family::Retrieval => "retrieval",
            Family::Implementation => "implementation",
            Family::Efficiency => "efficiency",
            Family::Polyglot => "polyglot",
        }
    }
}

/// The shape of the question, which is what an adapter dispatches on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    /// Find the declaration of a named symbol and show it.
    Definition,
    /// Who holds a resolved reference to this symbol.
    Callers,
    /// What this symbol's own body references.
    Outbound,
    /// Which types implement this trait / interface.
    Implementors,
    /// The exact signature, including resolved types.
    Signature,
    /// What changed between two versions of one package.
    Diff,
    /// What is in this package at all — top-level API surface.
    Overview,
    /// Fields, variants or methods owned by a type.
    Members,
    /// Free-text lookup where the expected hit set is known.
    Search,
    /// Write code against the package (judged, not matched).
    Implement,
}

impl TaskKind {
    /// Whether the task is inherently about one named symbol. Adapters use
    /// this to decide whether `probe.symbol` is required.
    pub fn is_symbol_scoped(self) -> bool {
        matches!(
            self,
            TaskKind::Definition
                | TaskKind::Callers
                | TaskKind::Outbound
                | TaskKind::Implementors
                | TaskKind::Signature
                | TaskKind::Members
        )
    }
}

/// Which package the question is about.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Target {
    /// `crates.io`, `npm`, `pypi`, `go`, `maven`, `nuget`, `cpp` — the
    /// ecosystem tags `nix/corpus.nix` uses.
    pub ecosystem: String,
    /// Package name as the registry spells it.
    pub package: String,
    /// Version the truth was recorded against. Required: a caller set is not
    /// a fact about a package, it is a fact about a version of one.
    pub version: String,
    /// Second version, for [`TaskKind::Diff`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_version: Option<String>,
    /// Where the source lives on this machine, if it has been fetched. Used
    /// by the file-based adapters (ripgrep, rust-analyzer), which cannot
    /// answer from an index they do not have.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<PathBuf>,
}

impl Target {
    /// The `ecosystem:name` lineage key Nudox's `packages` filter accepts.
    /// `crates.io` is spelled `cargo` in a Nudox address.
    pub fn lineage(&self) -> String {
        format!("{}:{}", nudox_ecosystem_tag(&self.ecosystem), self.package)
    }

    /// A version-pinned Nudox package coordinate: `cargo:memchr@2.8.3`.
    pub fn coord(&self) -> String {
        format!("{}@{}", self.lineage(), self.version)
    }
}

/// Map a corpus ecosystem tag onto the tag a Nudox address uses.
///
/// `nix/corpus.nix` says `crates.io`; a Nudox address says `cargo`. The
/// address parser also normalizes `golang`→`go` and `clang`→`cpp` on input
/// but never emits them, so those are folded here too.
pub fn nudox_ecosystem_tag(corpus_tag: &str) -> &str {
    match corpus_tag {
        "crates.io" | "cargo" => "cargo",
        "golang" | "go" => "go",
        "clang" | "cpp" | "c++" => "cpp",
        other => other,
    }
}

/// The subject of the question, in whatever forms are known.
///
/// Several forms are carried at once on purpose: `ripgrep` wants a literal
/// string, rust-analyzer wants a bare identifier plus a file position, and
/// Nudox wants an address. Making the scenario carry all three is what keeps
/// the adapters from having to guess-and-transform each other's spellings.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Probe {
    /// The natural-language or keyword query, as an agent would type it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// Bare identifier, e.g. `memchr3`. What ripgrep and `workspace/symbol`
    /// take.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Fully-qualified symbol path without the package coordinate, e.g.
    /// `memchr::memchr3`. Combined with [`Target::coord`] this is a Nudox
    /// address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Symbol kind qualifier where the path alone is ambiguous, e.g.
    /// `function`, `method`, `trait`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

impl Probe {
    /// The best available literal to hand a text search.
    pub fn literal(&self) -> Option<&str> {
        self.name
            .as_deref()
            .or_else(|| self.symbol.as_deref().and_then(|s| s.rsplit("::").next()))
            .or(self.query.as_deref())
    }
}

/// How an answer is checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TruthMode {
    /// The answer must mention this set of symbols. Scored by recall over
    /// `all_of` and precision against `none_of`.
    SymbolSet,
    /// The answer must contain these literal strings (a signature fragment,
    /// a type name, a version string).
    Literal,
    /// No mechanical truth: a judge decides. Used by
    /// [`Family::Implementation`].
    Judged,
}

/// The machine-checkable answer.
///
/// # On `none_of`
///
/// A retrieval tool can score perfect recall by returning the whole package.
/// `none_of` names symbols that a *correct, narrow* answer would not contain
/// — near-misses, same-named symbols in other modules, the classic
/// text-search false positives. It is what stops "dump everything" from
/// winning, and it is why a text-search baseline is worth running: without
/// `none_of` the ripgrep adapter would look far better than it is.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Truth {
    pub mode: TruthMode,
    /// Every one of these must appear for full recall.
    #[serde(default)]
    pub all_of: Vec<String>,
    /// At least one of these must appear. Used where the corpus offers
    /// several equally-correct spellings of the same fact.
    #[serde(default)]
    pub any_of: Vec<String>,
    /// None of these may appear in a clean answer.
    #[serde(default)]
    pub none_of: Vec<String>,
    /// For [`Family::Implementation`]: path to oracle code, relative to the
    /// repo root, exactly as context-bench models it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oracle: Option<PathBuf>,
    /// Documentation the oracle was written from. Recorded for provenance;
    /// never fetched by the harness.
    #[serde(default)]
    pub sources: Vec<String>,
}

impl Truth {
    /// Total number of facts that must be present for full recall.
    pub fn required_count(&self) -> usize {
        self.all_of.len() + usize::from(!self.any_of.is_empty())
    }
}

/// Per-scenario cost ceilings. Exceeding one is not a failure; it is
/// recorded, and the efficiency family scores against it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Budget {
    /// How many tool calls an agent-mode run may spend.
    #[serde(default = "default_max_tool_calls")]
    pub max_tool_calls: u32,
    /// Response tokens above which the answer is considered expensive.
    #[serde(default = "default_token_budget")]
    pub token_budget: u32,
}

fn default_max_tool_calls() -> u32 {
    8
}

fn default_token_budget() -> u32 {
    6_000
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_tool_calls: default_max_tool_calls(),
            token_budget: default_token_budget(),
        }
    }
}

/// One benchmark scenario.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scenario {
    /// Stable id, `<language>/<package>/<slug>`. Used in report keys and on
    /// the command line, so it must not change once published.
    pub id: String,
    pub family: Family,
    pub task: TaskKind,
    /// The question, phrased as a developer would ask it. Shown to judges
    /// and to agent-mode drivers verbatim.
    pub question: String,
    pub target: Target,
    #[serde(default)]
    pub probe: Probe,
    pub truth: Truth,
    #[serde(default)]
    pub budget: Budget,
    /// Free-text note on why this scenario is interesting or what it is
    /// known to be sensitive to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Filled in by the loader; not present in the TOML.
    #[serde(skip)]
    pub source_path: PathBuf,
}

impl Scenario {
    /// Reject scenarios that cannot be scored as written. Called on load, so
    /// a malformed scenario fails the run rather than silently scoring zero
    /// for every adapter — which would look like a finding.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() {
            return Err("empty id".into());
        }
        if self.question.trim().is_empty() {
            return Err(format!("{}: empty question", self.id));
        }
        match self.truth.mode {
            TruthMode::Judged => {
                if self.truth.oracle.is_none() {
                    return Err(format!("{}: judged truth needs an oracle path", self.id));
                }
            }
            TruthMode::SymbolSet | TruthMode::Literal => {
                if self.truth.all_of.is_empty() && self.truth.any_of.is_empty() {
                    return Err(format!(
                        "{}: {:?} truth with no all_of and no any_of can never fail, \
                         which makes it worthless as a measurement",
                        self.id, self.truth.mode
                    ));
                }
            }
        }
        if self.task.is_symbol_scoped() && self.probe.symbol.is_none() && self.probe.name.is_none()
        {
            return Err(format!(
                "{}: task {:?} is about one symbol but probe names none",
                self.id, self.task
            ));
        }
        if self.task == TaskKind::Diff && self.target.to_version.is_none() {
            return Err(format!("{}: diff task needs target.to_version", self.id));
        }
        if self.family.needs_judge() && self.truth.mode != TruthMode::Judged {
            return Err(format!(
                "{}: implementation family must use judged truth",
                self.id
            ));
        }
        Ok(())
    }
}

/// Expand `~` and `${VAR}` in a path.
///
/// # Why scenario roots need this
///
/// A cargo registry checkout lives at
/// `~/.cargo/registry/src/index.crates.io-<hash>/memchr-2.8.3`, and that hash
/// is not stable across machines or cargo versions. A scenario that hard-codes
/// one is a scenario only its author can run, which for a benchmark is the
/// same as a scenario nobody can reproduce. So roots are written with
/// variables — `${CARGO_SRC}/memchr-2.8.3` — and resolved here.
///
/// An unset variable is left untouched rather than expanded to nothing: a path
/// containing a literal `${CARGO_SRC}` fails loudly at `root.exists()` with the
/// name of what was missing still in the message, where expanding to `""`
/// would silently produce `/memchr-2.8.3` and report a confusing absence.
/// Well-known package caches this harness can find on its own.
///
/// These are the roots every scenario in this repo points at, and they live in
/// a predictable place on a developer machine — but a place with a
/// machine-specific component (cargo interposes a registry-index hash, Python
/// a minor version). Resolving them here means the committed scenarios run
/// unmodified on a machine that is not the author's, which is the difference
/// between a benchmark and a demo.
///
/// An explicitly-set environment variable always wins, so a caller pointing at
/// a vendored corpus is never overruled by a lucky guess.
fn builtin_root(name: &str) -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    match name {
        // ~/.cargo/registry/src/<index-hash>/ — the hash varies with the
        // registry protocol and cargo version, so it is globbed rather than
        // spelled. Newest entry wins when several indexes are present.
        "CARGO_SRC" => newest_child(&format!("{home}/.cargo/registry/src")),
        "GO_MOD" => exists(format!("{home}/go/pkg/mod")),
        // /usr/lib/python3.X/site-packages — highest version present.
        "PY_SITE" => newest_child("/usr/lib")
            .map(|dir| format!("{dir}/site-packages"))
            .filter(|path| std::path::Path::new(path).is_dir())
            .or_else(|| {
                newest_matching("/usr/lib", "python")
                    .map(|dir| format!("{dir}/site-packages"))
                    .filter(|path| std::path::Path::new(path).is_dir())
            }),
        _ => None,
    }
}

fn exists(path: String) -> Option<String> {
    std::path::Path::new(&path).is_dir().then_some(path)
}

fn newest_child(dir: &str) -> Option<String> {
    newest_matching(dir, "")
}

/// The lexically-greatest directory under `dir` whose name starts with
/// `prefix`. Lexical order is the right tie-break for both cases here:
/// `python3.14` sorts above `python3.13`, and any cargo index hash works.
fn newest_matching(dir: &str, prefix: &str) -> Option<String> {
    let mut best: Option<String> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(prefix) {
            continue;
        }
        let path = entry.path().to_string_lossy().into_owned();
        if best.as_deref().is_none_or(|current| current < path.as_str()) {
            best = Some(path);
        }
    }
    best
}

pub fn expand_path(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;

    if let Some(stripped) = rest.strip_prefix("~/") {
        match std::env::var("HOME") {
            Ok(home) => {
                out.push_str(&home);
                out.push('/');
                rest = stripped;
            }
            Err(_) => {}
        }
    }

    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                let name = &after[..end];
                match std::env::var(name).ok().or_else(|| builtin_root(name)) {
                    Some(value) => out.push_str(&value),
                    // Left verbatim on purpose — see the doc comment.
                    None => {
                        out.push_str("${");
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Load every `*.toml` under `dir`, recursively.
pub fn load_dir(dir: &Path) -> anyhow::Result<Vec<Scenario>> {
    let mut out = Vec::new();
    let mut problems = Vec::new();
    for entry in walkdir::WalkDir::new(dir).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let text = std::fs::read_to_string(path)?;
        let mut scenario: Scenario = toml::from_str(&text)
            .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
        scenario.source_path = path.to_path_buf();
        if let Some(root) = scenario.target.root.take() {
            let expanded = expand_path(&root.to_string_lossy());
            scenario.target.root = Some(PathBuf::from(expanded));
        }
        if let Err(problem) = scenario.validate() {
            problems.push(problem);
            continue;
        }
        out.push(scenario);
    }
    if !problems.is_empty() {
        anyhow::bail!("invalid scenarios:\n  {}", problems.join("\n  "));
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilde_and_variables_expand() {
        std::env::set_var("NBENCH_TEST_ROOT", "/opt/pkgs");
        assert_eq!(
            expand_path("${NBENCH_TEST_ROOT}/memchr-2.8.3"),
            "/opt/pkgs/memchr-2.8.3"
        );
    }

    #[test]
    fn an_unset_variable_is_left_verbatim() {
        // Expanding to "" would turn this into "/memchr-2.8.3", which exists
        // nowhere and names nothing — the failure would not say what was
        // missing.
        let expanded = expand_path("${NBENCH_DEFINITELY_UNSET}/memchr-2.8.3");
        assert!(
            expanded.contains("NBENCH_DEFINITELY_UNSET"),
            "the missing variable must survive into the error path: {expanded}"
        );
    }

    #[test]
    fn a_plain_path_is_unchanged() {
        assert_eq!(expand_path("/usr/lib/python3.14/site-packages/requests"),
                   "/usr/lib/python3.14/site-packages/requests");
    }

    #[test]
    fn multiple_variables_in_one_path() {
        std::env::set_var("NBENCH_A", "/a");
        std::env::set_var("NBENCH_B", "b");
        assert_eq!(expand_path("${NBENCH_A}/x/${NBENCH_B}"), "/a/x/b");
    }
}

#[cfg(test)]
mod builtin_tests {
    use super::*;

    #[test]
    fn an_explicit_variable_beats_the_builtin_guess() {
        std::env::set_var("CARGO_SRC", "/vendored/corpus");
        assert_eq!(expand_path("${CARGO_SRC}/memchr-2.8.3"), "/vendored/corpus/memchr-2.8.3");
        std::env::remove_var("CARGO_SRC");
    }

    #[test]
    fn unknown_names_have_no_builtin() {
        assert_eq!(builtin_root("NOT_A_KNOWN_ROOT"), None);
    }
}
