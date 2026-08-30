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
    /// Load and validate every scenario, then list them.
    List {
        #[arg(long, default_value = "scenarios")]
        scenarios: PathBuf,
    },
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
