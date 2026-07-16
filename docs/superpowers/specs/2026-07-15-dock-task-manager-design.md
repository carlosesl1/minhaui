# Dock Task Manager Command Design

## Goal

Add one global command, **Abrir Gerenciador de Tarefas**, to the dock's native right-click menu. Selecting it launches the Windows Task Manager through the existing shell-launch path.

## Design

- Keep the existing Win32 popup menu and visual treatment unchanged.
- Place the new command immediately before `Quit`, after the app-specific commands.
- Model the selection as `ContextMenuCommand::OpenTaskManager` with its own stable native command ID.
- Translate the command into `QueuedDockAction::Launch("taskmgr.exe")` so process launching stays inside the existing action adapter.
- Do not add preferences, separators, taskbar controls, restart behavior, or other Finder/MyDockFinder-inspired actions in this change.

## Verification

- A controller test proves that the new native command ID is recognized.
- A controller interaction test proves that the command queues `taskmgr.exe` even when the click is not over an app item.
- Formatting, targeted tests, Clippy, workspace tests, and a release build must pass.
- Manual Windows QA must show the exact menu label and confirm that selecting it opens Task Manager.
