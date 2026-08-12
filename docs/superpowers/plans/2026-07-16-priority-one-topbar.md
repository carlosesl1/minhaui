# Priority One Topbar Functional Implementation Plan

> **Execution:** Follow this plan with `test-driven-development`,
> `systematic-debugging`, and `verification-before-completion`. Preserve the
> current visual design and do not stage, commit, push, revert, or delete
> unrelated work.

**Goal:** Implement the approved first-priority topbar functions as a working
Windows-native slice before visual refinement.

**Architecture:** Extend the pure topbar model and persisted configuration,
introduce typed topbar/popover intents, feed popovers from the live status
snapshot, and execute native behavior through one Windows action adapter. Keep
the current row renderer.

**Tech stack:** Rust 2024, Win32, shell-core, shell-config, shell-renderer,
DirectComposition runtime, Cargo tests, Clippy, rustfmt.

**Approved reference:** [Design specification](../specs/2026-07-16-priority-one-topbar-design.md)

**Architecture decision:** [ADR-0007](../../architecture/adr/0007-topbar-system-actions.md)

## Architectural impact

- Affected Modules: shell topbar state, configuration schema, topbar
  scene/layout, topbar/popover controllers, Windows status Adapter, and Windows
  system-action Adapter.
- Altered Interfaces: pure topbar intent carried through the existing renderer
  facade. Do not add public platform facade items or convenience geometry
  reexports.
- State ownership: ordered visibility in `ShellState`/`ShellConfigV1`; transient
  status in `TopbarController`; calendar offset in the popover data owner.
- Dependencies: no new workspace dependency and no new crate.
- UI thread: native commands are bounded and non-waiting; media metadata,
  package discovery, disk I/O, and joined WinRT operations are excluded.
- Diagnostics: reuse the bounded asynchronous platform diagnostics Adapter.
- Tests: hermetic tests cover all policy and mapping; native validation is
  separate and read-only for destructive capabilities.
- Migration/rollback: inject fixed leading modules at runtime; leave V1
  documents unchanged; no schema bump.

## Task 1: Extend the pure topbar model and persisted configuration

**Files:**

- Modify: `crates/shell-core/src/model.rs`
- Modify: `crates/shell-core/src/events.rs`
- Modify: `crates/shell-core/src/reducer.rs`
- Modify: `crates/shell-core/src/state.rs`
- Modify: `crates/shell-core/src/tests.rs`
- Modify: `crates/shell-config/src/schema.rs`
- Modify: `crates/shell-config/src/tests.rs`

1. Add failing tests for the new default order, visibility persistence, valid
   reorder, move-to-end, and unknown/duplicate rejection.
2. Add `AppIdentity` and `Search`.
3. Add `ReorderTopbarModule { module, before }`.
4. Add a config builder for the V1-compatible ordered status modules.
5. Run:

```powershell
cargo test -p shell-core topbar
cargo test -p shell-config topbar
```

## Task 2: Add typed topbar intents and active application status

**Files:**

- Modify: `crates/shell-renderer/src/topbar_scene.rs`
- Modify: `crates/shell-renderer/src/topbar_layout.rs`
- Modify: `crates/shell-renderer/src/lib.rs`
- Modify: `crates/shell-renderer/tests/topbar_scene.rs`
- Modify: `crates/shell-platform-windows/src/topbar_types.rs`
- Modify: `crates/shell-platform-windows/src/topbar_controller.rs`
- Modify: `crates/shell-platform-windows/src/win32_topbar_status.rs`
- Modify: `crates/shell-platform-windows/tests/topbar_controller.rs`

1. Add failing tests for typed hit testing, left-cluster order, Search dispatch,
   and active app text.
2. Add `TopbarIntent::{Popover, OpenSearch}`.
3. Add `app_label` and local date fields to `TopbarSnapshot`.
4. Read the foreground window identity through the existing app identity seam.
5. Keep a generic app glyph in this pass.
6. Run:

```powershell
cargo test -p shell-renderer topbar
cargo test -p shell-platform-windows topbar_controller
```

## Task 3: Make popover data dynamic and add calendar navigation

**Files:**

- Create: `crates/shell-core/src/calendar.rs`
- Modify: `crates/shell-platform-windows/src/lib.rs`
- Modify: `crates/shell-platform-windows/src/popover_types.rs`
- Modify: `crates/shell-platform-windows/src/popover_adapters.rs`
- Modify: `crates/shell-platform-windows/src/popover_controller.rs`
- Modify: `crates/shell-platform-windows/tests/popover_controller.rs`

1. Add failing tests for live network/audio/power rows, full system/control
   menus, Gregorian week rows, month boundaries, leap years, and previous/next
   calendar actions.
2. Extend typed popover actions for Windows pages, volume steps, mute, calendar
   navigation, Task Manager, and session commands.
3. Give the provider the latest snapshot and a bounded calendar offset.
4. Reload the open calendar after navigation.
5. Run:

```powershell
cargo test -p shell-platform-windows calendar
cargo test -p shell-platform-windows popover_controller
```

## Task 4: Implement the Windows native action adapter

**Files:**

- Create: `crates/shell-platform-windows/src/win32_system_actions.rs`
- Modify: `crates/shell-platform-windows/src/lib.rs`
- Modify: `crates/shell-platform-windows/src/win32_popover_render.rs`
- Modify: `crates/shell-platform-windows/src/win32_owner.rs`
- Modify: `crates/shell-platform-windows/Cargo.toml` only if a documented Windows
  API requires another feature.

1. Add failing pure tests for every action-to-command/input mapping and
   destructive-action classification.
2. Implement Search, Settings routes, Task Manager, global media keys, system
   volume mutation, lock, sleep, sign out, restart, and shutdown.
3. Update the provider with the latest snapshot before opening a popover.
4. Execute typed intents and redraw/reload only when required.
5. Anchor the popover to the activated topbar module bounds using the already
   approved `TopbarLayout`/laid-out item facade. Do not expose a new geometry
   helper type solely for this consumer.
6. Run:

```powershell
cargo test -p shell-platform-windows system_actions
cargo test -p shell-platform-windows popover
```

## Task 5: Add transactional topbar customization

**Files:**

- Modify: `crates/shell-platform-windows/src/settings_controller.rs`
- Modify: `crates/shell-platform-windows/tests/settings_controller.rs`
- Modify: runtime/config persistence files identified by the existing Settings
  apply path.

1. Add failing tests for preview/apply/cancel/reset of visibility and order.
2. Add `SettingsEdit` variants for visibility and reorder of configurable
   status modules.
3. Persist the applied configuration through the existing store.
4. Keep the visual settings form unchanged; expose stable controller behavior
   for the later UI pass.
5. Run:

```powershell
cargo test -p shell-platform-windows settings_controller
cargo test -p shell-config
```

## Task 6: Integrate and verify

1. Run focused suites after each integration seam.
2. Run the intentionally scoped verification:

```powershell
cargo fmt --all
cargo check -p shell-platform-windows --all-targets
powershell -NoProfile -File scripts/Check-Architecture.ps1
```

3. Leave native visual validation and UI polish for the explicit visual pass.

## Completion criteria

- Priority-one functions work through real Windows actions or live data.
- Calendar navigation is deterministic and tested.
- Topbar order and visibility are transactional and persisted.
- The current visual presentation remains intact.
- Deterministic workspace gates are green.
