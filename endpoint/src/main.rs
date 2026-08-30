//! Start a Nudox MCP endpoint and print its URL and launch token.
//!
//! # Why this exists
//!
//! Nudox's MCP server has no standalone binary. It is started inside the
//! `lindsey` GUI process by `McpHost::start`, because that is the only
//! process that was ever meant to host it. That is a perfectly good decision
//! for the product and an impossible one for a benchmark: a harness cannot
//! drive a GUI, and CI cannot open a window.
//!
//! This is the missing forty lines. It builds an engine over whatever package
//! roots you name, starts the same `McpEndpoint` the GUI starts, prints the
//! two values a client needs, and then stays up until interrupted.
//!
//! ```text
//! cargo run --manifest-path endpoint/Cargo.toml -- \
//!     --package ~/.cargo/registry/src/*/memchr-2.8.3
//! ```
//!
//! It prints shell-eval-able lines, so a run is two commands:
//!
//! ```text
//! eval "$(cargo run -q --manifest-path endpoint/Cargo.toml -- --package … --export)"
//! cargo run --bin nbench -- run --config configs/default.toml
//! ```

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::Parser;
use nudox_engine::mcp::{AccountGate, McpEndpoint, NudoxMcpServer, SessionToken};
use nudox_engine::{
    Engine, EngineConfig, EngineHandle, PackageLoadEvent, PackageSpec, ProducerLanguage,
};

#[derive(Parser)]
#[command(name = "nudox-endpoint", about = "Host a Nudox MCP endpoint on loopback")]
struct Cli {
    /// A package root to index at start-up. Repeatable.
    ///
    /// Name, version and language are read from the manifest in the
    /// directory, so a bare path is enough. Override with
    /// `PATH=name@version` when the manifest disagrees with what the corpus
    /// should call it.
    #[arg(long = "package", value_name = "PATH[=NAME@VERSION]", required = true)]
    packages: Vec<String>,

    /// Use this launch token instead of generating one. Handy for a stable
    /// dev loop; never use a fixed token for anything reachable.
    #[arg(long, env = "NUDOX_MCP_TOKEN")]
    token: Option<String>,

    /// Print `export NUDOX_MCP_URL=… ; export NUDOX_MCP_TOKEN=…` and nothing
    /// else on stdout, so the output can be `eval`'d.
    #[arg(long)]
    export: bool,

    /// How long to wait for every named package to finish loading.
    #[arg(long, default_value_t = 300)]
    load_timeout_secs: u64,
}

/// Which producer a package root belongs to, read from the manifest present.
///
/// This mirrors what the `index` tool does with a path target: the language
/// is read from which manifest is there, so a caller never names it.
fn language_for(root: &Path) -> Result<ProducerLanguage> {
    let has = |name: &str| root.join(name).is_file();
    if has("Cargo.toml") {
        return Ok(ProducerLanguage::Rust);
    }
    if has("go.mod") {
        return Ok(ProducerLanguage::Go);
    }
    if has("package.json") {
        return Ok(ProducerLanguage::TypeScript);
    }
    if has("pyproject.toml") || has("setup.py") {
        return Ok(ProducerLanguage::Python);
    }
    if has("pom.xml") || has("build.gradle") {
        return Ok(ProducerLanguage::Java);
    }
    if has("CMakeLists.txt") || has("compile_commands.json") {
        return Ok(ProducerLanguage::Cpp);
    }
    if std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("csproj"))
        })
    {
        return Ok(ProducerLanguage::CSharp);
    }
    bail!(
        "no recognised manifest in {} — expected one of Cargo.toml, go.mod, \
         package.json, pyproject.toml, pom.xml, build.gradle, *.csproj, \
         CMakeLists.txt or compile_commands.json",
        root.display()
    )
}

/// Best-effort name and version from the directory name, which for every
/// package cache on disk encodes both: `memchr-2.8.3`, `uuid@v1.6.0`.
fn name_and_version(root: &Path) -> (String, String) {
    let dir = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    if let Some((name, version)) = dir.rsplit_once('@') {
        return (name.to_owned(), version.to_owned());
    }
    // Split at the last `-` that begins a digit: `memchr-2.8.3` → memchr, 2.8.3.
    if let Some(index) = dir.rfind('-') {
        let (name, rest) = dir.split_at(index);
        let version = &rest[1..];
        if version.starts_with(|c: char| c.is_ascii_digit()) {
            return (name.to_owned(), version.to_owned());
        }
    }
    (dir.to_owned(), "0.0.0".to_owned())
}

fn parse_package(spec: &str) -> Result<PackageSpec> {
    let (path, explicit) = match spec.split_once('=') {
        Some((path, rest)) => (path, Some(rest)),
        None => (spec, None),
    };
    let root = PathBuf::from(shellexpand(path))
        .canonicalize()
        .with_context(|| format!("resolving package root {path}"))?;
    if !root.is_dir() {
        bail!("{} is not a directory", root.display());
    }

    let (name, version) = match explicit {
        Some(rest) => match rest.rsplit_once('@') {
            Some((name, version)) => (name.to_owned(), version.to_owned()),
            None => bail!("`{rest}` must be spelled NAME@VERSION"),
        },
        None => name_and_version(&root),
    };

    Ok(PackageSpec {
        language: language_for(&root)?,
        root,
        name,
        version,
    })
}

/// Expand a leading `~`, which a shell would normally have done but does not
/// when the argument came through a quoted glob.
fn shellexpand(path: &str) -> String {
    match path.strip_prefix("~/") {
        Some(rest) => match std::env::var("HOME") {
            Ok(home) => format!("{home}/{rest}"),
            Err(_) => path.to_owned(),
        },
        None => path.to_owned(),
    }
}

/// Block until every named package has loaded, or one fails, or we time out.
///
/// Loading runs a real compiler front end, so this is slow — tens of seconds
/// for a small crate. Reporting a `LoadFailed` immediately matters: without
/// it a producer failure presents as a silent hang until the deadline.
async fn wait_for_packages(
    engine: &EngineHandle,
    expected: &[String],
    timeout: Duration,
    quiet: bool,
) -> Result<()> {
    let events = engine.packages();
    let deadline = tokio::time::Instant::now() + timeout;
    let mut outstanding: Vec<String> = expected.to_vec();
    let mut failed: Vec<String> = Vec::new();

    while !outstanding.is_empty() {
        match tokio::time::timeout_at(deadline, events.recv_async()).await {
            Ok(Ok(PackageLoadEvent::Loaded { name, .. })) => {
                let name = name.to_string();
                outstanding.retain(|n| *n != name);
                if !quiet {
                    eprintln!("loaded {name} ({} outstanding)", outstanding.len());
                }
            }
            Ok(Ok(PackageLoadEvent::LoadFailed { name, error, .. })) => {
                // Not fatal. A benchmark corpus deliberately spans seven
                // languages, and some producers need optional features or an
                // external oracle that may not be installed. One package
                // failing to lower is a result to record, not a reason to
                // refuse to serve the ones that did.
                let name = name.to_string();
                outstanding.retain(|n| *n != name);
                eprintln!("WARN {name} failed to load: {error}");
                failed.push(name);
            }
            Ok(Ok(_)) => {}
            // The channel closing means every load already happened.
            Ok(Err(_)) => return Ok(()),
            Err(_) => {
                eprintln!(
                    "WARN timed out after {timeout:?} with {} package(s) still loading: {}",
                    outstanding.len(),
                    outstanding.join(", ")
                );
                break;
            }
        }
    }
    if !failed.is_empty() {
        eprintln!("\n{} package(s) did not load: {}", failed.len(), failed.join(", "));
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let specs: Vec<PackageSpec> = cli
        .packages
        .iter()
        .map(|spec| parse_package(spec))
        .collect::<Result<_>>()?;

    if !cli.export {
        for spec in &specs {
            eprintln!(
                "indexing {}@{} ({:?}) from {}",
                spec.name,
                spec.version,
                spec.language,
                spec.root.display()
            );
        }
    }

    let expected: Vec<String> = specs.iter().map(|s| s.name.clone()).collect();
    let engine = Engine::start_with_producer(EngineConfig::default(), specs);
    wait_for_packages(
        &engine,
        &expected,
        Duration::from_secs(cli.load_timeout_secs),
        cli.export,
    )
    .await?;

    let token = match cli.token {
        Some(literal) => SessionToken::from_secret(literal),
        None => SessionToken::generate(),
    };
    let secret = token.expose().to_owned();

    let endpoint = McpEndpoint::start_with_token(
        NudoxMcpServer::new(
            engine.clone(),
            // The benchmark measures the corpus, not billing.
            AccountGate::unmetered("nudox-bench: local benchmark endpoint"),
        ),
        token,
    )
    .await
    .context("binding the loopback MCP endpoint")?;

    let url = endpoint.url();
    if cli.export {
        println!("export NUDOX_MCP_URL={url}");
        println!("export NUDOX_MCP_TOKEN={secret}");
    } else {
        println!("NUDOX_MCP_URL={url}");
        println!("NUDOX_MCP_TOKEN={secret}");
        eprintln!("\nendpoint up. Ctrl-C to stop.");
    }

    tokio::signal::ctrl_c().await.ok();
    eprintln!("stopping…");
    endpoint.stop().await;
    Ok(())
}
