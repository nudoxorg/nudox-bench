//! `nbench` — the command line.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use nudox_bench::config::RunConfig;
use nudox_bench::mcp::{McpClient, McpTransport};
use nudox_bench::runner::{self, RunOptions};
use nudox_bench::scenario;
use nudox_bench::tokens::TokenCount;

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
        /// Also try the Nudox endpoint.
        #[arg(long, env = "NUDOX_MCP_URL")]
        url: Option<String>,
        #[arg(long, env = "NUDOX_MCP_TOKEN")]
        token: Option<String>,
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
