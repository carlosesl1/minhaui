# Native Windows validation

The Windows validation boundary has two deliberately separate layers. A test
must fail when its prerequisite is absent; environmental tests are never
converted into conditional skips that can report a false green.

## Hosted, non-interactive gate

`windows-quality` runs on GitHub's `windows-latest` image and executes
`cargo test --workspace`. In addition to the deterministic suites, Windows now
exercises these real process contracts:

- a second process cannot acquire the per-session shell lock;
- terminating the owning process abnormally releases the lock;
- the watchdog kills a child that misses its bounded startup heartbeat;
- `--restore-only --check-only` is idempotent when no journal exists;
- a corrupt authoritative recovery journal fails closed and is preserved.

These tests do not claim that a visible shell, DWM composition, UI Automation,
RDP, or cross-integrity activation works. The hosted runner is not treated as a
controlled interactive desktop.

## Controlled interactive gate

The `native-windows-validation` job runs only through `workflow_dispatch` when
`run_native_validation` is explicitly enabled. Its self-hosted runner must have
the labels `windows`, `x64`, and `obsidian-native-validation`, an interactive
non-Session-0 desktop, DWM, Calculator, and Windows Terminal. No product
instance or conflicting AppBar may already be running.

The gate performs the existing package/AppBar probes and then launches the real
shell with WARP, safe mode, reduced motion, a synthetic device-loss/rebuild
event, and bounded display/resume/Explorer lifecycle events. A concurrent
second launch must
forward activation to the primary process and exit. The validation client then
uses Windows UI Automation to prove that Settings exposes:

- a visible Window root with non-empty screen bounds;
- the Settings sections List;
- an invokable Dock navigation item;
- the Icon size Slider and a working RangeValue pattern.

The RangeValue action changes the in-memory draft, waits until an external
client observes the new value, and restores the original value. The smoke never
applies or persists a configuration change. Safe mode and the absence of the
experimental feature/environment opt-in keep Explorer taskbar replacement
disabled. Successful evidence is uploaded from
`artifacts/native-windows-validation/summary.json` and retained for 14 days.

Local invocation on a prepared Windows desktop:

```powershell
cargo test -p shell-platform-windows --features native-validation --lib
cargo test -p shell-app --features native-validation --test showcase_startup
cargo build -p shell-app --features native-validation
powershell -NoProfile -ExecutionPolicy Bypass `
  -File scripts/Invoke-NativeWindowsValidation.ps1 `
  -ShellAppPath target/debug/shell-app.exe
```

## Gates that still require a lab or human evidence

This automation does not close Narrator or Accessibility Insights acceptance,
elevated-to-medium activation, RDP, Fast User Switching, mixed-DPI monitor
hotplug, sleep/resume, hardware-renderer device loss, visual parity, or a long
soak. Those scenarios must remain open release gates until evidence is captured
on the corresponding controlled topology.
