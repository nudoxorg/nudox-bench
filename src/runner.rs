//! Wiring: build the adapters, run the matrix, score it, write it out.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::adapter::nudox::NudoxAdapter;
use crate::adapter::ripgrep::RipgrepAdapter;
use crate::adapter::rust_analyzer::RustAnalyzerAdapter;
use crate::adapter::Adapter;
use crate::config::{policy_by_name, AdapterConfig, RunConfig};
use crate::judge::Judge;
use crate::report::{outcome, AdapterInfo, RunReport};
use crate::scenario::Scenario;

/// Options that are properties of a run rather than of a config.
#[derive(Clone, Debug)]
pub struct RunOptions {
    pub run_id: String,
    pub out_dir: PathBuf,
    /// Keep every response body in the JSON report. Off by default: these
    /// dominate the file size and are only wanted when debugging a score.
    pub keep_responses: bool,
    /// Run the judge for implementation scenarios.
    pub judge: bool,
    /// Index each scenario's `target.root` into the Nudox corpus first.
    pub ensure_indexed: bool,
}

/// Build the adapters a config asks for.
pub async fn build_adapters(
    config: &RunConfig,
    keep_responses: bool,
    ensure_indexed: bool,
) -> Result<Vec<Box<dyn Adapter>>> {
    let mut adapters: Vec<Box<dyn Adapter>> = Vec::new();
    for entry in &config.adapters {
        match entry {
            AdapterConfig::Nudox {
                policy,
                url,
                token_env,
            } => {
                let policy = policy_by_name(policy.as_deref().unwrap_or("full"))?;
                let url = url
                    .clone()
                    .or_else(|| std::env::var("NUDOX_MCP_URL").ok())
                    .context(
                        "no Nudox endpoint: set `url` in the config or $NUDOX_MCP_URL. \
                         The endpoint is started by the Nudox host on an ephemeral \
                         loopback port; see docs/RUNNING.md",
                    )?;
                let token_var = token_env.as_deref().unwrap_or("NUDOX_MCP_TOKEN");
                let token = std::env::var(token_var).ok().filter(|t| !t.trim().is_empty());
                let adapter =
                    NudoxAdapter::connect(&url, token, policy, keep_responses, ensure_indexed)
                    .await
                    .with_context(|| format!("connecting to Nudox at {url}"))?;
                adapters.push(Box::new(adapter));
            }
            AdapterConfig::Ripgrep => {
                adapters.push(Box::new(RipgrepAdapter::new(keep_responses)));
            }
            AdapterConfig::RustAnalyzer => {
                adapters.push(Box::new(RustAnalyzerAdapter::new(keep_responses)));
            }
            AdapterConfig::McpStdio { name, .. } => {
                anyhow::bail!(
                    "adapter `{name}`: the generic stdio MCP adapter is declared but not \
                     wired yet — the first comparison set is Nudox against the tools an \
                     agent already has"
                );
            }
        }
    }
    Ok(adapters)
}

/// Run every scenario against every adapter.
pub async fn run(
    scenarios: &[Scenario],
    mut adapters: Vec<Box<dyn Adapter>>,
    options: &RunOptions,
) -> Result<RunReport> {
    let judge = options.judge.then(Judge::from_env).flatten();
    if options.judge && judge.is_none() {
        tracing::warn!(
            "--judge was passed but OPENROUTER_API_KEY is unset; \
             implementation scenarios will be reported as not-run"
        );
    }

    let info: Vec<AdapterInfo> = adapters
        .iter()
        .map(|a| AdapterInfo {
            name: a.name().to_owned(),
            description: a.description(),
            resident: a.resident_cost(),
        })
        .collect();

    for adapter in adapters.iter_mut() {
        if let Err(e) = adapter.preflight(scenarios).await {
            // A failed preflight is a warning, not a stop: the run still
            // produces numbers, and the report records what the corpus
            // actually held rather than what it was meant to.
            tracing::warn!("{}: preflight failed: {e:#}", adapter.name());
        }
    }

    let mut outcomes = Vec::new();
    for adapter in adapters.iter_mut() {
        for scenario in scenarios {
            tracing::info!("{} → {}", adapter.name(), scenario.id);
            let attempt = adapter.attempt(scenario).await;
            let score = crate::truth::score(scenario, &attempt.context);

            let verdict = match (&judge, score.judged, attempt.unsupported.is_some()) {
                (Some(judge), true, false) => match judge.judge(scenario, &attempt.context).await {
                    Ok(verdict) => Some(verdict),
                    Err(e) => {
                        tracing::warn!("judging {}: {e}", scenario.id);
                        None
                    }
                },
                _ => None,
            };

            outcomes.push(outcome(scenario, attempt, score, verdict));
        }
    }

    for adapter in adapters.iter_mut() {
        adapter.shutdown().await;
    }

    Ok(RunReport {
        run_id: options.run_id.clone(),
        timestamp: chrono::Local::now().to_rfc3339(),
        adapters: info,
        outcomes,
    })
}

/// Write `report.json` and `report.md` into the run directory.
pub fn write_report(report: &RunReport, out_dir: &Path) -> Result<PathBuf> {
    let dir = out_dir.join(&report.run_id);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating {}", dir.display()))?;

    let json_path = dir.join("report.json");
    std::fs::write(&json_path, serde_json::to_string_pretty(report)?)
        .with_context(|| format!("writing {}", json_path.display()))?;

    let md_path = dir.join("report.md");
    std::fs::write(&md_path, report.to_markdown())
        .with_context(|| format!("writing {}", md_path.display()))?;

    Ok(dir)
}
