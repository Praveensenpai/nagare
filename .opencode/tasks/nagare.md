# Task: Ship nagare with Interactive TUI (anime RSS → Seedr → Jellyfin)

> **Status:** Complete
> **Updated:** 2026-11-07

## Plan

- [x] **1. seedr-dl queue API** — `queue.rs` with `space`, `ensure_cached`, `purge_folders`
- [x] **2. seedr-dl static checks** — build + `clippy -D warnings`
- [x] **3. seedr-dl tests** — `cargo test` green (17 tests passing)
- [x] **4. nagare core engine** — feed, matcher, db, organize, pipeline, CLI compiles
- [x] **5. nagare interactive TUI** — Ratatui 0.30 + Crossterm 0.29 aesthetic terminal dashboard
- [x] **6. nagare test fix & verification** — fixed `parse_size` test, clippy 0 warnings, 19 unit & headless TUI tests passing, karakuri audit 0 violations
- [x] **7. nagare deploy & docs** — README.md, LICENSE, systemd user timer spec, CODEBASE.md
- [x] **8. seedr-dl release** — tag `v0.0.14`, push, CI green with Linux musl & macOS aarch64 assets
- [x] **9. nagare release** — public GitHub repo `Praveensenpai/nagare`, tag `v0.1.0`, CI green with Linux x86_64 asset

## Details

- [x] SubsPlease feed verified (50 items, ~5-day retention, category = `Show - 1080`)
- [x] `CachePolicy`, `CacheOutcome`, `EnsureOpts`, `Space` written in seedr-dl
- [x] `nagare` builds with 0 warnings (`cargo clippy -D warnings` clean)
- [x] TUI: 3-tab layout (Tracked Shows, Live Feed, Cloud/Activity)
- [x] TUI: rounded borders (`BorderType::Rounded`) & Nagare water aesthetic
- [x] TUI: responsive resize handling & panic-safe terminal restoration
- [x] TUI: interactive keyboard actions (tab switch, add show modal, delete, scan now, filter)
- [x] Headless TUI rendering integration test passing
- [x] Codebase audited with zero function length, nesting, or documentation violations
