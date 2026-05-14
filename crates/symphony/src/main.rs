mod http;
mod watcher;

use anyhow::{bail, Result};
use clap::Parser;
use std::path::PathBuf;
use symphony_core::config::EffectiveConfig;
use symphony_core::orchestrator::{build_tracker, Orchestrator};
use symphony_core::workflow::load_workflow;
use symphony_core::workspace::WorkspaceManager;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(Debug, Parser)]
#[command(name = "symphony", version, about = "Symphony agent orchestrator")]
struct Cli {
    /// Path to WORKFLOW.md
    #[arg(long, default_value = "WORKFLOW.md")]
    workflow: PathBuf,
    /// HTTP server port (overrides server.port in WORKFLOW.md)
    #[arg(long)]
    port: Option<u16>,
    /// Log format: text|json
    #[arg(long, default_value = "text")]
    log_format: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Handle hidden mcp-bridge subcommand before clap parses other flags.
    let raw: Vec<String> = std::env::args().collect();
    if raw.get(1).map(|s| s.as_str()) == Some("mcp-bridge") {
        // parse --issue
        let mut issue = None;
        let mut iter = raw.iter().skip(2);
        while let Some(arg) = iter.next() {
            if arg == "--issue" {
                issue = iter.next().cloned();
            }
        }
        let Some(issue) = issue else {
            bail!("usage: symphony mcp-bridge --issue <issue-id>");
        };
        return symphony_core::harness::mcp_bridge::run_mcp_server(issue).await;
    }

    let cli = Cli::parse();
    init_tracing(&cli.log_format);

    let workflow = load_workflow(&cli.workflow)?;
    let config = EffectiveConfig::from_workflow(&workflow)?;
    if let Err(e) = config.validate_for_dispatch() {
        tracing::warn!("startup_validation_warn: {e}");
    }

    let tracker = build_tracker(&config)?;
    let workspace = WorkspaceManager::new(config.workspace_root.clone(), config.hooks.clone());
    let orchestrator = Orchestrator::new(workflow.clone(), config.clone(), tracker, workspace);

    // HTTP server (optional)
    let port = cli.port.or(config.server.port);
    if let Some(port) = port {
        let orch = orchestrator.clone();
        tokio::spawn(async move {
            if let Err(e) = http::serve(orch, port).await {
                tracing::error!("http_server_failed: {e}");
            }
        });
    }

    // Dynamic reload watcher
    {
        let orch = orchestrator.clone();
        let path = cli.workflow.clone();
        tokio::spawn(async move {
            if let Err(e) = watcher::watch_workflow(orch, path).await {
                tracing::warn!("workflow_watcher_failed: {e}");
            }
        });
    }

    // Signal handling — graceful shutdown on SIGINT (ctrl-c) *and* SIGTERM.
    // SIGTERM is what container runtimes / systemd / k8s send on stop; without
    // it the orchestrator would be hard-killed mid-turn instead of draining.
    let orch_for_signal = orchestrator.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        orch_for_signal.shutdown();
    });

    orchestrator.run().await;
    Ok(())
}

/// Resolves when the process receives SIGINT or (on Unix) SIGTERM.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(e) => {
                tracing::warn!("sigterm_handler_install_failed: {e}");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("sigint_received"),
        _ = terminate => tracing::info!("sigterm_received"),
    }
}

fn init_tracing(format: &str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber = tracing_subscriber::registry().with(filter);
    if format == "json" {
        subscriber.with(fmt::layer().json()).init();
    } else {
        subscriber.with(fmt::layer().with_target(true)).init();
    }
}
