//! Telegram events through the `tayori` CLI.

use anyhow::{Context, Result};
use tokio::process::Command;

/// Sends a completion card. Failures are logged, never fatal.
pub async fn done(title: &str, body: &str) {
    let message = format!("{title}\n{body}");
    if let Err(err) = run(&["done", &message]).await {
        eprintln!("tayori done failed: {err:#}");
    }
}

/// Sends an alert card. Failures are logged, never fatal.
pub async fn alert(title: &str, body: &str) {
    let message = format!("{title}\n{body}");
    if let Err(err) = run(&["alert", &message]).await {
        eprintln!("tayori alert failed: {err:#}");
    }
}

async fn run(args: &[&str]) -> Result<()> {
    let status = Command::new("tayori")
        .args(args)
        .status()
        .await
        .context("failed to spawn tayori")?;
    if !status.success() {
        anyhow::bail!("tayori exited with {status}");
    }
    Ok(())
}
