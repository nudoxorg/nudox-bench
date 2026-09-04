//! `nbench` — the command line.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use nudox_bench::config::{AdapterConfig, RunConfig};
use nudox_bench::mcp::{McpClient, McpTransport};
use nudox_bench::runner::{self, RunOptions};
use nudox_bench::scenario;
use nudox_bench::tokens::TokenCount;
use nudox_bench::verify::Stability;

#[derive(Parser)]
#[command(name = "nbench", about = "Benchmark the Nudox code-intelligence surface")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run scenarios against the adapters a config declares.
    Run {
        #[arg(long, default_value = "configs/default.toml")]
        config: PathBuf,
        #[arg(long, default_value = "scenarios")]
        scenarios: PathBuf,
        /// Only scenarios whose id contains this substring.
        #[arg(long)]
        filter: Option<String>,
        #[arg(long, default_value = "runs")]
        out: PathBuf,
        /// Keep every response body in report.json.
        #[arg(long)]
        keep_responses: bool,
        /// Judge implementation scenarios (needs OPENROUTER_API_KEY).
        #[arg(long)]
        judge: bool,
        /// Index each scenario's `target.root` into the Nudox corpus before
        /// running, so Nudox and the file-based adapters answer about the
        /// same code. Slow — it runs a real compiler front end per package.
        #[arg(long)]
        ensure_indexed: bool,
    },
    /// Measure a live MCP surface's fixed cost without running any scenario.
    ///
    /// This is the fastest useful thing in the repo: it answers "what does
    /// this tool surface cost an agent before it asks anything", which is the
    /// number `Backend/docs/MCP-SURFACE-PLAN.md` §1.1 tracks.
    Surface {
        #[arg(long, env = "NUDOX_MCP_URL")]
        url: String,
        #[arg(long, env = "NUDOX_MCP_TOKEN")]
        token: Option<String>,
        /// Print the per-tool breakdown, largest first.
        #[arg(long)]
        per_tool: bool,
    },
    /// Check that everything a run needs is actually present.
    ///
    /// Reports, per scenario, whether its package root resolved on this
    /// machine; whether `rg` and rust-analyzer are real binaries rather than
    /// a rustup shim; and whether the Nudox endpoint answers. Exits non-zero
    /// if anything a run would need is missing, so it works in CI.
    Doctor {
        #[arg(long, default_value = "scenarios")]
        scenarios: PathBuf,
        /// Check the adapters this config declares, rather than the standard
        /// set. Without it `doctor` surveys every tool the repo knows about,
        /// which answers "what is installed" but not "will this run work".
        #[arg(long)]
        config: Option<PathBuf>,
        /// Also try the Nudox endpoint.
        #[arg(long, env = "NUDOX_MCP_URL")]
        url: Option<String>,
        #[arg(long, env = "NUDOX_MCP_TOKEN")]
        token: Option<String>,
    },
    /// Re-run a config several times and report what did not reproduce.
    ///
    /// Every claim this repo makes rests on the scoring being deterministic,
    /// and nothing else measures that: a single report cannot tell a verdict
    /// that holds from one that alternates. A verdict that moves is an alarm.
    /// A cost that moves is ordinary for anything crossing a process or a
    /// network and is reported as a spread.
    Verify {
        #[arg(long, default_value = "configs/default.toml")]
        config: PathBuf,
        #[arg(long, default_value = "scenarios")]
        scenarios: PathBuf,
        /// Only scenarios whose id contains this substring.
        #[arg(long)]
        filter: Option<String>,
        /// How many times to run. Two is the minimum that can disagree.
        #[arg(long, default_value_t = 3)]
        repeat: usize,
        /// Report a cost as drifting past this fraction of its own floor.
        #[arg(long, default_value_t = 0.05)]
        drift: f64,
    },
    /// Load and validate every scenario, then list them.
    List {
        #[arg(long, default_value = "scenarios")]
        scenarios: PathBuf,
    },
}

/// Run `bin args…` and return its first line of output, or `None` if it
/// cannot be run at all.
///
/// Checking that the binary *answers* rather than merely exists is the point:
/// `rustup` puts a shim named `rust-analyzer` on PATH whether or not the
/// component is installed, so `which` succeeds where spawning fails.
async fn which_ok(bin: &str, args: &[&str]) -> Option<String> {
    let output = tokio::process::Command::new(bin)
        .args(args)
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Some(text.lines().next().unwrap_or("").trim().to_owned())
}

/// Check exactly the adapters one config declares, and return how many of
/// them a run could not use.
///
/// # Why this is not the same as the standard survey
///
/// `doctor` without a config reports what is installed on this machine. That
/// answers a different question from the one someone is usually asking, which
/// is whether *this run* will work. A config naming only `ripgrep` should not
/// fail because rust-analyzer is absent, and a config naming
/// `nudox-canonical` should say so when the binary it needs was never built.
async fn check_declared(config: &RunConfig) -> usize {
    let mut problems = 0usize;
    for adapter in &config.adapters {
        match adapter {
            AdapterConfig::Ripgrep => match which_ok("rg", &["--version"]).await {
                Some(version) => println!("  ok      ripgrep — {version}"),
                None => {
                    problems += 1;
                    println!("  MISSING ripgrep: `rg` is not runnable");
                }
            },
            AdapterConfig::RustAnalyzer => {
                let bin = std::env::var("RUST_ANALYZER_BIN")
                    .unwrap_or_else(|_| "rust-analyzer".to_owned());
                match which_ok(&bin, &["--version"]).await {
                    Some(version) => println!("  ok      rust-analyzer — {version}"),
                    None => {
                        problems += 1;
                        println!(
                            "  MISSING `{bin}` is not runnable.\n          \
                             rustup component add rust-analyzer\n          \
                             or export RUST_ANALYZER_BIN=/path/to/rust-analyzer"
                        );
                    }
                }
            }
            AdapterConfig::Nudox { url, token_env, policy } => {
                let policy = policy.as_deref().unwrap_or("full");
                let Some(url) = url
                    .clone()
                    .or_else(|| std::env::var("NUDOX_MCP_URL").ok())
                else {
                    problems += 1;
                    println!(
                        "  MISSING nudox ({policy}): no endpoint. Set `url` in the config \
                         or $NUDOX_MCP_URL"
                    );
                    continue;
                };
                let token_var = token_env.as_deref().unwrap_or("NUDOX_MCP_TOKEN");
                let token = std::env::var(token_var).ok().filter(|t| !t.trim().is_empty());
                match McpClient::connect(McpTransport::Http { url: url.clone(), token }).await {
                    Ok(client) => println!(
                        "  ok      nudox ({policy}) — {}, {} tools, {} cl100k resident",
                        client.server_name(),
                        client.tools().len(),
                        client.resident_cost().cl100k
                    ),
                    Err(error) => {
                        problems += 1;
                        println!("  FAILED  nudox ({policy}) at {url}: {error:#}");
                    }
                }
            }
            AdapterConfig::NudoxCanonical { binary, .. } => {
                let Some(binary) = binary
                    .clone()
                    .or_else(|| std::env::var("NUDOX_CANONICAL_CLI").ok())
                else {
                    problems += 1;
                    println!(
                        "  MISSING nudox-canonical: no CLI. Set `binary` in the config or \
                         $NUDOX_CANONICAL_CLI\n          \
                         (cargo build -p interface-cli, in the Backend checkout)"
                    );
                    continue;
                };
                let Some(reply) = which_ok(&binary, &["health", "1"]).await else {
                    problems += 1;
                    println!("  MISSING nudox-canonical: `{binary}` is not runnable");
                    continue;
                };
                // The binary answering is not the same as the capabilities
                // being present: canonical builds and runs with its whole
                // index plane unwired, and says so only in this roll-call.
                let down = unavailable_capabilities(&reply);
                if down.is_empty() {
                    println!("  ok      nudox-canonical — every capability ready");
                } else {
                    println!(
                        "  ok      nudox-canonical — runs, but {} unavailable: {}",
                        down.len(),
                        down.join(", ")
                    );
                    if down.iter().any(|c| c == "index") {
                        println!(
                            "          every scenario will report `unsupported`; the index \
                             plane is not wired to interface-core"
                        );
                    }
                }
            }
            AdapterConfig::McpStdio { name, .. } => {
                problems += 1;
                println!("  MISSING adapter `{name}`: the generic stdio MCP adapter is declared but not wired yet");
            }
        }
    }
    problems
}

/// Capability names a canonical health reply reports as anything but ready.
///
/// Returns nothing for a reply this cannot parse, because guessing at an
/// unfamiliar envelope would turn a surface change into a false alarm.
fn unavailable_capabilities(reply: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(reply) else {
        return Vec::new();
    };
    let Some(facts) = value
        .get("body")
        .and_then(|body| body.get("facts"))
        .and_then(|facts| facts.as_array())
    else {
        return Vec::new();
    };
    facts
        .iter()
        .filter(|fact| fact.get("state").and_then(|s| s.as_str()) != Some("local_ready"))
        .filter_map(|fact| {
            fact.get("capability")
                .and_then(|c| c.as_str())
                .map(str::to_owned)
        })
        .collect()
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "nudox_bench=info,nbench=info".into()),
        )
        .with_target(false)
        .init();

    match Cli::parse().command {
        Command::Run {
            config,
            scenarios: scenario_dir,
            filter,
            out,
            keep_responses,
            judge,
            ensure_indexed,
        } => {
            let config = RunConfig::load(&config)?;
            let mut scenarios = scenario::load_dir(&scenario_dir)
                .with_context(|| format!("loading scenarios from {}", scenario_dir.display()))?;
            if let Some(filter) = &filter {
                scenarios.retain(|s| s.id.contains(filter.as_str()));
            }
            if scenarios.is_empty() {
                anyhow::bail!("no scenarios matched");
            }

            let run_id = format!(
                "run-{}",
                chrono::Local::now().format("%Y-%m-%d-%H%M%S")
            );
            println!(
                "config `{}`: {} scenario(s) × {} adapter(s)",
                config.name,
                scenarios.len(),
                config.adapters.len()
            );

            let adapters =
                runner::build_adapters(&config, keep_responses, ensure_indexed).await?;
            let options = RunOptions {
                run_id,
                out_dir: out.clone(),
                keep_responses,
                judge,
                ensure_indexed,
            };
            let report = runner::run(&scenarios, adapters, &options).await?;
            let dir = runner::write_report(&report, &out)?;

            println!("\n{}", report.to_markdown());
            println!("written to {}", dir.display());
        }

        Command::Surface {
            url,
            token,
            per_tool,
        } => {
            let client = McpClient::connect(McpTransport::Http {
                url: url.clone(),
                token,
            })
            .await
            .with_context(|| format!("connecting to {url}"))?;

            let resident = client.resident_cost();
            println!("server:  {}", client.server_name());
            println!("tools:   {}", client.tools().len());
            println!(
                "resident before any call: {} bytes | {} cl100k | {} o200k",
                resident.bytes, resident.cl100k, resident.o200k
            );
            if resident.vocabularies_disagree() {
                println!("  note: the two tokenizers disagree by >5%; quote both.");
            }

            if per_tool {
                let mut rows: Vec<(String, TokenCount)> = client
                    .tools()
                    .iter()
                    .map(|t| (t.name.clone(), TokenCount::measure(&t.resident_text())))
                    .collect();
                rows.sort_by_key(|(_, cost)| std::cmp::Reverse(cost.cl100k));
                println!("\n| tool | cl100k | o200k | bytes |");
                println!("|---|---:|---:|---:|");
                for (name, cost) in rows {
                    println!(
                        "| {name} | {} | {} | {} |",
                        cost.cl100k, cost.o200k, cost.bytes
                    );
                }
            }
        }

        Command::Doctor {
            scenarios: scenario_dir,
            config,
            url,
            token,
        } => {
            let mut problems = 0usize;

            println!("scenarios");
            let scenarios = scenario::load_dir(&scenario_dir)?;
            println!("  {} load and validate", scenarios.len());

            println!("\npackage roots");
            let mut seen: Vec<(String, bool)> = Vec::new();
            for scenario in &scenarios {
                let Some(root) = &scenario.target.root else {
                    continue;
                };
                let rendered = root.display().to_string();
                if seen.iter().any(|(path, _)| *path == rendered) {
                    continue;
                }
                let present = root.is_dir();
                seen.push((rendered.clone(), present));
                if present {
                    println!("  ok      {rendered}");
                } else {
                    problems += 1;
                    // An unexpanded ${VAR} survives into this message on
                    // purpose, so the fix is visible in the failure.
                    println!("  MISSING {rendered}");
                }
            }
            let unresolved: Vec<&str> = seen
                .iter()
                .filter(|(_, present)| !present)
                .map(|(path, _)| path.as_str())
                .filter(|path| path.contains("${"))
                .collect();
            if !unresolved.is_empty() {
                println!(
                    "\n  Set the named variable(s), e.g.:\n    \
                     export NODE_MODULES=/path/to/node_modules"
                );
            }

            if let Some(path) = &config {
                let declared = RunConfig::load(path)?;
                println!("\nadapters declared by {}", path.display());
                problems += check_declared(&declared).await;

                println!("\njudge");
                if std::env::var("OPENROUTER_API_KEY").is_ok() {
                    println!("  ok      OPENROUTER_API_KEY is set; --judge will run");
                } else {
                    println!(
                        "  absent  OPENROUTER_API_KEY unset — implementation scenarios \
                         report as `unscored`, which is not a failure"
                    );
                }
                if problems > 0 {
                    println!(
                        "\n{problems} problem(s). A run will still work for whatever is ok above."
                    );
                    std::process::exit(1);
                }
                println!("\nall good.");
                return Ok(());
            }

            println!("\nbaseline tools");
            match which_ok("rg", &["--version"]).await {
                Some(version) => println!("  ok      rg — {version}"),
                None => {
                    problems += 1;
                    println!("  MISSING rg (ripgrep) is not runnable");
                }
            }
            let ra = std::env::var("RUST_ANALYZER_BIN")
                .unwrap_or_else(|_| "rust-analyzer".to_owned());
            match which_ok(&ra, &["--version"]).await {
                Some(version) => println!("  ok      rust-analyzer — {version}"),
                None => {
                    problems += 1;
                    println!(
                        "  MISSING `{ra}` is not runnable.\n          \
                         rustup installs a shim that fails when the component is absent:\n          \
                         rustup component add rust-analyzer\n          \
                         or export RUST_ANALYZER_BIN=/path/to/rust-analyzer"
                    );
                }
            }

            println!("\nnudox endpoint");
            match url {
                None => println!(
                    "  skipped — set NUDOX_MCP_URL to check it\n            \
                     (start one with: cd endpoint && cargo run --release)"
                ),
                Some(url) => {
                    match McpClient::connect(McpTransport::Http {
                        url: url.clone(),
                        token,
                    })
                    .await
                    {
                        Ok(client) => {
                            let resident = client.resident_cost();
                            println!(
                                "  ok      {} — {} tools, {} cl100k resident",
                                client.server_name(),
                                client.tools().len(),
                                resident.cl100k
                            );
                        }
                        Err(e) => {
                            problems += 1;
                            println!("  FAILED  {url}: {e:#}");
                        }
                    }
                }
            }

            println!("\njudge");
            if std::env::var("OPENROUTER_API_KEY").is_ok() {
                println!("  ok      OPENROUTER_API_KEY is set; --judge will run");
            } else {
                println!(
                    "  absent  OPENROUTER_API_KEY unset — implementation scenarios \
                     report as `unscored`, which is not a failure"
                );
            }

            if problems > 0 {
                println!("\n{problems} problem(s). A run will still work for whatever is ok above.");
                std::process::exit(1);
            }
            println!("\nall good.");
        }

        Command::Verify {
            config,
            scenarios: scenario_dir,
            filter,
            repeat,
            drift,
        } => {
            if repeat < 2 {
                anyhow::bail!("--repeat must be at least 2: one run cannot disagree with itself");
            }
            let config = RunConfig::load(&config)?;
            let mut scenarios = scenario::load_dir(&scenario_dir)?;
            if let Some(filter) = &filter {
                scenarios.retain(|s| s.id.contains(filter.as_str()));
            }
            if scenarios.is_empty() {
                anyhow::bail!("no scenarios matched");
            }
            let out = std::env::temp_dir().join("nbench-verify");
            println!(
                "config `{}`: {} scenario(s) x {} adapter(s), {repeat} times",
                config.name,
                scenarios.len(),
                config.adapters.len()
            );

            let mut runs = Vec::new();
            for pass in 1..=repeat {
                println!("  run {pass}/{repeat}");
                let adapters = runner::build_adapters(&config, false, false).await?;
                let options = RunOptions {
                    run_id: format!("verify-{pass}"),
                    out_dir: out.clone(),
                    keep_responses: false,
                    judge: false,
                    ensure_indexed: false,
                };
                runs.push(runner::run(&scenarios, adapters, &options).await?);
            }

            let stability = Stability::compare(&runs);
            let verdicts = stability.unstable_verdicts();
            let scores = stability.unstable_scores();
            let drifting = stability.drifting_costs(drift);

            println!(
                "\n{} cell(s) over {} runs — {:.0}% reproduced exactly",
                stability.cells.len(),
                stability.repeats,
                stability.reproducibility() * 100.0
            );

            if verdicts.is_empty() {
                println!("\nverdicts    every cell agreed on what happened");
            } else {
                println!("\nverdicts    {} MOVED — a published pass rate is partly luck", verdicts.len());
                for cell in &verdicts {
                    let seen: Vec<&str> = cell.verdicts.iter().map(|v| v.label()).collect();
                    println!("  {} / {} — {}", cell.scenario, cell.adapter, seen.join(" then "));
                }
            }

            if !scores.is_empty() {
                println!("\nrecall      {} cell(s) kept the verdict but moved the score", scores.len());
                for cell in &scores {
                    let seen: Vec<String> =
                        cell.strict.iter().map(|s| format!("{s}%")).collect();
                    println!("  {} / {} — strict {}", cell.scenario, cell.adapter, seen.join(" then "));
                }
            }

            if drifting.is_empty() {
                println!("cost        no cell moved more than {:.0}%", drift * 100.0);
            } else {
                println!("\ncost        {} cell(s) moved more than {:.0}%", drifting.len(), drift * 100.0);
                for cell in drifting.iter().take(10) {
                    let (low, high) = cell.token_span();
                    println!(
                        "  {} / {} — {low}..{high} tokens ({:.0}%)",
                        cell.scenario,
                        cell.adapter,
                        cell.token_drift() * 100.0
                    );
                }
            }

            if !verdicts.is_empty() {
                std::process::exit(1);
            }
        }

        Command::List {
            scenarios: scenario_dir,
        } => {
            let scenarios = scenario::load_dir(&scenario_dir)?;
            println!("{} scenario(s)\n", scenarios.len());
            for scenario in &scenarios {
                println!(
                    "{:<44} {:<15} {:<13} {}",
                    scenario.id,
                    scenario.family.label(),
                    format!("{:?}", scenario.task).to_lowercase(),
                    scenario.target.coord()
                );
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::unavailable_capabilities;

    #[test]
    fn a_health_reply_names_what_is_not_ready() {
        let reply = r#"{"correlation":1,"body":{"kind":"health","facts":[
            {"capability":"compiler_registry","state":"local_ready"},
            {"capability":"index","state":"unavailable"},
            {"capability":"graph","state":"unavailable"}]},
            "terminal":{"kind":"partial"},"diagnostic":null}"#;
        assert_eq!(unavailable_capabilities(reply), vec!["index", "graph"]);
    }

    #[test]
    fn a_fully_ready_engine_reports_nothing() {
        let reply = r#"{"body":{"kind":"health","facts":[
            {"capability":"index","state":"local_ready"}]}}"#;
        assert!(unavailable_capabilities(reply).is_empty());
    }

    #[test]
    fn an_unparseable_reply_raises_no_false_alarm() {
        // A surface change must not be reported as a missing capability:
        // that would send someone looking for an outage that is not there.
        assert!(unavailable_capabilities("not json at all").is_empty());
        assert!(unavailable_capabilities(r#"{"body":{"kind":"rejected"}}"#).is_empty());
    }
}
