# Shell Runtime Performance Plan

**Goal:** Reduce repeated Windows polling and dock work without changing the
current UI.

**Risk:** M — integrated behavior across the process runtime, Shell Slots,
timers, rendering, and launch configuration.

## Architectural impact

- Modules: Shell Observation Runtime, Slot Runtime, Dock Runtime, Native
  Surface Runtime launch configuration.
- Interfaces: private process runtime and immutable Shell Observation; no new
  public facade.
- State owner: the process runtime owns discovery/status cache and its Poll
  Budget; each Shell Slot continues to own windows, controllers, and timers.
- Dependencies: none.
- UI thread: somente agenda capturas e aplica snapshots; um worker process-wide
  executa no máximo uma descoberta/status por vez e coalesce solicitações
  pendentes para a mais recente.
- Diagnostics: dock redraw timing remains opt-in and asynchronous, now split
  into scene construction and native present stages.
- Tests: pure cadence policies, process-level polling deduplication, launch
  mapping, render classification, workspace gates, and native smoke launch.
- Rollback: remove the process runtime fan-out and restore per-slot polling;
  timer and render optimizations are independently reversible.
- ADR: not required because no public interface, dependency, persistence,
  process boundary, or concurrency model changed.

## Delivery

- [x] Centralize window discovery and topbar status.
- [x] Move recurring discovery and status capture off the UI thread.
- [x] Separate safe mode from explicit WARP selection.
- [x] Make the dock edge probe cadence adaptive.
- [x] Measure dock redraw stages when diagnostics are enabled.
- [x] Skip width scene construction and state cloning on visual-only frames.
- [x] Complete workspace gates and produce the release executable.
