# Native Surface Runtime Implementation Plan

> **Execution:** Follow this plan with `test-driven-development`, `systematic-debugging`, and `verification-before-completion`. Work only in the canonical checkout. Do not stage, commit, push, delete, or revert unrelated user changes.

**Goal:** Extract ownership of the five Win32/DirectComposition surfaces from `RuntimeSurfaces` into a focused `NativeSurfaceRuntime`, preserving current behavior while reducing the blast radius of dock and overlay changes.

**Architecture:** `RuntimeSurfaces` remains the Win32 event coordinator. A new private runtime owns the renderer and the topbar, dock, popover, preview, and settings surfaces. Controllers continue to build fresh scenes; the new runtime only creates, replaces, resizes, draws, presents, animates, and transactionally rebuilds native resources. Presentation failures are normalized to one recovery policy at the module boundary.

**Tech stack:** Rust, Win32, DirectComposition, `shell-renderer`, PowerShell architecture gates, Cargo tests, Clippy, rustfmt, `cargo-deny`.

**Approved references:**

- [Design specification](../specs/2026-07-15-native-surface-runtime-design.md)
- [ADR-0005](../../architecture/adr/0005-native-surface-runtime.md)
- [Ubiquitous language](../../../CONTEXT.md)

## Scope and constraints

- This increment is an extraction, not a visual or behavioral redesign.
- Preserve window ownership, z-order, AppBar behavior, timers, animation timing, diagnostics, and logging semantics.
- Keep dock, topbar, preview, popover, context-menu, and settings controllers outside the new runtime.
- Never store borrowed or stale scenes in the new runtime.
- Rebuild all native resources after device loss; do not rebuild individual resources against a lost device.
- Retry a recoverable build failure once. Return the second failure without an unbounded retry loop.
- Install a new resource set only after all five surfaces and renderer are created successfully.
- Ensure surfaces are dropped before their renderer.
- Do not broaden the existing architecture exception expiry of 2026-07-29.
- Keep logs on existing non-blocking/modular paths; do not introduce synchronous file I/O on redraw or animation paths.

## File map

| Action | File | Responsibility |
| --- | --- | --- |
| Create | `crates/shell-platform-windows/src/win32_surface_runtime.rs` | Native renderer/surface ownership, build transaction, normalized present recovery, production and recording adapters |
| Modify | `crates/shell-platform-windows/src/lib.rs` | Declare the private runtime module |
| Modify | `crates/shell-platform-windows/src/win32_owner.rs` | Replace native resource fields with `NativeSurfaceRuntime`; keep event coordination and fresh scene construction |
| Modify | `crates/shell-platform-windows/src/win32_slots.rs` | Pass grouped native surface targets and keep slot/window lifecycle explicit |
| Modify | `crates/shell-platform-windows/src/win32_dock_render.rs` | Use role-level runtime operations instead of direct renderer/surface access |
| Modify | `crates/shell-platform-windows/src/win32_topbar_render.rs` | Use role-level runtime operations |
| Modify | `crates/shell-platform-windows/src/win32_dock_visibility.rs` | Use role-level opacity and animation operations |
| Modify | `crates/shell-platform-windows/src/win32_popover_render.rs` | Use the popover/settings role operations without moving controller behavior |
| Modify | `crates/shell-platform-windows/src/win32_preview_interaction.rs` | Use preview role operations while retaining DWM/controller state |
| Modify | `crates/shell-platform-windows/src/win32_preview_render.rs` | Use preview redraw/resize operations |
| Remove last | `crates/shell-platform-windows/src/win32_surface_present.rs` | Delete only after recovery policy and tests have moved |
| Modify | `scripts/ArchitecturePolicy.psm1` | Add the native-surface ownership boundary gate |
| Modify | `scripts/Test-ArchitecturePolicy.ps1` | Test both compliant and violating ownership fixtures |
| Modify if applicable | `architecture-exceptions.toml` | Remove, but never extend, an exception made obsolete by the extraction |
| Update | `docs/architecture/audits/` | Record evidence, remaining risks, and gate results |

## Task 1: Normalize presentation outcomes at the new boundary

**Files:**

- Create: `crates/shell-platform-windows/src/win32_surface_runtime.rs`
- Modify: `crates/shell-platform-windows/src/lib.rs`

1. Declare `mod win32_surface_runtime;` next to the other private Win32 modules.
2. Add a unit test first. The test must prove all four policy cases:
   - `Presented` becomes `SurfaceUpdate::Presented`.
   - `DeviceLost` becomes `SurfaceUpdate::RebuildAllRequired`.
   - a recoverable `Failed(HRESULT)` becomes `SurfaceUpdate::RebuildAllRequired`.
   - an unrecoverable `Failed(HRESULT)` remains an error.
3. Run the narrow test and confirm it fails because the new types/functions do not exist:

```powershell
cargo test -p shell-platform-windows win32_surface_runtime::tests::presentation_results_have_one_recovery_policy -- --exact
```

4. Implement the smallest policy surface:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SurfaceUpdate {
    Presented,
    RebuildAllRequired,
}

fn normalize_present_result(
    outcome: shell_renderer::native::PresentOutcome,
) -> windows::core::Result<SurfaceUpdate> {
    match outcome {
        shell_renderer::native::PresentOutcome::Presented => Ok(SurfaceUpdate::Presented),
        shell_renderer::native::PresentOutcome::DeviceLost(_) => {
            Ok(SurfaceUpdate::RebuildAllRequired)
        }
        shell_renderer::native::PresentOutcome::Failed(code)
            if shell_renderer::native::is_recoverable_hresult(code) =>
        {
            Ok(SurfaceUpdate::RebuildAllRequired)
        }
        shell_renderer::native::PresentOutcome::Failed(code) => {
            Err(windows::core::Error::from_hresult(code))
        }
    }
}
```

5. Keep this function private to the module. Re-run the narrow test and rustfmt:

```powershell
cargo test -p shell-platform-windows win32_surface_runtime::tests::presentation_results_have_one_recovery_policy -- --exact
cargo fmt --all -- --check
```

## Task 2: Define the resource transaction and recording adapter

**Files:**

- Modify: `crates/shell-platform-windows/src/win32_surface_runtime.rs`

1. Add recording-adapter tests first for these invariants:
   - a successful build creates roles in the deterministic order `Topbar`, `Dock`, `Popover`, `Preview`, `Settings`;
   - dock opacity is applied before the transaction is installed;
   - a failure after a partial build drops partial surfaces before retrying;
   - one recoverable failure retries exactly once;
   - a second recoverable failure returns an error and leaves the runtime not ready;
   - an unrecoverable failure does not retry.
2. Run the module tests and confirm they fail at compile time:

```powershell
cargo test -p shell-platform-windows win32_surface_runtime::tests
```

3. Introduce private input and ownership types. Keep scenes borrowed only for the duration of a call:

```rust
#[derive(Clone, Copy)]
pub(super) struct NativeSurfaceOptions {
    pub force_warp: bool,
    pub solid_material: bool,
}

pub(super) struct SurfaceTarget {
    pub hwnd: windows::Win32::Foundation::HWND,
    pub role: shell_renderer::native::ShowcaseRole,
    pub metrics: shell_renderer::native::SurfaceMetrics,
}

pub(super) struct SurfaceFrame<'scene> {
    pub target: SurfaceTarget,
    pub scenes: shell_renderer::native::ShellScenes<'scene>,
}

pub(super) struct SurfaceBuildPlan<'scene> {
    pub topbar: SurfaceFrame<'scene>,
    pub dock: SurfaceFrame<'scene>,
    pub popover: SurfaceFrame<'scene>,
    pub preview: SurfaceFrame<'scene>,
    pub settings: SurfaceFrame<'scene>,
    pub dock_opacity: f32,
}
```

4. Define a private `SurfaceAdapter` seam. It must cover only operations already needed by the production path:

```rust
trait SurfaceAdapter {
    type Renderer;
    type Surface;

    fn create_renderer(options: NativeSurfaceOptions) -> windows::core::Result<Self::Renderer>;
    fn device_kind(renderer: &Self::Renderer) -> shell_renderer::native::DeviceKind;
    fn create_surface(
        renderer: &Self::Renderer,
        frame: &SurfaceFrame<'_>,
    ) -> windows::core::Result<Self::Surface>;
    fn redraw_surface(
        renderer: &Self::Renderer,
        surface: &Self::Surface,
        role: shell_renderer::native::ShowcaseRole,
        scenes: shell_renderer::native::ShellScenes<'_>,
    ) -> windows::core::Result<shell_renderer::native::PresentOutcome>;
    fn resize_surface(
        renderer: &Self::Renderer,
        surface: &mut Self::Surface,
        metrics: shell_renderer::native::SurfaceMetrics,
        role: shell_renderer::native::ShowcaseRole,
        scenes: shell_renderer::native::ShellScenes<'_>,
    ) -> windows::core::Result<shell_renderer::native::PresentOutcome>;
    fn size(surface: &Self::Surface) -> (u32, u32);
    fn set_opacity(surface: &Self::Surface, opacity: f32) -> windows::core::Result<()>;
    fn animate_entrance(
        surface: &Self::Surface,
        reduced_motion: bool,
    ) -> windows::core::Result<()>;
    fn animate_visibility(
        surface: &Self::Surface,
        spec: shell_renderer::native::SurfaceVisibilityAnimation,
    ) -> windows::core::Result<()>;
}
```

5. Store all five surfaces in one non-optional `SurfaceSet<S>`. Wrap renderer and set in `SurfaceResources<R, S>` and implement an explicit `Drop` that takes and drops the set before the renderer.
6. Implement `NativeSurfaceRuntime<A>` with `options`, an adapter marker, and `resources: Option<SurfaceResources<A::Renderer, A::Surface>>`. Build into locals and assign `resources` only after all work succeeds.
7. Limit build attempts to two. Clear old resources before a rebuild. Classify retry eligibility with the renderer's existing recoverable-HRESULT policy.
8. Make the recording adapter capture creation, opacity, drop, retry, redraw, resize, and animation events without Win32/DirectComposition dependencies.
9. Re-run the module tests, then Clippy for the package:

```powershell
cargo test -p shell-platform-windows win32_surface_runtime::tests
cargo clippy -p shell-platform-windows --all-targets --all-features -- -D warnings
```

## Task 3: Add the production DirectComposition adapter

**Files:**

- Modify: `crates/shell-platform-windows/src/win32_surface_runtime.rs`

1. Add a test that exercises role lookup through the recording adapter and proves each operation targets only the requested role.
2. Implement `DirectCompositionAdapter` using:
   - `CompositionRenderer::new` for renderer creation;
   - `CompositionRenderer::create_surface` for creation;
   - `CompositionRenderer::redraw_surface` and `resize_surface` for frames;
   - `WindowSurface::{set_opacity, animate_entrance, animate_visibility}` for animation;
   - `WindowSurface::size` for size decisions.
3. Add private role accessors to `SurfaceSet` and expose only coordinator-safe operations on the concrete runtime:

```rust
pub(super) type Win32NativeSurfaceRuntime =
    NativeSurfaceRuntime<DirectCompositionAdapter>;

impl NativeSurfaceRuntime<DirectCompositionAdapter> {
    pub(super) fn new(options: NativeSurfaceOptions) -> Self;
    pub(super) fn device_kind(&self) -> Option<shell_renderer::native::DeviceKind>;
    pub(super) fn build(&mut self, plan: SurfaceBuildPlan<'_>) -> windows::core::Result<()>;
    pub(super) fn rebuild(&mut self, plan: SurfaceBuildPlan<'_>) -> windows::core::Result<()>;
    pub(super) fn size(
        &self,
        role: shell_renderer::native::ShowcaseRole,
    ) -> Option<(u32, u32)>;
    pub(super) fn redraw(
        &mut self,
        role: shell_renderer::native::ShowcaseRole,
        scenes: shell_renderer::native::ShellScenes<'_>,
    ) -> windows::core::Result<SurfaceUpdate>;
    pub(super) fn resize(
        &mut self,
        role: shell_renderer::native::ShowcaseRole,
        metrics: shell_renderer::native::SurfaceMetrics,
        scenes: shell_renderer::native::ShellScenes<'_>,
    ) -> windows::core::Result<SurfaceUpdate>;
    pub(super) fn set_opacity(
        &mut self,
        role: shell_renderer::native::ShowcaseRole,
        opacity: f32,
    ) -> windows::core::Result<()>;
    pub(super) fn animate_entrance(
        &mut self,
        role: shell_renderer::native::ShowcaseRole,
        reduced_motion: bool,
    ) -> windows::core::Result<()>;
    pub(super) fn animate_visibility(
        &mut self,
        role: shell_renderer::native::ShowcaseRole,
        spec: shell_renderer::native::SurfaceVisibilityAnimation,
    ) -> windows::core::Result<()>;
}
```

4. Do not expose `CompositionRenderer` or `WindowSurface` through any public accessor.
5. Run the narrow tests and package checks:

```powershell
cargo test -p shell-platform-windows win32_surface_runtime::tests
cargo check -p shell-platform-windows --all-targets --all-features
cargo clippy -p shell-platform-windows --all-targets --all-features -- -D warnings
```

## Task 4: Move initial build, rebuild, and device-kind ownership

**Files:**

- Modify: `crates/shell-platform-windows/src/win32_owner.rs`
- Modify: `crates/shell-platform-windows/src/win32_slots.rs`
- Modify: `crates/shell-platform-windows/src/win32_surface_runtime.rs`

1. Add or move tests that prove:
   - the coordinator builds fresh scenes for each full build;
   - all five grouped windows map to the correct roles;
   - `device_kind` delegates to the native runtime;
   - a requested full rebuild does not reuse an old scene.
2. In `RuntimeSurfaces`, replace these native ownership fields with one field named `surface_runtime`:

```text
force_warp
solid_material
renderer
topbar
dock
popover
preview
settings
```

3. Construct `Win32NativeSurfaceRuntime` from `RuntimeOptions` in `RuntimeSurfaces::new`.
4. Keep one coordinator method, `rebuild_native_surfaces`, that:
   - updates controller/model state required for scenes;
   - creates all scenes as fresh locals;
   - assembles `SurfaceBuildPlan` from `SurfaceWindows`;
   - calls `surface_runtime.rebuild(plan)` only after all unrelated mutable borrows have ended.
5. Keep Win32 window handles and the existing `SurfaceWindows<'_>` grouping outside the runtime. Do not move message-loop or window lifecycle ownership.
6. Replace `RuntimeSurfaces::device_kind` internals with delegation.
7. Run owner/runtime tests and the Windows package tests:

```powershell
cargo test -p shell-platform-windows win32_surface_runtime::tests
cargo test -p shell-platform-windows win32_owner
cargo test -p shell-platform-windows
```

## Task 5: Migrate topbar and dock frame rendering

**Files:**

- Modify: `crates/shell-platform-windows/src/win32_dock_render.rs`
- Modify: `crates/shell-platform-windows/src/win32_topbar_render.rs`
- Modify: `crates/shell-platform-windows/src/win32_owner.rs`

1. Add coordinator tests around the existing decision seams before changing production code:
   - unchanged dimensions choose redraw;
   - changed dimensions choose resize;
   - missing resources request a full build;
   - `RebuildAllRequired` is handled only after the runtime call returns;
   - diagnostics and QA markers are emitted in their existing order.
2. Replace direct `self.renderer`, `self.dock`, and `self.topbar` access with role-level `size`, `redraw`, and `resize` calls.
3. Keep scene construction in the coordinator. Pass each scene directly into a single runtime call and never persist it.
4. Avoid a nested mutable borrow: store the returned `SurfaceUpdate`, end the call/borrow, then call `rebuild_native_surfaces` if required.
5. Preserve all existing logging fields and events. Do not add per-frame synchronous logging.
6. Run focused tests after each role migration:

```powershell
cargo test -p shell-platform-windows dock_render
cargo test -p shell-platform-windows topbar_render
cargo test -p shell-platform-windows
```

## Task 6: Migrate dock opacity and visibility animation

**Files:**

- Modify: `crates/shell-platform-windows/src/win32_dock_visibility.rs`
- Modify: `crates/shell-platform-windows/src/win32_owner.rs`

1. Add recording tests proving dock opacity and entrance/visibility animation affect only the `Dock` role.
2. Replace direct dock-surface animation calls with `set_opacity`, `animate_entrance`, and `animate_visibility` role operations.
3. Keep DWM thumbnail state, timers, auto-hide policy, and window show/hide behavior in the coordinator.
4. Preserve the existing order between native-window visibility and composition animation.
5. Run focused and package tests:

```powershell
cargo test -p shell-platform-windows dock_visibility
cargo test -p shell-platform-windows win32_surface_runtime::tests
cargo test -p shell-platform-windows
```

## Task 7: Migrate popover, context menu, and settings rendering

**Files:**

- Modify: `crates/shell-platform-windows/src/win32_popover_render.rs`
- Modify: `crates/shell-platform-windows/src/win32_owner.rs`

1. Add tests around the existing overlay decision logic before changing ownership. Cover:
   - popover redraw and resize;
   - context-menu content rendered through the shared popover surface role;
   - settings redraw and resize;
   - device-loss propagation to a full rebuild;
   - unchanged overlay z-order and visibility sequencing.
2. Replace direct renderer/surface access with runtime operations for `Popover` and `Settings`.
3. Keep controller state, overlay selection, positioning, focus, visibility, and event routing outside the runtime.
4. Do not introduce separate native surface ownership for the context menu in this increment; retain the existing shared-popover behavior.
5. Run focused tests and the package suite:

```powershell
cargo test -p shell-platform-windows popover
cargo test -p shell-platform-windows settings
cargo test -p shell-platform-windows
```

## Task 8: Migrate preview rendering without moving DWM state

**Files:**

- Modify: `crates/shell-platform-windows/src/win32_preview_interaction.rs`
- Modify: `crates/shell-platform-windows/src/win32_preview_render.rs`
- Modify: `crates/shell-platform-windows/src/win32_owner.rs`

1. Add tests for the preview branch order: create/full-build when unavailable, resize when dimensions differ, redraw otherwise, then full rebuild on recoverable presentation failure.
2. Replace preview renderer/surface access with `Preview` role operations.
3. Keep DWM thumbnail registration, source-window identity, preview controller state, positioning, timers, and interaction routing outside `NativeSurfaceRuntime`.
4. Verify closing or replacing a preview does not rebuild unrelated surfaces unless the renderer reports device loss.
5. Run focused tests and the package suite:

```powershell
cargo test -p shell-platform-windows preview
cargo test -p shell-platform-windows win32_surface_runtime::tests
cargo test -p shell-platform-windows
```

## Task 9: Remove the shallow presentation helper and enforce ownership

**Files:**

- Modify: `scripts/ArchitecturePolicy.psm1`
- Modify: `scripts/Test-ArchitecturePolicy.ps1`
- Modify: `crates/shell-platform-windows/src/lib.rs`
- Remove: `crates/shell-platform-windows/src/win32_surface_present.rs`
- Modify: `crates/shell-platform-windows/src/win32_surface_runtime.rs`

1. Add failing architecture-policy fixtures before changing the policy. The fixture set must catch:
   - a native renderer field added back to `RuntimeSurfaces`;
   - a native surface field added back to `RuntimeSurfaces`;
   - direct `CompositionRenderer` or `WindowSurface` ownership outside `win32_surface_runtime.rs`;
   - a presentation helper that accepts `&mut RuntimeSurfaces` to rebuild resources.
2. Add a compliant fixture that uses only the role-level `NativeSurfaceRuntime` interface.
3. Run the policy tests and confirm the new violation fixtures initially fail to be detected:

```powershell
pwsh -NoProfile -File scripts/Test-ArchitecturePolicy.ps1
```

4. Implement `Test-NativeSurfaceOwnershipPolicy` in `ArchitecturePolicy.psm1`. Scope the rule to ownership/dependency declarations so ordinary references in tests, diagnostics, and adapter implementation do not create false positives.
5. Move `surface_size_changed` coverage into `win32_surface_runtime` tests.
6. Search for consumers before deletion:

```powershell
rg -n "win32_surface_present|handle_present|self\.(renderer|topbar|dock|popover|preview|settings)" crates/shell-platform-windows/src
```

7. Remove `win32_surface_present.rs` and its module declaration only when the search has no legitimate consumer.
8. Run policy and package tests:

```powershell
pwsh -NoProfile -File scripts/Test-ArchitecturePolicy.ps1
pwsh -NoProfile -File scripts/Check-ArchitecturePolicy.ps1
cargo test -p shell-platform-windows
```

## Task 10: Reconcile governance records and temporary exceptions

**Files:**

- Modify if applicable: `architecture-exceptions.toml`
- Create: `docs/architecture/audits/2026-07-15-native-surface-runtime.md`
- Modify if implementation differs materially: `docs/architecture/adr/0005-native-surface-runtime.md`
- Modify if terminology changes: `CONTEXT.md`

1. Run strict Clippy before changing exceptions:

```powershell
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

2. Remove an exception only when its violation no longer exists and the strict gate stays green. Never extend the existing expiry date.
3. Write the audit with:
   - the exact ownership boundary achieved;
   - before/after structural search evidence;
   - test and gate results;
   - any behavior that intentionally remains coordinated by `RuntimeSurfaces`;
   - known follow-up candidates for `DockRuntime` and `OverlayRuntime` without declaring them implemented.
4. Update ADR-0005 only if the implementation required a durable decision change. Record deviations explicitly rather than silently rewriting the approved intent.
5. Run the architecture checks again after documentation and exception changes.

## Task 11: Run the full delivery gates

**Files:**

- Verify the complete workspace; change code only to correct failures caused by this increment.

1. Confirm formatting:

```powershell
cargo fmt --all -- --check
```

2. Run architecture policy tests and the real checkout policy:

```powershell
pwsh -NoProfile -File scripts/Test-ArchitecturePolicy.ps1
pwsh -NoProfile -File scripts/Check-ArchitecturePolicy.ps1
```

3. Run strict lint, hermetic tests, release build, and dependency policy:

```powershell
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo build --workspace --release
cargo deny check
```

4. Run native validation separately. If it remains red only for the previously recorded packaging/AppBar environment constraints, preserve that distinction in the audit; do not report it as a green gate:

```powershell
pwsh -NoProfile -File scripts/Validate-NativeRuntime.ps1
```

5. Run final structural searches:

```powershell
rg -n "CompositionRenderer|WindowSurface" crates/shell-platform-windows/src
rg -n "self\.(renderer|topbar|dock|popover|preview|settings)" crates/shell-platform-windows/src
rg -n "handle_present|win32_surface_present" crates/shell-platform-windows/src
```

Expected result: production ownership of `CompositionRenderer` and `WindowSurface` is confined to `win32_surface_runtime.rs`; no stale direct native-resource fields or old presentation helper remain.

6. Review the working tree without modifying unrelated changes:

```powershell
git status --short
git diff --check
git diff -- crates/shell-platform-windows/src scripts docs/architecture CONTEXT.md
```

7. Do not stage, commit, or push unless the user explicitly authorizes those actions after reviewing the implementation and gate evidence.

## Completion criteria

- `RuntimeSurfaces` no longer owns the renderer or any of the five native surfaces.
- `NativeSurfaceRuntime` owns exactly one renderer and the five role surfaces.
- Build/rebuild is transactional, bounded to one retry, and covered by recording-adapter tests.
- Presentation recovery has one normalized module policy.
- Scene construction and all behavioral controllers remain outside the native runtime.
- Dock and overlay features use role-level operations and cannot directly access native surface resources.
- The architecture policy prevents native ownership from drifting back into the coordinator.
- All deterministic gates are green.
- Native validation status is reported honestly and separately.
- No unrelated user changes were staged, reverted, deleted, committed, or pushed.
