//! JSON-RPC 2.0 over the two MCP transports this harness needs.

use std::collections::HashMap;
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex;

use crate::tokens::TokenCount;

/// The MCP revision this client advertises.
const PROTOCOL_VERSION: &str = "2025-06-18";

/// How a client reaches its server.
#[derive(Clone, Debug)]
pub enum McpTransport {
    /// Streamable HTTP, optionally bearer-token gated. This is what Nudox
    /// serves: loopback only, ephemeral port, `Authorization: Bearer <token>`
    /// enforced in a layer in front of the MCP router.
    Http { url: String, token: Option<String> },
    /// A child process speaking newline-delimited JSON-RPC on stdio. Not used
    /// by Nudox; here so third-party servers drop in without a second client.
    Stdio {
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
    },
}

/// One tool as the server describes it, kept as raw JSON so its full
/// description text can be measured rather than summarized.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, rename = "inputSchema")]
    pub input_schema: Value,
}

impl ToolDescriptor {
    /// Everything about this tool that is resident in an agent's context
    /// before it calls anything: the description plus every `description`
    /// string inside its argument schema.
    pub fn resident_text(&self) -> String {
        let mut out = String::with_capacity(self.description.len() + 256);
        out.push_str(&self.name);
        out.push('\n');
        out.push_str(&self.description);
        out.push('\n');
        collect_schema_descriptions(&self.input_schema, &mut out);
        out
    }
}

/// Walk a JSON Schema and append every `description` value found. Argument
/// doc comments become schema descriptions, and MCP-SURFACE-PLAN §1.1 counts
/// them as part of the resident surface (1,115 cl100k tokens on the 13-tool
/// surface) — so they are counted here too.
fn collect_schema_descriptions(schema: &Value, out: &mut String) {
    match schema {
        Value::Object(map) => {
            for (key, value) in map {
                if key == "description" {
                    if let Some(text) = value.as_str() {
                        out.push_str(text);
                        out.push('\n');
                    }
                }
                collect_schema_descriptions(value, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_schema_descriptions(item, out);
            }
        }
        _ => {}
    }
}

/// What a `tools/call` returned, kept as the concatenated text the agent
/// would actually see.
#[derive(Clone, Debug)]
pub struct ToolResponse {
    pub tool: String,
    /// The flattened text of every content block, in order.
    pub text: String,
    /// The raw JSON-RPC `result`, kept for adapters that need structure.
    pub raw: Value,
    pub is_error: bool,
    pub elapsed: Duration,
    pub cost: TokenCount,
}

/// A connected MCP session.
pub struct McpClient {
    transport: Transport,
    next_id: std::sync::atomic::AtomicI64,
    tools: Vec<ToolDescriptor>,
    server_name: String,
}

/// Boxed because the two variants differ sharply in size — an HTTP session is
/// a client handle and two strings; a stdio session carries a child process
/// and two buffered pipes.
enum Transport {
    Http {
        client: reqwest::Client,
        url: String,
        token: Option<String>,
        session_id: Mutex<Option<String>>,
    },
    Stdio(Box<StdioSession>),
}

/// The state of a stdio MCP session.
struct StdioSession {
    /// Held so the child is not reaped while the session is open; the handle
    /// itself is never read.
    #[allow(dead_code)]
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<BufReader<ChildStdout>>,
}

impl McpClient {
    /// Connect, perform the MCP handshake, and cache the tool list.
    pub async fn connect(transport: McpTransport) -> Result<Self> {
        let transport = match transport {
            McpTransport::Http { url, token } => Transport::Http {
                client: reqwest::Client::builder()
                    .timeout(Duration::from_secs(300))
                    .build()
                    .context("building HTTP client")?,
                url,
                token,
                session_id: Mutex::new(None),
            },
            McpTransport::Stdio { command, args, env } => {
                let mut child = tokio::process::Command::new(&command)
                    .args(&args)
                    .envs(&env)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                    .with_context(|| format!("spawning MCP server `{command}`"))?;
                let stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
                let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
                Transport::Stdio(Box::new(StdioSession {
                    child: Mutex::new(child),
                    stdin: Mutex::new(stdin),
                    stdout: Mutex::new(BufReader::new(stdout)),
                }))
            }
        };

        let mut client = Self {
            transport,
            next_id: std::sync::atomic::AtomicI64::new(1),
            tools: Vec::new(),
            server_name: String::new(),
        };

        let init = client
            .request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": { "name": "nudox-bench", "version": env!("CARGO_PKG_VERSION") }
                }),
            )
            .await
            .context("MCP initialize")?;

        client.server_name = init
            .pointer("/serverInfo/name")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned();

        client.notify("notifications/initialized", json!({})).await.ok();

        let listed = client
            .request("tools/list", json!({}))
            .await
            .context("MCP tools/list")?;
        client.tools = serde_json::from_value(
            listed
                .get("tools")
                .cloned()
                .unwrap_or_else(|| Value::Array(vec![])),
        )
        .context("decoding tools/list")?;

        Ok(client)
    }

    /// The server's self-reported name.
    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// Every tool the server exposes.
    pub fn tools(&self) -> &[ToolDescriptor] {
        &self.tools
    }

    /// The fixed context cost of this surface: every tool description plus
    /// every argument-schema description, measured once.
    pub fn resident_cost(&self) -> TokenCount {
        let all: String = self
            .tools
            .iter()
            .map(|t| t.resident_text())
            .collect::<Vec<_>>()
            .join("\n");
        TokenCount::measure(&all)
    }

    /// Call a tool and measure what came back.
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<ToolResponse> {
        let started = Instant::now();
        let result = self
            .request(
                "tools/call",
                json!({ "name": name, "arguments": arguments }),
            )
            .await;
        let elapsed = started.elapsed();

        let raw = match result {
            Ok(raw) => raw,
            // A tool that errors is data, not a harness failure: an adapter
            // that cannot answer is a result worth recording.
            Err(e) => {
                let text = format!("tool call failed: {e}");
                return Ok(ToolResponse {
                    tool: name.to_owned(),
                    cost: TokenCount::measure(&text),
                    text,
                    raw: Value::Null,
                    is_error: true,
                    elapsed,
                });
            }
        };

        let is_error = raw
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let text = flatten_content(&raw);

        Ok(ToolResponse {
            tool: name.to_owned(),
            cost: TokenCount::measure(&text),
            text,
            raw,
            is_error,
            elapsed,
        })
    }

    fn next_id(&self) -> i64 {
        self.next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    async fn notify(&self, method: &str, params: Value) -> Result<()> {
        let body = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        match &self.transport {
            Transport::Http { .. } => {
                let _ = self.http_post(&body).await;
                Ok(())
            }
            Transport::Stdio(session) => {
                let stdin = &session.stdin;
                let mut line = serde_json::to_string(&body)?;
                line.push('\n');
                stdin.lock().await.write_all(line.as_bytes()).await?;
                Ok(())
            }
        }
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id();
        let body = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });

        let response: Value = match &self.transport {
            Transport::Http { .. } => self.http_post(&body).await?,
            Transport::Stdio(session) => {
                let (stdin, stdout) = (&session.stdin, &session.stdout);
                let mut line = serde_json::to_string(&body)?;
                line.push('\n');
                stdin.lock().await.write_all(line.as_bytes()).await?;
                stdin.lock().await.flush().await?;

                let mut reader = stdout.lock().await;
                loop {
                    let mut buf = String::new();
                    let read = reader.read_line(&mut buf).await?;
                    if read == 0 {
                        bail!("MCP server closed stdout before answering {method}");
                    }
                    let trimmed = buf.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let value: Value = match serde_json::from_str(trimmed) {
                        Ok(v) => v,
                        Err(_) => continue, // servers that chatter on stdout
                    };
                    // Skip notifications and other requests' answers.
                    if value.get("id").and_then(Value::as_i64) == Some(id) {
                        break value;
                    }
                }
            }
        };

        if let Some(error) = response.get("error") {
            bail!("{method}: {error}");
        }
        response
            .get("result")
            .cloned()
            .ok_or_else(|| anyhow!("{method}: response carried neither result nor error"))
    }

    async fn http_post(&self, body: &Value) -> Result<Value> {
        let Transport::Http {
            client,
            url,
            token,
            session_id,
        } = &self.transport
        else {
            unreachable!("http_post on a stdio transport")
        };

        let mut request = client
            .post(url)
            .header("Content-Type", "application/json")
            // Both are required: a streamable-HTTP server chooses per response
            // whether to answer with a JSON body or an SSE stream, and refuses
            // the request outright if the client did not offer both.
            .header("Accept", "application/json, text/event-stream")
            .json(body);

        if let Some(token) = token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        if let Some(sid) = session_id.lock().await.as_ref() {
            request = request.header("Mcp-Session-Id", sid);
        }

        let response = request.send().await.context("POST to MCP endpoint")?;
        let status = response.status();

        if let Some(sid) = response.headers().get("Mcp-Session-Id") {
            if let Ok(sid) = sid.to_str() {
                *session_id.lock().await = Some(sid.to_owned());
            }
        }

        let content_type = response
            .headers()
            .get("Content-Type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();
        let text = response.text().await.context("reading MCP response body")?;

        if status == reqwest::StatusCode::UNAUTHORIZED {
            bail!(
                "401 from {url}: the endpoint is token-gated and the token was missing or wrong. \
                 Nudox mints a fresh launch token per start; take it from the host's \
                 client-config snippet."
            );
        }
        if !status.is_success() && text.trim().is_empty() {
            bail!("HTTP {status} from {url} with an empty body");
        }

        if content_type.contains("text/event-stream") {
            parse_sse(&text)
        } else if text.trim().is_empty() {
            // Notifications legitimately answer 202 with no body.
            Ok(Value::Null)
        } else {
            serde_json::from_str(&text)
                .with_context(|| format!("decoding MCP response body: {}", truncate(&text, 400)))
        }
    }
}

/// Pull the last JSON-RPC message out of an SSE stream body.
///
/// The server may emit several `event:`/`data:` frames; the answer to the
/// request is the last `data:` payload that parses as JSON-RPC and is not a
/// notification.
fn parse_sse(body: &str) -> Result<Value> {
    let mut last: Option<Value> = None;
    for block in body.split("\n\n") {
        let mut data = String::new();
        for line in block.lines() {
            if let Some(rest) = line.strip_prefix("data:") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(rest.trim_start());
            }
        }
        if data.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(&data) {
            if value.get("id").is_some() {
                last = Some(value);
            }
        }
    }
    last.ok_or_else(|| anyhow!("SSE body carried no JSON-RPC response: {}", truncate(body, 400)))
}

/// Flatten MCP content blocks into the text an agent would read.
fn flatten_content(result: &Value) -> String {
    let mut out = String::new();
    if let Some(blocks) = result.get("content").and_then(Value::as_array) {
        for block in blocks {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    if let Some(text) = block.get("text").and_then(Value::as_str) {
                        out.push_str(text);
                        out.push('\n');
                    }
                }
                _ => {
                    // Non-text blocks still cost context; count their JSON.
                    out.push_str(&block.to_string());
                    out.push('\n');
                }
            }
        }
    }
    if out.is_empty() {
        // Structured-only results (`structuredContent`) still have to be read.
        if let Some(structured) = result.get("structuredContent") {
            out = serde_json::to_string_pretty(structured).unwrap_or_default();
        }
    }
    out.trim_end().to_owned()
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_owned()
    } else {
        format!("{}…", &s[..max])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_body_yields_the_response_frame() {
        let body = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"ok\":true}}\n\n";
        let value = parse_sse(body).expect("parses");
        assert_eq!(value["result"]["ok"], json!(true));
    }

    #[test]
    fn sse_ignores_notification_frames() {
        let body = "data: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}\n\n\
                    data: {\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{\"v\":1}}\n\n";
        let value = parse_sse(body).expect("parses");
        assert_eq!(value["id"], json!(7));
    }

    #[test]
    fn content_blocks_flatten_in_order() {
        let result = json!({"content":[{"type":"text","text":"a"},{"type":"text","text":"b"}]});
        assert_eq!(flatten_content(&result), "a\nb");
    }

    #[test]
    fn schema_descriptions_are_counted_as_resident() {
        let tool = ToolDescriptor {
            name: "search".into(),
            description: "find things".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "the text to search for" }
                }
            }),
        };
        let resident = tool.resident_text();
        assert!(resident.contains("find things"));
        assert!(
            resident.contains("the text to search for"),
            "argument doc comments are part of the resident surface"
        );
    }
}
