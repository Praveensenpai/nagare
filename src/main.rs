//! nagare — weekly anime RSS scheduler with Seedr delivery.

mod commands;
mod config;
mod db;
mod feed;
mod matcher;
mod notify;
mod organize;
mod pipeline;
mod tui;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "nagare",
    version,
    about = "Weekly anime RSS scheduler that delivers through Seedr and purges the cloud"
)]
struct Cli {
    /// Emit machine-readable JSON.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Fetch the feed and grab anything new.
    Run {
        /// Report what would be grabbed without touching Seedr.
        #[arg(long)]
        dry_run: bool,
    },
    /// Track a show, or pick one interactively.
    Add {
        /// Show name fragment; omit to pick from the live feed.
        query: Option<String>,
    },
    /// List tracked shows.
    List,
    /// Stop tracking a show by id.
    Remove { id: i64 },
    /// Show what the next run would grab.
    Scan,
    /// Print Seedr usage and recent events.
    Status,
    /// Check auth, feed, paths, and notification tooling.
    Doctor,
    /// Launch the interactive terminal UI dashboard.
    Tui,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = config::load()?;
    std::fs::create_dir_all(&cfg.staging_dir)
        .with_context(|| format!("failed to create {}", cfg.staging_dir.display()))?;
    let mut db = db::Db::open()?;

    match cli.command {
        None | Some(Command::Tui) => tui::run(cfg, db).await,
        Some(Command::Run { dry_run }) => commands::run(&cfg, &mut db, dry_run, cli.json).await,
        Some(Command::Add { query }) => commands::add(&cfg, &db, query, cli.json).await,
        Some(Command::List) => commands::list(&db, cli.json),
        Some(Command::Remove { id }) => commands::remove(&db, id, cli.json),
        Some(Command::Scan) => commands::scan(&cfg, &db, cli.json).await,
        Some(Command::Status) => commands::status(&cfg, &db, cli.json).await,
        Some(Command::Doctor) => commands::doctor(&cfg, &db, cli.json).await,
    }
}
