# CODEBASE.md: nagare Semantic Digest

> **Notice**: AI-optimized semantic index. Do not write narrative prose. Keep token density high.

## 1. System Topology & Data Flow
```text
Entrypoint ──> CLI/Parser ──> Domain Logic ──> Infra/IO
```

## 2. Global Constraints & Architecture Patterns
- **Primary Language**: Rust 2021 edition
- **Architectural Paradigm**: Role-based (domain/, infra/, api/cli/, tui/)
- **Hard Constraints**: <400 lines/file, <60 lines/fn, zero production unwrap(), 0 warnings.
- **Target Distribution**: Linux x86_64 standalone binary

## 3. Module & Interface Skeleton

### `src/commands.rs` (Role: general, Lines: 359)
- **Responsibility**: Core general logic in src/commands.rs
- **Imports**: use crate :: config :: Config , use crate :: db :: Db , use crate :: feed :: { self , Item } , use crate :: matcher , use crate :: pipeline , use anyhow :: { bail , Context , Result } , use std :: collections :: BTreeMap , use std :: io :: Write 
- **Public Functions & Signatures**:
  ```rust
  async fn run (cfg : & Config , db : & mut Db , dry_run : bool , json : bool) -> Result < () >
  async fn add (cfg : & Config , db : & Db , query : Option < String > , json : bool) -> Result < () >
  fn list (db : & Db , json : bool) -> Result < () >
  fn remove (db : & Db , id : i64 , json : bool) -> Result < () >
  async fn scan (cfg : & Config , db : & Db , json : bool) -> Result < () >
  async fn status (cfg : & Config , db : & Db , json : bool) -> Result < () >
  async fn doctor (cfg : & Config , db : & Db , json : bool) -> Result < () >
  ```

### `src/config.rs` (Role: general, Lines: 125)
- **Responsibility**: Core general logic in src/config.rs
- **Imports**: use anyhow :: { Context , Result } , use serde :: { Deserialize , Serialize } , use std :: path :: PathBuf 
- **Types & Enums**:
  ```rust
  pub struct Config
  pub struct Jellyfin
  ```
- **Public Functions & Signatures**:
  ```rust
  fn config_dir () -> PathBuf
  fn state_dir () -> PathBuf
  fn load () -> Result < Config >
  ```

### `src/db.rs` (Role: general, Lines: 303)
- **Responsibility**: Core general logic in src/db.rs
- **Imports**: use crate :: config , use anyhow :: { Context , Result } , use rusqlite :: { params , Connection , OptionalExtension } , use std :: path :: Path 
- **Types & Enums**:
  ```rust
  pub struct Show
  pub struct Event
  pub struct Db
  ```
- **Public Functions & Signatures**:
  ```rust
  fn open () -> Result < Self >
  fn open_at (path : & Path) -> Result < Self >
  fn add_show (& self , title : & str , match_title : & str , season : u32 , max_episode : f64 ,) -> Result < i64 >
  fn shows (& self) -> Result < Vec < Show > >
  fn remove_show (& self , id : i64) -> Result < bool >
  fn has_release (& self , infohash : & str) -> Result < bool >
  fn record_release (& mut self , infohash : & str , show_id : i64 , season : u32 , episode : f64 , title : & str , bytes : u64 ,) -> Result < () >
  fn get_state (& self , key : & str) -> Result < Option < String > >
  fn set_state (& self , key : & str , value : & str) -> Result < () >
  fn log_event (& self , level : & str , message : & str) -> Result < () >
  fn recent_events (& self , limit : usize) -> Result < Vec < Event > >
  ```

### `src/feed.rs` (Role: general, Lines: 268)
- **Responsibility**: Core general logic in src/feed.rs
- **Imports**: use anyhow :: { Context , Result } , use chrono :: { DateTime , Utc } , use std :: time :: Duration 
- **Types & Enums**:
  ```rust
  pub struct Item
  pub struct FetchOutcome
  ```
- **Public Functions & Signatures**:
  ```rust
  fn episode_label (& self) -> String
  async fn fetch (url : & str , etag : Option < & str > , last_modified : Option < & str > ,) -> Result < FetchOutcome >
  fn parse (body : & str) -> Result < Vec < Item > >
  ```

### `src/main.rs` (Role: general, Lines: 75)
- **Responsibility**: Core general logic in src/main.rs
- **Imports**: use anyhow :: { Context , Result } , use clap :: { Parser , Subcommand } 

### `src/matcher.rs` (Role: general, Lines: 122)
- **Responsibility**: Core general logic in src/matcher.rs
- **Imports**: use crate :: db :: Show , use crate :: feed :: Item 
- **Public Functions & Signatures**:
  ```rust
  fn normalize (value : & str) -> String
  fn matches (show : & Show , item : & Item) -> bool
  fn pick < 'a > (show : & Show , items : & 'a [Item]) -> Option < & 'a Item >
  ```

### `src/notify.rs` (Role: general, Lines: 32)
- **Responsibility**: Core general logic in src/notify.rs
- **Imports**: use anyhow :: { Context , Result } , use tokio :: process :: Command 
- **Public Functions & Signatures**:
  ```rust
  async fn done (title : & str , body : & str)
  async fn alert (title : & str , body : & str)
  ```

### `src/organize.rs` (Role: general, Lines: 158)
- **Responsibility**: Core general logic in src/organize.rs
- **Imports**: use crate :: config :: Jellyfin , use crate :: feed :: Item , use anyhow :: { Context , Result } , use std :: path :: { Path , PathBuf } 
- **Types & Enums**:
  ```rust
  pub struct Placement
  ```
- **Public Functions & Signatures**:
  ```rust
  fn plan (item : & Item) -> Placement
  async fn place (library : & Path , staging : & Path , placement : & Placement , files : & [PathBuf] ,) -> Result < PathBuf >
  async fn refresh_jellyfin (config : & Jellyfin) -> Result < () >
  ```

### `src/pipeline.rs` (Role: general, Lines: 181)
- **Responsibility**: Core general logic in src/pipeline.rs
- **Imports**: use crate :: config :: Config , use crate :: db :: Db , use crate :: feed :: { self , Item } , use crate :: matcher , use crate :: notify , use crate :: organize , use anyhow :: { bail , Context , Result } , use seedr_dl :: { load_auth , SeedrClient } , use std :: path :: PathBuf , use std :: time :: Duration 
- **Types & Enums**:
  ```rust
  pub struct Report
  ```
- **Public Functions & Signatures**:
  ```rust
  fn is_idle (& self) -> bool
  async fn run (cfg : & Config , db : & mut Db , dry_run : bool) -> Result < Report >
  ```

### `src/tui/activity.rs` (Role: tui, Lines: 160)
- **Responsibility**: Core tui logic in src/tui/activity.rs
- **Imports**: use crate :: tui :: app :: App , use ratatui :: { layout :: { Constraint , Direction , Layout , Rect } , style :: { Color , Modifier , Style } , text :: { Line , Span } , widgets :: { Block , BorderType , Borders , Gauge , Paragraph , Row , Table } , Frame , } 
- **Public Functions & Signatures**:
  ```rust
  fn render (frame : & mut Frame , area : Rect , app : & App)
  ```

### `src/tui/app.rs` (Role: tui, Lines: 368)
- **Responsibility**: Core tui logic in src/tui/app.rs
- **Imports**: use crate :: config :: Config , use crate :: db :: { Db , Event , Show } , use crate :: feed :: { self , Item } , use crate :: matcher , use crate :: pipeline :: { self , Report } , use anyhow :: Result , use std :: time :: Instant 
- **Types & Enums**:
  ```rust
  pub enum Tab
  pub enum InputMode
  pub struct TrackPrompt
  pub enum AddField
  pub struct App
  ```
- **Public Functions & Signatures**:
  ```rust
  fn next (self) -> Self
  fn prev (self) -> Self
  fn next (self) -> Self
  fn new (config : Config , db : Db) -> Self
  fn reload_shows (& mut self)
  fn reload_events (& mut self)
  fn set_status (& mut self , text : impl Into < String >)
  fn active_status (& self) -> Option < & str >
  fn update_filter (& mut self)
  fn remove_selected_show (& mut self) -> Result < () >
  fn open_track_prompt (& mut self) -> Result < () >
  fn apply_track_prompt (& mut self , choice_idx : usize) -> Result < () >
  fn submit_add_modal (& mut self) -> Result < () >
  async fn run_pipeline (& mut self) -> Result < Report >
  async fn fetch_feed (& mut self) -> Result < () >
  async fn fetch_space (& mut self)
  ```

### `src/tui/app_tests.rs` (Role: tui, Lines: 133)
- **Responsibility**: Core tui logic in src/tui/app_tests.rs
- **Imports**: use super :: * , use std :: path :: { Path , PathBuf } 

### `src/tui/modals.rs` (Role: tui, Lines: 239)
- **Responsibility**: Core tui logic in src/tui/modals.rs
- **Imports**: use crate :: tui :: app :: { AddField , App } , use ratatui :: { layout :: { Alignment , Constraint , Direction , Layout , Rect } , style :: { Color , Modifier , Style } , text :: { Line , Span } , widgets :: { Block , BorderType , Borders , Clear , Paragraph } , Frame , } 
- **Public Functions & Signatures**:
  ```rust
  fn render_help (frame : & mut Frame , area : Rect)
  fn render_add_modal (frame : & mut Frame , area : Rect , app : & App)
  fn centered_rect (percent_x : u16 , percent_y : u16 , area : Rect) -> Rect
  fn render_track_modal (frame : & mut Frame , area : Rect , app : & App)
  ```

### `src/tui/ui.rs` (Role: tui, Lines: 285)
- **Responsibility**: Core tui logic in src/tui/ui.rs
- **Imports**: use crate :: tui :: activity , use crate :: tui :: app :: { App , InputMode , Tab } , use crate :: tui :: modals , use crate :: tui :: views , use ratatui :: { layout :: { Alignment , Constraint , Direction , Layout , Rect } , style :: { Color , Modifier , Style } , text :: { Line , Span } , widgets :: { Block , BorderType , Borders , Paragraph , Tabs } , Frame , } 
- **Public Functions & Signatures**:
  ```rust
  fn draw (frame : & mut Frame , app : & App)
  ```

### `src/tui/views.rs` (Role: tui, Lines: 280)
- **Responsibility**: Core tui logic in src/tui/views.rs
- **Imports**: use crate :: db :: Show , use crate :: feed :: Item , use crate :: tui :: app :: { App , InputMode } , use ratatui :: { layout :: { Alignment , Constraint , Direction , Layout , Rect } , style :: { Color , Modifier , Style } , text :: { Line , Span } , widgets :: { Block , BorderType , Borders , Paragraph , Row , Table } , Frame , } 
- **Public Functions & Signatures**:
  ```rust
  fn render_tracked (frame : & mut Frame , area : Rect , app : & App)
  fn render_feed (frame : & mut Frame , area : Rect , app : & App)
  ```

### `src/tui.rs` (Role: tui, Lines: 277)
- **Responsibility**: Core tui logic in src/tui.rs
- **Imports**: use crate :: config :: Config , use crate :: db :: Db , use anyhow :: { bail , Context , Result } , use app :: { AddField , App , InputMode , Tab } , use crossterm :: { event :: { self , Event , KeyCode , KeyEvent , KeyEventKind , KeyModifiers } , execute , terminal :: { disable_raw_mode , enable_raw_mode , EnterAlternateScreen , LeaveAlternateScreen } , } , use ratatui :: backend :: CrosstermBackend , use ratatui :: { Terminal , TerminalOptions , Viewport } , use std :: io :: { stdout , IsTerminal , Stdout } , use std :: time :: Duration 
- **Public Functions & Signatures**:
  ```rust
  async fn run (config : Config , db : Db) -> Result < () >
  ```

## 4. Execution Lifecycle Trace
1. **Startup**: Entrypoint parses CLI flags & dispatches command.
2. **Execution**: Core domain logic processes inputs and evaluates rules.
3. **Persistence / I/O**: Domain logic calls infra for disk/terminal I/O.
4. **Exit**: Graceful termination with standard exit codes.

## 5. Verification Commands
```bash
cargo build --release --target x86_64-unknown-linux-gnu
cargo test --all-targets
cargo clippy --all-targets -- -D warnings && cargo fmt --check
```
