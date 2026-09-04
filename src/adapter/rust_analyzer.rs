//! rust-analyzer over LSP: the incumbent answer to "who calls this".
//!
//! # Why an LSP baseline belongs here
//!
//! `ripgrep` is the floor. rust-analyzer is the ceiling that already exists
//! on every Rust developer's machine: it resolves symbols properly, it knows
//! the difference between a call and a comment, and an agent can drive it.
//! A code-intelligence index that loses to `ripgrep` is broken; one that
//! merely *ties* rust-analyzer on Rust has to justify itself on the axes
//! rust-analyzer cannot reach at all — cross-package questions, versions it
//! does not have checked out, and the six other languages in the corpus.
//! Recording where it declines ([`Attempt::unsupported`]) is therefore as
//! much of the result as recording where it answers.
//!
//! # The two costs an LSP hides
//!
//! Neither shows up in a response-token count, so both are recorded
//! explicitly: **warm-up**, because rust-analyzer must build a crate graph
//! before it can answer anything, and **position resolution**, because
//! `textDocument/references` takes a file position rather than a name — so
//! an agent that knows only a symbol's name must first find where it is
//! written. That second call is a real cost of the interface and is charged
//! as a step.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};

use super::{Adapter, Attempt, AttemptBuilder};
use crate::scenario::{Scenario, TaskKind};

/// How long to wait for the crate graph before giving up on a package.
const INDEX_TIMEOUT: Duration = Duration::from_secs(180);
/// How long any single request may take once the server is warm.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Which rust-analyzer to run.
///
/// Configurable because `rustup` puts a *shim* named `rust-analyzer` on PATH
/// whether or not the component is installed for the active toolchain; the
/// shim exits with "Unknown binary" rather than failing to exist, so `which`
/// finds it and spawning it fails. Naming the real binary is the only
/// reliable fix, and which toolchain holds it varies per machine.
fn binary() -> String {
    std::env::var("RUST_ANALYZER_BIN").unwrap_or_else(|_| "rust-analyzer".to_owned())
}

/// A rust-analyzer process bound to one package root.
struct Server {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
    /// Kept for diagnostics when a session misbehaves; not read on the happy
    /// path.
    #[allow(dead_code)]
    root: PathBuf,
}

impl Server {
    async fn start(root: &Path) -> Result<Self> {
        let mut child = tokio::process::Command::new(binary())
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| {
                format!(
                    "spawning `{}` (set $RUST_ANALYZER_BIN if it is not on PATH; \
                     note that rustup installs a shim that fails when the component \
                     is absent from the active toolchain)",
                    binary()
                )
            })?;
        let stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
        let mut server = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
            root: root.to_path_buf(),
        };

        let root_uri = path_to_uri(root);
        server
            .request(
                "initialize",
                json!({
                    "processId": std::process::id(),
                    "rootUri": root_uri,
                    "workspaceFolders": [{ "uri": root_uri, "name": "target" }],
                    "capabilities": {
                        "workspace": { "symbol": { "dynamicRegistration": false } },
                        "textDocument": {
                            "references": { "dynamicRegistration": false },
                            "documentSymbol": { "hierarchicalDocumentSymbolSupport": true }
                        }
                    }
                }),
            )
            .await
            .context("LSP initialize")?;
        server.notify("initialized", json!({})).await?;
        Ok(server)
    }

    /// Block until rust-analyzer says it has finished building the crate
    /// graph, or until [`INDEX_TIMEOUT`].
    ///
    /// Returns how long the wait took, which is reported: a tool that answers
    /// in 8ms after a 90-second warm-up has an 90-second cost, and a
    /// benchmark that only timed the query would hide it.
    async fn await_ready(&mut self) -> Result<Duration> {
        let started = Instant::now();
        let deadline = started + INDEX_TIMEOUT;
        loop {
            if Instant::now() >= deadline {
                // Not fatal: rust-analyzer answers partially while indexing.
                return Ok(started.elapsed());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            let message = match tokio::time::timeout(remaining, self.read_message()).await {
                Ok(Ok(message)) => message,
                Ok(Err(e)) => return Err(e),
                Err(_) => return Ok(started.elapsed()),
            };
            if message.get("method").and_then(Value::as_str) == Some("$/progress") {
                let kind = message.pointer("/params/value/kind").and_then(Value::as_str);
                let token = message
                    .pointer("/params/token")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if kind == Some("end") && token.contains("Indexing") {
                    return Ok(started.elapsed());
                }
            }
        }
    }

    fn next_id(&mut self) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    async fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        let body = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        self.write(&body).await
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id();
        let body = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.write(&body).await?;

        let deadline = Instant::now() + REQUEST_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!("{method} timed out after {REQUEST_TIMEOUT:?}");
            }
            let message = tokio::time::timeout(remaining, self.read_message())
                .await
                .map_err(|_| anyhow!("{method} timed out"))??;
            if message.get("id").and_then(Value::as_i64) != Some(id) {
                continue; // progress notifications and server-initiated requests
            }
            if let Some(error) = message.get("error") {
                bail!("{method}: {error}");
            }
            return Ok(message.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    async fn write(&mut self, body: &Value) -> Result<()> {
        let payload = serde_json::to_string(body)?;
        self.stdin
            .write_all(format!("Content-Length: {}\r\n\r\n", payload.len()).as_bytes())
            .await?;
        self.stdin.write_all(payload.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    /// Read one LSP message: `Content-Length` header block, then that many
    /// bytes of JSON.
    async fn read_message(&mut self) -> Result<Value> {
        let mut length = None;
        loop {
            let mut line = Vec::new();
            loop {
                let mut byte = [0u8; 1];
                let read = self.stdout.read_exact(&mut byte).await;
                if read.is_err() {
                    bail!("rust-analyzer closed its stdout");
                }
                if byte[0] == b'\n' {
                    break;
                }
                if byte[0] != b'\r' {
                    line.push(byte[0]);
                }
            }
            let line = String::from_utf8_lossy(&line).into_owned();
            if line.is_empty() {
                break;
            }
            if let Some(value) = line.strip_prefix("Content-Length:") {
                length = value.trim().parse::<usize>().ok();
            }
        }
        let length = length.ok_or_else(|| anyhow!("LSP message without Content-Length"))?;
        let mut buf = vec![0u8; length];
        self.stdout.read_exact(&mut buf).await?;
        Ok(serde_json::from_slice(&buf)?)
    }

    async fn shutdown(mut self) {
        let _ = self.request("shutdown", Value::Null).await;
        let _ = self.notify("exit", Value::Null).await;
        let _ = self.child.kill().await;
    }
}

/// rust-analyzer, started per package root and reused across scenarios that
/// share one.
pub struct RustAnalyzerAdapter {
    server: Option<(PathBuf, Server)>,
    keep_responses: bool,
}

impl RustAnalyzerAdapter {
    pub fn new(keep_responses: bool) -> Self {
        Self {
            server: None,
            keep_responses,
        }
    }

    /// Get a server for `root`, starting one (and paying warm-up) if the
    /// current one is for a different package.
    async fn server_for(&mut self, root: &Path) -> Result<(&mut Server, Option<Duration>)> {
        let matches = matches!(&self.server, Some((current, _)) if current == root);
        let mut warmup = None;
        if !matches {
            if let Some((_, old)) = self.server.take() {
                old.shutdown().await;
            }
            let mut server = Server::start(root).await?;
            warmup = Some(server.await_ready().await?);
            self.server = Some((root.to_path_buf(), server));
        }
        let (_, server) = self.server.as_mut().expect("just installed");
        Ok((server, warmup))
    }
}

#[async_trait::async_trait]
impl Adapter for RustAnalyzerAdapter {
    fn name(&self) -> &str {
        "rust-analyzer"
    }

    fn description(&self) -> String {
        "rust-analyzer over LSP: workspace/symbol then textDocument/references".into()
    }

    async fn attempt(&mut self, scenario: &Scenario) -> Attempt {
        if crate::scenario::nudox_ecosystem_tag(&scenario.target.ecosystem) != "cargo" {
            return Attempt::unsupported(
                self.name(),
                scenario,
                format!(
                    "rust-analyzer does not speak {}; this is a real limit of the \
                     comparison, not a harness gap",
                    scenario.target.ecosystem
                ),
            );
        }
        let Some(root) = scenario.target.root.clone() else {
            return Attempt::unsupported(
                self.name(),
                scenario,
                "scenario has no target.root; an LSP server needs a checkout",
            );
        };
        if !root.join("Cargo.toml").exists() {
            return Attempt::unsupported(
                self.name(),
                scenario,
                format!("no Cargo.toml under {}", root.display()),
            );
        }
        if matches!(scenario.task, TaskKind::Diff) {
            return Attempt::unsupported(
                self.name(),
                scenario,
                "an LSP server answers about the checkout it has, so it cannot \
                 compare two versions",
            );
        }
        let Some(name) = scenario.probe.literal().map(str::to_owned) else {
            return Attempt::unsupported(self.name(), scenario, "no probe literal");
        };

        let mut builder =
            AttemptBuilder::new(self.name(), scenario, Default::default(), self.keep_responses);
        let mut error = None;

        let (server, warmup) = match self.server_for(&root).await {
            Ok(pair) => pair,
            Err(e) => {
                builder.step(
                    "start",
                    format!("rust-analyzer in {}", root.display()),
                    &format!("failed to start: {e:#}"),
                    Duration::ZERO,
                    true,
                );
                return builder.finish(Some(format!("{e:#}")));
            }
        };
        if let Some(warmup) = warmup {
            // Charged as a step with no response text: it costs wall clock,
            // not context, and both belong in the report.
            builder.step(
                "warm-up",
                format!("crate graph for {}", root.display()),
                "",
                warmup,
                false,
            );
        }

        // Step one: find the symbol. This is the call an agent must make
        // before it can ask a positional question at all.
        let started = Instant::now();
        let symbols = match server
            .request("workspace/symbol", json!({ "query": name }))
            .await
        {
            Ok(value) => value,
            Err(e) => {
                let message = format!("workspace/symbol: {e:#}");
                builder.step("workspace/symbol", name.clone(), &message, started.elapsed(), true);
                return builder.finish(Some(message));
            }
        };
        let rendered = render_symbols(&symbols, &root);
        builder.step(
            "workspace/symbol",
            format!("query={name}"),
            &rendered,
            started.elapsed(),
            false,
        );

        // Definition-shaped questions are answered by that alone.
        if matches!(
            scenario.task,
            TaskKind::Definition | TaskKind::Signature | TaskKind::Search | TaskKind::Overview
        ) {
            return builder.finish(error);
        }

        let Some(location) = first_location(&symbols, &name) else {
            return builder.finish(error);
        };

        if matches!(scenario.task, TaskKind::Callers) {
            let started = Instant::now();
            let uri = location.uri.clone();
            let file_path = uri_to_path(&uri);
            let text = file_path
                .as_ref()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .unwrap_or_default();

            // `references` needs a position on the identifier itself, and
            // `workspace/symbol` returns the range of the whole item. Find
            // the identifier's column on its line rather than trusting the
            // range start, which usually points at `pub`.
            let character = text
                .lines()
                .nth(location.line)
                .and_then(|line| line.find(&name))
                .unwrap_or(location.character) as u64;

            if !text.is_empty() {
                let _ = server
                    .notify(
                        "textDocument/didOpen",
                        json!({ "textDocument": {
                            "uri": uri, "languageId": "rust", "version": 1, "text": text
                        }}),
                    )
                    .await;
            }

            let params = json!({
                "textDocument": { "uri": uri },
                "position": { "line": location.line, "character": character },
                "context": { "includeDeclaration": false }
            });
            match server.request("textDocument/references", params.clone()).await {
                Ok(value) => {
                    let rendered = render_references(&value, &root);
                    builder.step(
                        "textDocument/references",
                        params.to_string(),
                        &rendered,
                        started.elapsed(),
                        false,
                    );
                }
                Err(e) => {
                    let message = format!("textDocument/references: {e:#}");
                    builder.step(
                        "textDocument/references",
                        params.to_string(),
                        &message,
                        started.elapsed(),
                        true,
                    );
                    error.get_or_insert(message);
                }
            }
        }

        builder.finish(error)
    }

    async fn shutdown(&mut self) {
        if let Some((_, server)) = self.server.take() {
            server.shutdown().await;
        }
    }
}

/// A resolved position in a file.
struct Located {
    uri: String,
    line: usize,
    character: usize,
}

fn first_location(symbols: &Value, name: &str) -> Option<Located> {
    let array = symbols.as_array()?;
    // Prefer an exact name match over a fuzzy one; `workspace/symbol` ranks
    // by its own heuristics and will happily return near-misses first.
    let exact = array
        .iter()
        .find(|s| s.get("name").and_then(Value::as_str) == Some(name));
    let chosen = exact.or_else(|| array.first())?;
    let location = chosen.get("location").or_else(|| chosen.get("range"))?;
    let uri = location.get("uri").and_then(Value::as_str)?.to_owned();
    let start = location.pointer("/range/start")?;
    Some(Located {
        uri,
        line: start.get("line").and_then(Value::as_u64)? as usize,
        character: start.get("character").and_then(Value::as_u64).unwrap_or(0) as usize,
    })
}

/// Render `workspace/symbol` output as the text an agent would read.
fn render_symbols(symbols: &Value, root: &Path) -> String {
    let Some(array) = symbols.as_array() else {
        return "(no symbols)".into();
    };
    if array.is_empty() {
        return "(no symbols)".into();
    }
    let mut out = String::new();
    for symbol in array {
        let name = symbol.get("name").and_then(Value::as_str).unwrap_or("?");
        let container = symbol
            .get("containerName")
            .and_then(Value::as_str)
            .unwrap_or("");
        let kind = symbol.get("kind").and_then(Value::as_u64).unwrap_or(0);
        let location = symbol
            .get("location")
            .and_then(|l| l.get("uri"))
            .and_then(Value::as_str)
            .map(|u| display_path(u, root))
            .unwrap_or_default();
        let line = symbol
            .pointer("/location/range/start/line")
            .and_then(Value::as_u64)
            .map(|l| l + 1)
            .unwrap_or(0);
        if container.is_empty() {
            out.push_str(&format!("{name}  [kind {kind}]  {location}:{line}\n"));
        } else {
            out.push_str(&format!(
                "{container}::{name}  [kind {kind}]  {location}:{line}\n"
            ));
        }
    }
    out
}

/// Render `textDocument/references` output, including the source line, which
/// is what an agent would have to fetch next anyway.
fn render_references(references: &Value, root: &Path) -> String {
    let Some(array) = references.as_array() else {
        return "(no references)".into();
    };
    if array.is_empty() {
        return "(no references)".into();
    }
    let mut out = String::new();
    for reference in array {
        let uri = reference.get("uri").and_then(Value::as_str).unwrap_or("");
        let line = reference
            .pointer("/range/start/line")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let path = display_path(uri, root);
        let source = uri_to_path(uri)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| text.lines().nth(line as usize).map(|l| l.trim().to_owned()))
            .unwrap_or_default();
        out.push_str(&format!("{path}:{}: {source}\n", line + 1));
    }
    out
}

fn display_path(uri: &str, root: &Path) -> String {
    match uri_to_path(uri) {
        Some(path) => path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string(),
        None => uri.to_owned(),
    }
}

fn path_to_uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

fn uri_to_path(uri: &str) -> Option<PathBuf> {
    uri.strip_prefix("file://").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_name_beats_fuzzy_rank() {
        let symbols = json!([
            { "name": "memchr3_iter", "location": { "uri": "file:///x.rs",
              "range": { "start": { "line": 4, "character": 7 } } } },
            { "name": "memchr", "location": { "uri": "file:///y.rs",
              "range": { "start": { "line": 9, "character": 7 } } } }
        ]);
        let located = first_location(&symbols, "memchr").expect("found");
        assert_eq!(located.line, 9, "exact match must win over the first row");
    }

    #[test]
    fn empty_results_render_as_a_statement_not_an_empty_string() {
        assert_eq!(render_symbols(&json!([]), Path::new("/")), "(no symbols)");
        assert_eq!(render_references(&json!([]), Path::new("/")), "(no references)");
    }

    #[test]
    fn uris_round_trip() {
        let path = PathBuf::from("/tmp/pkg/src/lib.rs");
        assert_eq!(uri_to_path(&path_to_uri(&path)), Some(path));
    }
}
