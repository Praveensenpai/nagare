//! CLI command implementations.

mod downloads;
pub use downloads::downloads;

use crate::config::Config;
use crate::db::Db;
use crate::feed::{self, Item};
use crate::matcher;
use crate::pipeline;
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::io::Write;

/// `nagare run`
pub async fn run(cfg: &Config, db: &mut Db, dry_run: bool, json: bool) -> Result<()> {
    let report = pipeline::run(cfg, db, dry_run).await?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "grabbed": report.grabbed,
                "planned": report.planned,
                "skipped": report.skipped,
                "failed": report.failed,
                "not_modified": report.not_modified,
            })
        );
    } else if report.not_modified {
        println!("feed unchanged");
    } else if report.is_idle() {
        println!("nothing new");
    } else {
        for path in &report.grabbed {
            println!("grabbed {path}");
        }
        for title in &report.planned {
            println!("would grab {title}");
        }
        for reason in &report.skipped {
            println!("skipped: {reason}");
        }
        for failure in &report.failed {
            eprintln!("failed: {failure}");
        }
    }
    if !report.failed.is_empty() {
        bail!("{} release(s) failed", report.failed.len());
    }
    Ok(())
}

/// `nagare add`
pub async fn add(cfg: &Config, db: &Db, query: Option<String>, json: bool) -> Result<()> {
    let outcome = feed::fetch(&cfg.feed_url, None, None).await?;
    let catalog = catalog(&outcome.items);
    if catalog.is_empty() {
        bail!("feed returned no shows");
    }
    match query {
        Some(value) => add_by_query(db, &catalog, &value, json),
        None => interactive(db, &catalog, json),
    }
}

fn add_by_query(
    db: &Db,
    catalog: &BTreeMap<String, (u32, f64)>,
    value: &str,
    json: bool,
) -> Result<()> {
    let needle = matcher::normalize(value);
    let Some((name, &(season, episode))) = catalog
        .iter()
        .find(|(name, _)| matcher::normalize(name).contains(&needle))
    else {
        bail!("no feed show matches {value:?}");
    };
    track(db, name, season, episode, json)
}

fn interactive(db: &Db, catalog: &BTreeMap<String, (u32, f64)>, json: bool) -> Result<()> {
    let entries: Vec<(&String, u32, f64)> = catalog
        .iter()
        .map(|(name, (season, episode))| (name, *season, *episode))
        .collect();
    for (index, (name, season, episode)) in entries.iter().enumerate() {
        println!(
            "{:>3}. {name} (after S{season:02}E{:02})",
            index + 1,
            *episode as u32
        );
    }
    print!("\nselect numbers separated by spaces: ");
    std::io::stdout()
        .flush()
        .context("failed to flush stdout")?;
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .context("failed to read selection")?;
    let mut chosen = 0;
    for token in line.split_whitespace() {
        let Ok(index) = token.parse::<usize>() else {
            continue;
        };
        let Some((name, season, episode)) = index
            .checked_sub(1)
            .and_then(|position| entries.get(position))
        else {
            eprintln!("ignoring out-of-range selection {index}");
            continue;
        };
        track(db, name.as_str(), *season, *episode, json)?;
        chosen += 1;
    }
    if chosen == 0 {
        bail!("no shows selected");
    }
    Ok(())
}

fn track(db: &Db, name: &str, season: u32, episode: f64, json: bool) -> Result<()> {
    db.add_show(name, name, season, episode)?;
    db.log_event(
        "track",
        &format!("{name} after S{season:02}E{:02}", episode as u32),
    )?;
    if json {
        println!("{}", serde_json::json!({ "tracked": name }));
    } else {
        println!(
            "tracking {name}; next grab starts at S{season:02}E{:02}",
            episode as u32 + 1
        );
    }
    Ok(())
}

/// `nagare list`
pub fn list(db: &Db, json: bool) -> Result<()> {
    let shows = db.shows()?;
    if json {
        let rows: Vec<_> = shows
            .iter()
            .map(|show| {
                serde_json::json!({
                    "id": show.id,
                    "title": show.title,
                    "season": show.season,
                    "max_episode": show.max_episode,
                    "enabled": show.enabled,
                })
            })
            .collect();
        println!("{}", serde_json::Value::Array(rows));
        return Ok(());
    }
    if shows.is_empty() {
        println!("no tracked shows");
        return Ok(());
    }
    for show in shows {
        println!(
            "{:>3}  {:<48}  S{:02}E{:02}  {}",
            show.id,
            show.title,
            show.season,
            show.max_episode as u32,
            if show.enabled { "on" } else { "off" }
        );
    }
    Ok(())
}

/// `nagare remove`
pub fn remove(db: &Db, id: i64, json: bool) -> Result<()> {
    if !db.remove_show(id)? {
        bail!("no show with id {id}");
    }
    if json {
        println!("{}", serde_json::json!({ "removed": id }));
    } else {
        println!("removed {id}");
    }
    Ok(())
}

/// `nagare scan`
pub async fn scan(cfg: &Config, db: &Db, json: bool) -> Result<()> {
    let items = feed::fetch(&cfg.feed_url, None, None).await?.items;
    let shows = db.shows()?;
    let mut planned = Vec::new();
    for show in shows.iter().filter(|show| show.enabled) {
        if let Some(item) = matcher::pick(show, &items) {
            planned.push(item.title.clone());
        }
    }
    if json {
        println!("{}", serde_json::json!({ "planned": planned }));
    } else if planned.is_empty() {
        println!("nothing pending");
    } else {
        for title in planned {
            println!("would grab {title}");
        }
    }
    Ok(())
}

/// `nagare status`
pub async fn status(cfg: &Config, db: &Db, json: bool) -> Result<()> {
    let events = db.recent_events(10)?;
    let space = seedr_space().await;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "feed_url": cfg.feed_url,
                "library_dir": cfg.library_dir,
                "next_run": crate::tui::transfer::timer_next_run(),
                "seedr": space.as_ref().ok().map(|(max, used)| serde_json::json!({ "max": max, "used": used })),
                "seedr_error": space.as_ref().err().map(ToString::to_string),
                "events": events.iter().map(|event| serde_json::json!({
                    "at": event.at, "level": event.level, "message": event.message
                })).collect::<Vec<_>>(),
            })
        );
        return Ok(());
    }
    match &space {
        Ok((max, used)) => println!("seedr   {used} / {max} bytes used"),
        Err(err) => println!("seedr   unavailable ({err})"),
    }
    let next_run = crate::tui::transfer::timer_next_run();
    let timer_active = check_systemd_timer();
    println!(
        "timer   {}",
        match (&next_run, timer_active) {
            (Some(next), true) => format!("active (next: {next})"),
            (None, true) => "active (scheduled)".into(),
            (_, false) => "inactive (systemctl --user enable --now nagare.timer)".into(),
        }
    );
    println!("library {}", cfg.library_dir.display());
    for event in events {
        println!("{}  {:<5}  {}", event.at, event.level, event.message);
    }
    Ok(())
}

/// `nagare doctor`
pub async fn doctor(cfg: &Config, db: &Db, json: bool) -> Result<()> {
    let checks = collect_checks(cfg, db).await?;

    if json {
        let rows: Vec<_> = checks
            .iter()
            .map(|(name, ok, detail)| {
                serde_json::json!({ "check": name, "ok": ok, "detail": detail })
            })
            .collect();
        println!("{}", serde_json::Value::Array(rows));
    } else {
        for (name, ok, detail) in &checks {
            println!(
                "{:<5} {:<11} {detail}",
                if *ok { "ok" } else { "fail" },
                name
            );
        }
    }
    if checks.iter().any(|(_, ok, _)| !ok) {
        bail!("one or more checks failed");
    }
    Ok(())
}

async fn collect_checks<'a>(cfg: &'a Config, db: &'a Db) -> Result<Vec<(&'a str, bool, String)>> {
    let mut checks: Vec<(&str, bool, String)> = Vec::new();

    let auth = seedr_dl::load_auth();
    checks.push((
        "seedr auth",
        matches!(auth, Ok(Some(_))),
        "run `seedr-dl auth`".into(),
    ));

    match feed::fetch(&cfg.feed_url, None, None).await {
        Ok(outcome) => checks.push((
            "feed",
            !outcome.items.is_empty(),
            format!("{} items", outcome.items.len()),
        )),
        Err(err) => checks.push(("feed", false, err.to_string())),
    }

    checks.push((
        "library",
        cfg.library_dir.exists(),
        cfg.library_dir.display().to_string(),
    ));
    checks.push((
        "staging",
        cfg.staging_dir.exists(),
        cfg.staging_dir.display().to_string(),
    ));
    checks.push((
        "shows",
        !db.shows()?.is_empty(),
        "at least one tracked".into(),
    ));
    checks.push(("tayori", which("tayori"), "notifications".into()));
    let timer_ok = check_systemd_timer();
    checks.push((
        "timer",
        timer_ok,
        if timer_ok {
            "nagare.timer active".into()
        } else {
            "systemctl --user enable --now nagare.timer".into()
        },
    ));

    Ok(checks)
}

async fn seedr_space() -> Result<(u64, u64)> {
    let auth = seedr_dl::load_auth()?.context("no Seedr auth")?;
    let client = seedr_dl::SeedrClient::new(auth.access_token);
    let space = client.space().await?;
    Ok((space.max, space.used))
}

fn which(binary: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(binary).is_file()))
}

fn check_systemd_timer() -> bool {
    std::process::Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", "nagare.timer"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn catalog(items: &[Item]) -> BTreeMap<String, (u32, f64)> {
    let mut map: BTreeMap<String, (u32, f64)> = BTreeMap::new();
    for item in items.iter().filter(|item| !item.is_batch) {
        let Some(episode) = item.episode else {
            continue;
        };
        map.entry(item.show.clone())
            .and_modify(|(season, max)| {
                if item.season > *season || (item.season == *season && episode > *max) {
                    *season = item.season;
                    *max = episode;
                }
            })
            .or_insert((item.season, episode));
    }
    map
}
