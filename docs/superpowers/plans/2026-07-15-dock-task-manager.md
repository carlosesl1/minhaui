# Dock Task Manager Command Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an “Abrir Gerenciador de Tarefas” command to the dock's native context menu and launch `taskmgr.exe` when selected.

**Architecture:** Extend the typed context-menu command enum, map the command to the existing queued launch action in `DockController`, and append the label in the Win32 menu adapter. Reuse the existing `ShellExecuteW` launch path so no new process-launch boundary is introduced.

**Tech Stack:** Rust 2024, Windows crate/Win32 menus, Cargo test and Clippy.

---

### Task 1: Lock the command contract with tests

**Files:**
- Modify: `crates/shell-platform-windows/tests/dock_controller_config.rs`
- Modify: `crates/shell-platform-windows/tests/dock_controller_interactions.rs`

- [ ] Add native command ID 7 to `native_context_menu_ids_map_to_controller_commands` and expect `ContextMenuCommand::OpenTaskManager`.
- [ ] Add `task_manager_context_command_queues_the_windows_task_manager` using a point outside all app items and expect `QueuedDockAction::Launch("taskmgr.exe".to_owned())`.
- [ ] Run `cargo test -p shell-platform-windows --test dock_controller_config --test dock_controller_interactions`; expect compilation to fail because the enum variant does not exist yet.

### Task 2: Implement the typed command and menu row

**Files:**
- Modify: `crates/shell-platform-windows/src/dock_types.rs`
- Modify: `crates/shell-platform-windows/src/dock_controller_interaction.rs`
- Modify: `crates/shell-platform-windows/src/win32_context_menu.rs`

- [ ] Add `OpenTaskManager` with native ID 7 to both exhaustive enum mappings.
- [ ] Map `OpenTaskManager` to `QueuedDockAction::Launch("taskmgr.exe".to_owned())` without depending on dock-item hit testing.
- [ ] Append `Abrir Gerenciador de Tarefas` immediately before `Quit` in the existing native menu.
- [ ] Run the targeted tests again; expect both test binaries to pass.

### Task 3: Verify the native feature

**Files:**
- No additional files.

- [ ] Run `cargo fmt --all -- --check`; expect exit code 0.
- [ ] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`; expect exit code 0.
- [ ] Run `cargo test --workspace`; expect exit code 0.
- [ ] Run `cargo build --workspace --release`; expect exit code 0.
- [ ] Launch the native showcase, right-click the dock, verify the exact new label, select it, and confirm Windows Task Manager opens.
