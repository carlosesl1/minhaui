# Background Apps Topbar Dropdown Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a topbar dropdown that lists currently running applications registered in the Windows notification area and safely opens a selected application.

**Architecture:** Add runtime-only pure module and popover variants, build a bounded notification catalog from injected registry/process records, and isolate real Windows registry and ToolHelp calls in one private Adapter. Load the catalog on demand on a worker, return it through the existing native event queue, render it through the existing popover surface, and reuse cached executable icons.

**Tech Stack:** Rust 1.97, Win32 Registry and ToolHelp APIs through `windows` 0.62, Direct2D/DirectWrite, existing shell controllers and event queue, Cargo tests, Clippy, rustfmt, PowerShell architecture gate.

**Design:** [`../specs/2026-07-16-background-apps-topbar-design.md`](../specs/2026-07-16-background-apps-topbar-design.md)

**Architecture:** [`../../architecture/adr/0008-background-apps-notification-catalog.md`](../../architecture/adr/0008-background-apps-notification-catalog.md)

**Architectural impact:** Risk L. `shell-core` gains runtime-only identities;
`shell-renderer` gains bounded row icon/viewport data; the Slot Runtime owns the
active catalog and latest centralized window snapshot; the private Windows
Adapter adds Registry, ToolHelp and one-shot `PostMessageW` FFI. No new local
crate edge or persisted schema is added. Registry/process work runs off the UI
thread, diagnostics retain only closed error codes, and rollback removes the
runtime-only module without migration.

**Workspace note:** Execute in the current integrated workspace because the feature depends on uncommitted priority-one topbar and observation work already present there. Never stage unrelated dirty files; each optional checkpoint must inventory exact paths first.

---

## File map

| Responsibility | File |
| --- | --- |
| Pure module/popover identities and persistence eligibility | `crates/shell-core/src/model.rs` |
| Default runtime ordering and invariants | `crates/shell-core/src/state.rs` |
| Pure notification registration/process matching | `crates/shell-platform-windows/src/background_apps.rs` |
| Registry, ToolHelp, file-description, and activation Adapter | `crates/shell-platform-windows/src/win32_background_apps.rs` |
| On-demand worker request/result handoff | `crates/shell-platform-windows/src/background_apps_worker.rs` |
| Popover items, loading generation, scrolling, and action | `crates/shell-platform-windows/src/popover_types.rs`, `popover_controller.rs`, `popover_adapters.rs` |
| Runtime-only module insertion | `crates/shell-platform-windows/src/win32_config.rs`, `settings_controller.rs` |
| Topbar intent and native event integration | `crates/shell-platform-windows/src/topbar_controller.rs`, `lib.rs`, `win32_event_queue.rs`, `win32_windowing.rs`, `win32_owner.rs`, `win32_popover_render.rs` |
| Popover icon/scroll scene and geometry | `crates/shell-renderer/src/popover_scene.rs`, `popover_layout.rs`, `native_showcase.rs`, `native_showcase_popover.rs` |
| Windows features | `crates/shell-platform-windows/Cargo.toml` |
| Documentation and verification | `README.md`, `SUPPORT.md`, this plan |

### Task 1: Pure domain identity and runtime composition

**Files:**

- Modify: `crates/shell-core/src/model.rs`
- Modify: `crates/shell-core/src/state.rs`
- Modify: `crates/shell-core/tests/visual_defaults.rs`
- Modify: `crates/shell-core/tests/reducer.rs`
- Modify: `crates/shell-config/tests/config.rs`
- Modify: `crates/shell-platform-windows/src/win32_config.rs`
- Modify: `crates/shell-platform-windows/src/settings_controller.rs`

- [x] **Step 1: Write failing domain and composition tests**

Add assertions that the new module is immediately before the clock, cannot be customized, and is excluded from V1 persistence:

```rust
#[test]
fn background_apps_is_runtime_only_and_precedes_clock() {
    let state = ShellState::default();
    let kinds = state
        .topbar_modules()
        .iter()
        .map(|module| module.kind())
        .collect::<Vec<_>>();
    let background = kinds
        .iter()
        .position(|kind| *kind == TopbarModuleKind::BackgroundApps)
        .expect("background apps module");
    let clock = kinds
        .iter()
        .position(|kind| *kind == TopbarModuleKind::Clock)
        .expect("clock module");
    assert_eq!(background + 1, clock);
    assert!(!TopbarModuleKind::BackgroundApps.is_v1_persisted());
    assert!(!TopbarModuleKind::BackgroundApps.is_customizable());
}
```

In `win32_config.rs`, load a V1 fixture and assert runtime insertion without serialized insertion:

```rust
assert_eq!(
    kinds,
    [
        TopbarModuleKind::SystemMenu,
        TopbarModuleKind::AppIdentity,
        TopbarModuleKind::Search,
        TopbarModuleKind::Network,
        TopbarModuleKind::BackgroundApps,
        TopbarModuleKind::Clock,
    ]
);
```

- [x] **Step 2: Run focused tests and verify RED**

Run:

```powershell
cargo test -p shell-core background_apps
cargo test -p shell-platform-windows win32_config --lib
```

Expected: compilation fails because `BackgroundApps` and `is_customizable` do not exist.

- [x] **Step 3: Add the pure variants and fixed runtime ordering**

Extend the pure enums and policy:

```rust
pub enum TopbarModuleKind {
    SystemMenu,
    AppIdentity,
    Search,
    Clock,
    Network,
    Volume,
    Power,
    Notifications,
    BackgroundApps,
}

impl TopbarModuleKind {
    pub const fn is_customizable(self) -> bool {
        !matches!(
            self,
            Self::SystemMenu | Self::AppIdentity | Self::Search | Self::BackgroundApps
        )
    }

    pub const fn is_v1_persisted(self) -> bool {
        !matches!(self, Self::AppIdentity | Self::Search | Self::BackgroundApps)
    }
}

pub enum Popover {
    SystemMenu,
    Calendar,
    Network,
    Volume,
    Power,
    Notifications,
    BackgroundApps,
}
```

Insert the module before the first clock while preserving configured order:

```rust
fn runtime_topbar_modules(configured: &[TopbarModule]) -> Vec<TopbarModule> {
    let mut modules = vec![
        TopbarModule::new(TopbarModuleKind::SystemMenu, true),
        TopbarModule::new(TopbarModuleKind::AppIdentity, true),
        TopbarModule::new(TopbarModuleKind::Search, true),
    ];
    let mut inserted = false;
    for module in configured.iter().copied() {
        if module.kind() == TopbarModuleKind::Clock && !inserted {
            modules.push(TopbarModule::new(TopbarModuleKind::BackgroundApps, true));
            inserted = true;
        }
        modules.push(module);
    }
    if !inserted {
        modules.push(TopbarModule::new(TopbarModuleKind::BackgroundApps, true));
    }
    modules
}
```

Replace reducer/settings checks of `is_fixed_leading` with `!is_customizable` for edit authorization. Keep layout clustering based on `is_fixed_leading` so `BackgroundApps` remains in the right cluster.

- [x] **Step 4: Run focused tests and verify GREEN**

Run:

```powershell
cargo test -p shell-core
cargo test -p shell-config
cargo test -p shell-platform-windows win32_config --lib
cargo test -p shell-platform-windows settings_controller
```

Expected: all focused tests pass and V1 rejects/filters runtime-only kinds.

### Task 2: Pure bounded notification catalog

**Files:**

- Create: `crates/shell-platform-windows/src/background_apps.rs`
- Modify: `crates/shell-platform-windows/src/lib.rs`
- Test: inline unit tests in `background_apps.rs`

- [x] **Step 1: Write failing matching and privacy tests**

Define wished-for inputs and verify stale filtering, deduplication, label fallback, sorting, and bounds:

```rust
#[test]
fn catalog_keeps_only_live_registered_executables() {
    let registrations = vec![
        registration(r"{PROGRAMFILES}\AMD\RadeonSoftware.exe", ""),
        registration(r"C:\Old\Discord.exe", ""),
        registration(r"C:\Apps\Steam.exe", "Steam"),
        registration(r"C:\Apps\Steam.exe", "Steam duplicate"),
    ];
    let processes = vec![
        process(10, r"C:\Program Files\AMD\RadeonSoftware.exe", "AMD Software"),
        process(11, r"D:\Steam\Steam.exe", "Steam"),
    ];

    let catalog = build_background_apps(&registrations, &processes);

    assert_eq!(labels(&catalog), ["AMD Software", "Steam"]);
    assert!(catalog.iter().all(|entry| !entry.executable().is_empty()));
}

#[test]
fn tooltip_is_bounded_to_its_first_nonempty_line() {
    assert_eq!(safe_label(" Zoom - Signed in\r\nPrivate detail ", "Zoom"), "Zoom - Signed in");
    assert_eq!(safe_label("", "Discord.exe"), "Discord");
}
```

- [x] **Step 2: Run the pure test and verify RED**

Run:

```powershell
cargo test -p shell-platform-windows background_apps --lib
```

Expected: compilation fails because the module and functions do not exist.

- [x] **Step 3: Implement the pure catalog**

Create focused pure types:

```rust
pub(crate) const MAX_NOTIFICATION_REGISTRATIONS: usize = 256;
pub(crate) const MAX_BACKGROUND_APPS: usize = 32;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct BackgroundAppId(u64);

impl BackgroundAppId {
    pub(crate) const fn new(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NotificationRegistration {
    executable: String,
    tooltip: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RunningProcess {
    process_id: u32,
    executable: String,
    description: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BackgroundAppEntry {
    id: BackgroundAppId,
    process_id: u32,
    label: String,
    executable: String,
    icon_source: String,
}
```

`build_background_apps` must:

```rust
pub(crate) fn build_background_apps(
    registrations: &[NotificationRegistration],
    processes: &[RunningProcess],
) -> Vec<BackgroundAppEntry> {
    let live = processes
        .iter()
        .filter_map(|process| executable_name(&process.executable).map(|name| (name, process)))
        .collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();
    let mut result = registrations
        .iter()
        .take(MAX_NOTIFICATION_REGISTRATIONS)
        .filter_map(|registration| {
            let name = executable_name(&registration.executable)?;
            let process = live.get(&name)?;
            seen.insert(name.clone()).then(|| BackgroundAppEntry::from_match(registration, process))
        })
        .take(MAX_BACKGROUND_APPS)
        .collect::<Vec<_>>();
    result.sort_by_key(|entry| entry.label.to_lowercase());
    result
}
```

Use a deterministic FNV-1a hash of the normalized executable path for `BackgroundAppId`. Keep accessors crate-private and never implement `Display` for entries containing paths.

- [x] **Step 4: Run tests and verify GREEN**

Run:

```powershell
cargo test -p shell-platform-windows background_apps --lib
```

Expected: stale, duplicate, label, ordering, and bounds tests pass.

### Task 3: Read-only Windows Adapter and on-demand worker

**Files:**

- Modify: `crates/shell-platform-windows/Cargo.toml`
- Create: `crates/shell-platform-windows/src/win32_background_apps.rs`
- Create: `crates/shell-platform-windows/src/background_apps_worker.rs`
- Modify: `crates/shell-platform-windows/src/lib.rs`
- Modify: `crates/shell-platform-windows/src/win32_event_queue.rs`
- Modify: `crates/shell-platform-windows/src/win32_windowing.rs`
- Test: inline tests in both new modules

- [x] **Step 1: Write failing Adapter seam and generation tests**

Test the worker with an injected source; it must emit the requested generation and never expose raw failures:

```rust
#[test]
fn worker_returns_the_matching_generation() {
    let source = FixedBackgroundAppsSource::ready(vec![entry(7, "Steam")]);
    let result = load_request(&source, 41);
    assert_eq!(result.generation(), 41);
    assert_eq!(result.snapshot().unwrap()[0].label(), "Steam");
}

#[test]
fn adapter_error_is_typed_and_path_free() {
    let error = BackgroundAppsError::RegistryUnavailable;
    assert_eq!(error.code(), "registry_unavailable");
    assert!(!format!("{error:?}").contains("Carlos"));
}
```

- [x] **Step 2: Run focused tests and verify RED**

Run:

```powershell
cargo test -p shell-platform-windows background_apps_worker --lib
```

Expected: compilation fails because the worker source and result types do not exist.

- [x] **Step 3: Add only the required Windows feature flags**

Add these `windows` features without adding another crate:

```toml
"Win32_System_Diagnostics_ToolHelp",
"Win32_System_Registry",
"Win32_System_ProcessStatus",
```

- [x] **Step 4: Implement bounded registry and process capture**

In `win32_background_apps.rs`, keep all FFI local and expose only:

```rust
pub(super) fn capture_background_apps() -> Result<Vec<BackgroundAppEntry>, BackgroundAppsError>;
pub(super) fn activate_background_app(
    entry: &BackgroundAppEntry,
    windows: &[ObservedWindow],
) -> windows::core::Result<()>;
```

The registry reader opens `HKCU\Control Panel\NotifyIconSettings` with `KEY_READ`, enumerates at most `MAX_NOTIFICATION_REGISTRATIONS` subkeys, reads `ExecutablePath` and `InitialTooltip`, and closes every `HKEY` with a guard. The process reader uses `CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)`, restricts candidates to the current session with `ProcessIdToSessionId`, and resolves the full path with `QueryFullProcessImageNameW`. Each native handle has one RAII guard and a documented safety invariant.

Do not log the registry strings, paths, tooltips, process IDs, or file descriptions.

- [x] **Step 5: Implement the on-demand worker and wake message**

Use a one-shot thread per user request; there is no persistent worker and no timer:

```rust
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BackgroundAppsLoadResult {
    generation: u64,
    snapshot: Result<Vec<BackgroundAppEntry>, BackgroundAppsError>,
}

pub(super) fn request_background_apps(generation: u64, wake_hwnd: NativeWindowId) {
    std::thread::spawn(move || {
        let result = BackgroundAppsLoadResult::new(generation, capture_background_apps());
        queue_event(RoutedPlatformEvent::window_id(
            wake_hwnd,
            PlatformEvent::BackgroundAppsLoaded(result),
        ));
        wake_window(wake_hwnd);
    });
}
```

Reserve a private `WM_APP + 0x62` wake message. `wake_window` uses `PostMessageW`; `window_proc` returns `LRESULT(0)` for that message without creating a second event. The normal message-loop drain then processes the queued result.

- [x] **Step 6: Run Adapter tests and Windows package check**

Run:

```powershell
cargo test -p shell-platform-windows background_apps --lib
cargo test -p shell-platform-windows background_apps_worker --lib
cargo check -p shell-platform-windows --all-targets
```

Expected: all pass; no raw registry or process data appears in test output.

### Task 4: Popover loading, actions, and bounded scrolling

**Files:**

- Modify: `crates/shell-platform-windows/src/popover_types.rs`
- Modify: `crates/shell-platform-windows/src/popover_controller.rs`
- Modify: `crates/shell-platform-windows/src/popover_adapters.rs`
- Modify: `crates/shell-platform-windows/tests/popover_controller.rs`
- Modify: `crates/shell-platform-windows/src/lib.rs`

- [x] **Step 1: Write failing loading, activation, and scroll tests**

```rust
#[test]
fn background_apps_loading_ignores_stale_results_and_scrolls_focus_into_view() {
    let mut controller = PopoverController::new();
    let first = controller.begin_loading(Popover::BackgroundApps);
    let current = controller.begin_loading(Popover::BackgroundApps);

    assert!(!controller.complete_background_apps(first, ready_items(20)));
    assert!(controller.complete_background_apps(current, ready_items(20)));
    for _ in 0..18 {
        controller.handle_key(PopoverKey::Next);
    }
    let scene = controller.scene().expect("background apps scene");
    assert!(scene.scroll_offset() > 0);
    assert_eq!(scene.focused(), Some(18));
}

#[test]
fn background_app_activation_emits_stable_id() {
    let mut controller = loaded_background_apps_controller();
    assert_eq!(
        controller.handle_key(PopoverKey::Activate),
        vec![QueuedPopoverAction::TypedIntent(
            PopoverAction::OpenBackgroundApp(BackgroundAppId::new(7))
        )]
    );
}
```

- [x] **Step 2: Run controller tests and verify RED**

Run:

```powershell
cargo test -p shell-platform-windows background_apps_loading
cargo test -p shell-platform-windows background_app_activation
```

Expected: compilation fails because loading generations, icon rows, and the action do not exist.

- [x] **Step 3: Extend item/action types without leaking native details**

```rust
pub enum PopoverAction {
    OpenBackgroundApp(BackgroundAppId),
}

pub struct PopoverItem {
    label: String,
    detail: String,
    icon_source: Option<String>,
    enabled: bool,
    action: Option<PopoverAction>,
}

impl PopoverItem {
    pub fn with_icon_source(mut self, source: Option<String>) -> Self {
        self.icon_source = source;
        self
    }
}
```

Map a successful snapshot to enabled rows, an empty snapshot to `PopoverLoadState::Empty`, and an Adapter failure to `PopoverLoadState::Error("background apps unavailable".to_owned())`.

Preserve the `PopoverLoadState::Error(String)` message in `ActivePopover` rather
than discarding it. The background-app states exposed to the scene are exactly
`Carregando aplicativos…`, `Nenhum aplicativo em segundo plano`, and
`Aplicativos em segundo plano indisponíveis` for loading, empty, and error.

- [x] **Step 4: Add generation-safe completion and scrolling**

Add `load_generation: u64` and `scroll_offset: usize` to `PopoverController`. `begin_loading` increments the generation, resets selection/scroll, and installs Loading. `complete_background_apps` changes state only when the active kind and generation still match. `dismiss` increments generation so late results are ignored.

Keep at most 17 visible 24-DIP rows. `Next`/`Previous` adjust `scroll_offset` so the focused row stays in `[offset, offset + 17)`. Add:

```rust
pub fn handle_scroll(&mut self, rows: isize) -> Vec<QueuedPopoverAction> {
    let maximum = self.items.len().saturating_sub(17);
    self.scroll_offset = self.scroll_offset.saturating_add_signed(rows).min(maximum);
    vec![QueuedPopoverAction::Redraw]
}
```

- [x] **Step 5: Run controller regression tests**

Run:

```powershell
cargo test -p shell-platform-windows popover_controller
```

Expected: all existing projection, calendar, confirmation, pointer, and dismissal tests remain green with the new background-app tests.

### Task 5: Renderer row icons, ellipsis, and scroll viewport

**Files:**

- Modify: `crates/shell-renderer/src/popover_scene.rs`
- Modify: `crates/shell-renderer/src/popover_layout.rs`
- Modify: `crates/shell-renderer/src/native_showcase.rs`
- Modify: `crates/shell-renderer/src/native_showcase_popover.rs`
- Modify: `crates/shell-renderer/src/lib.rs`
- Create: `crates/shell-renderer/tests/popover_scene.rs`

- [x] **Step 1: Write failing renderer contract tests**

```rust
#[test]
fn scrolled_popover_keeps_rows_inside_the_surface() {
    let scene = PopoverScene::new(
        Popover::BackgroundApps,
        "Aplicativos em segundo plano",
        PopoverContentState::Ready,
        rows(24),
        Some(18),
    )
    .with_scroll_offset(8);
    let layout = layout_popover_scene(&scene, DipRect::new(0.0, 0.0, 244.0, 420.0));
    assert_eq!(layout.rows().first().unwrap().index(), 8);
    let last = layout.rows().last().unwrap().bounds();
    assert!(last.y + last.height <= 415.0);
}

#[test]
fn background_row_carries_only_an_optional_icon_source() {
    let row = PopoverRow::new("Steam", "", true).with_icon_source(Some("C:\\Steam.exe"));
    assert_eq!(row.icon_source(), Some("C:\\Steam.exe"));
}
```

- [x] **Step 2: Run renderer tests and verify RED**

Run:

```powershell
cargo test -p shell-renderer popover --lib --tests
```

Expected: compilation fails because the scene has no scroll offset or icon source.

- [x] **Step 3: Extend the pure scene and layout**

Add `icon_source: Option<String>` to `PopoverRow`, plus `scroll_offset: usize`
and `status_text: String` to `PopoverScene`. Preserve existing constructors with
`None`, zero, and the existing English status text defaults; add
builders/accessors. `PopoverController` overrides the status text with the
three Portuguese background-app messages. Change layout iteration to:

```rust
for (index, row) in scene
    .rows()
    .iter()
    .enumerate()
    .skip(scene.scroll_offset())
{
    if y + 24.0 > surface.y + surface.height - 5.0 {
        break;
    }
    // existing bounded row placement
}
```

- [x] **Step 4: Draw cached icons and reserve text space**

Pass `&mut NativeIconCache` and the existing icon text format into `draw_functional_popover`. For rows with an icon source, draw a 18×18 DIP bitmap at `bounds.x .. bounds.x + 18` and start the label at `bounds.x + 26`. If `NativeIconCache::draw` returns false, draw one generic MDL2 application glyph in the same rectangle. Rows without an icon keep the existing full label bounds.

Keep the label's right edge before the detail column and use the existing no-wrap/ellipsis DirectWrite format. Do not load icons from layout or controller code.

- [x] **Step 5: Run renderer regression tests**

Run:

```powershell
cargo test -p shell-renderer popover --lib --tests
cargo test -p shell-renderer topbar_scene
```

Expected: icon, clipping, scroll, topbar containment, and existing layout tests pass.

### Task 6: Runtime integration and conservative activation

**Files:**

- Modify: `crates/shell-platform-windows/src/topbar_controller.rs`
- Modify: `crates/shell-platform-windows/tests/topbar_controller.rs`
- Modify: `crates/shell-platform-windows/src/lib.rs`
- Modify: `crates/shell-platform-windows/src/win32_owner.rs`
- Modify: `crates/shell-platform-windows/src/win32_popover_render.rs`
- Modify: `crates/shell-platform-windows/src/win32_windowing.rs`
- Modify: `crates/shell-platform-windows/src/win32_actions.rs`
- Modify: `crates/shell-platform-windows/src/win32_shell_observation.rs`

- [x] **Step 1: Write failing topbar and runtime-result tests**

```rust
#[test]
fn background_apps_module_opens_its_typed_popover() -> Result<(), Box<dyn Error>> {
    let mut controller = TopbarController::new(ShellState::default(), TopbarDensity::Compact)?;
    controller.update_surface(DipRect::new(0.0, 0.0, 1280.0, 32.0));
    let point = module_center(&controller, TopbarModuleKind::BackgroundApps)?;
    controller.handle_pointer(TopbarPointerSample::new(TopbarPointerPhase::Pressed, point))?;
    assert_eq!(
        controller.handle_pointer(TopbarPointerSample::new(TopbarPointerPhase::Released, point))?,
        vec![QueuedTopbarAction::OpenPopover {
            popover: Popover::BackgroundApps,
            anchor: TopbarOverlayAnchor::Module(TopbarModuleKind::BackgroundApps),
        }]
    );
    Ok(())
}
```

Add a pure runtime test proving a completed generation updates only an active matching popover and a stale completion leaves the current scene unchanged.

- [x] **Step 2: Run focused tests and verify RED**

Run:

```powershell
cargo test -p shell-platform-windows background_apps_module
cargo test -p shell-platform-windows background_apps_result
```

Expected: the topbar match is non-exhaustive and runtime result handling is absent.

- [x] **Step 3: Wire the topbar visual and loading request**

Map the module to an icon-only visual:

```rust
TopbarModuleKind::BackgroundApps => visual(
    kind,
    "\u{E74A}",
    "Apps",
    TopbarModuleStatus::Neutral,
    Some(TopbarIntent::Popover(Popover::BackgroundApps)),
),
```

When `apply_topbar_actions` receives `Popover::BackgroundApps`, call `begin_loading`, place/redraw/show the popover immediately, and call `request_background_apps(generation, native_window_id(topbar.hwnd))` after the loading surface is visible.

- [x] **Step 4: Apply worker results only to the current popover**

Store the latest successful entries in `RuntimeSurfaces.background_apps`. Handle:

```rust
PlatformEvent::BackgroundAppsLoaded(result) => {
    let (generation, captured) = result.into_parts();
    let (items, catalog) = match captured {
        Ok(entries) => (Ok(background_app_items(&entries)), entries),
        Err(error) => (Err(error), Vec::new()),
    };
    if self
        .popover_controller
        .complete_background_apps(generation, items)
    {
        self.background_apps = catalog;
        self.place_active_overlay(window_work_area(topbar.hwnd)?, topbar, dock, popover)?;
        self.redraw_popover(topbar, dock, popover, preview, settings)?;
    }
    return Ok(true);
}
```

Structure ownership so the result is consumed once rather than cloned. A stale or dismissed generation performs no redraw.

- [x] **Step 5: Activate a selected entry conservatively**

When `PopoverAction::OpenBackgroundApp(id)` arrives, look up the ID in the current bounded catalog. First match an `ObservedWindow` from the latest centralized `ShellObservation` and use the existing focus path. If none exists, call `ShellExecuteW` with the already resolved live executable path. Dismiss the popover after a successful request; on failure keep it open and record only `background_app_open_failed` plus the typed error code.

Retain the latest observed windows in `RuntimeSurfaces` when `apply_shell_observation` runs; do not start a second window discovery path.

- [x] **Step 6: Add mouse-wheel routing for long lists**

Translate `WM_MOUSEWHEEL` over the popover to `PlatformEvent::PopoverScroll(-wheel_delta.signum())`. Route it to `PopoverController::handle_scroll` and redraw. Existing keyboard navigation must continue to move selection and viewport together.

- [x] **Step 7: Run integration tests and Windows package check**

Run:

```powershell
cargo test -p shell-platform-windows topbar_controller
cargo test -p shell-platform-windows popover_controller
cargo test -p shell-platform-windows shell_observation_runtime
cargo check -p shell-platform-windows --all-targets
```

Expected: all pass with no actual user application launch in tests.

### Task 7: Documentation, native verification, and executable

**Files:**

- Modify: `README.md`
- Modify: `SUPPORT.md`
- Modify: `docs/superpowers/plans/2026-07-16-background-apps-topbar.md`

- [x] **Step 1: Document behavior and compatibility**

Document that the dropdown is read-only with respect to Explorer settings, refreshes only when opened, and safely shows unavailable if a future Windows version removes the registry contract. State that row activation opens/focuses the application rather than reproducing undocumented tray callbacks.

- [x] **Step 2: Run complete quality gates**

Run:

```powershell
cargo fmt --all --check
powershell -NoProfile -File scripts/Check-Architecture.ps1
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Expected: all commands exit successfully with no warning or failure.

- [x] **Step 3: Build an isolated release executable**

Run:

```powershell
$env:CARGO_TARGET_DIR = Join-Path $PWD 'target-background-apps'
cargo build --workspace --release
```

Expected executable:

```text
target-background-apps/x86_64-pc-windows-msvc/release/shell-app.exe
```

- [ ] **Step 4: Perform bounded native smoke verification**

Launch with `--qa-exit-ms 12000`, open the new topbar module, and verify representative running registrations such as AMD Software, Windows Security, Discord, Bing Wallpaper, Google Drive, ChatGPT, and Steam are present when their processes are live. Verify the popover stays within monitor bounds, wheel and keyboard scrolling work, names do not overlap, and closing/reopening refreshes the list.

Do not activate rows belonging to applications with unsaved work during automated smoke verification. Use one harmless singleton app or an injected QA entry for activation.

- [ ] **Step 5: Measure the on-demand cost**

Compare ten idle process samples before and after the feature with the dropdown closed. Then measure one dropdown open. Acceptance:

- no additional idle thread after the one-shot load completes;
- no periodic CPU increase while closed;
- working-set growth at rest no more than 3 MB;
- first list load completes without blocking pointer or redraw input;
- no `dock.redraw.slow` or popover presentation above 16 ms caused by registry/process/icon work.

- [x] **Step 6: Record evidence and deliver the executable**

Mark all plan checkboxes complete, add exact gate and performance results, calculate SHA-256, and provide a clickable absolute executable path. Inventory staged files before any commit and leave unrelated dirty files untouched.

## Evidence recorded on 2026-07-16

- `cargo fmt --all --check`: passed.
- `scripts/Check-Architecture.ps1`: passed for 7 workspace crates and 7 registered exceptions.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed.
- `cargo test --workspace`: passed; the platform package ran 215 tests and the renderer ran 68 tests with zero failures.
- Isolated release build: passed in `target-background-apps`.
- Release startup smoke: exited with code 0 after creating hardware-rendered slots on two monitors; the popover HWND remained hidden before interaction.
- Closed-menu idle sample: 0.0000 CPU seconds over the 6-second sample window, 83.17 MB average working set, 83.13–83.20 MB range, and 35–36 total process threads. The background-app worker is created only by the open action and has no idle timer.
- SHA-256: `48388078BE2FEAE4A8C45706CF4E72C68084C132204EA06DEB3C1250895C8716`.
- Interactive dropdown inspection and one-open cost remain a manual test because desktop-control approval expired before the click; no row was activated.
