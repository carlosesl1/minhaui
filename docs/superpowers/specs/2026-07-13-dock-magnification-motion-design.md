# Continuous Dock Magnification Motion

## Objective

Replace the dock's index-based hover jumps with a continuous, Windows-native magnification system that follows the pointer smoothly, pushes neighboring icons aside, returns to rest without visible bounce, and consumes no animation frames after settling.

## Observable Behavior

- The icon under the pointer reaches the configured maximum size.
- Influence falls continuously with horizontal distance; crossing an icon boundary cannot cause a size jump.
- Neighboring icons move sideways by the accumulated growth of the icons between them and the pointer.
- Logical hit regions stay on the resting slots so a target does not move away from a click.
- Enter, movement, and exit preserve velocity through a critically damped spring.
- The dock returns to its exact resting geometry after exit.
- Reduced-motion mode snaps to the current target and does not run a frame timer.
- The animation timer exists only while at least one spring remains unsettled.
- The dock material lifts by at most two DIP, expands by at most six DIP, and shifts its reflected highlight toward the pointer without moving the HWND.

## Spatial Model

For an application icon with resting center `c`, pointer position `p`, and influence radius `r`:

```text
u = clamp(abs(p - c) / r, 0, 1)
w = 0.5 * (1 + cos(pi * u)) when u < 1, otherwise 0
size = base + (maximum - base) * w * approach_strength
```

The raised-cosine kernel has zero slope at its center and edge. This prevents a visible derivative change when the pointer crosses an icon center or enters/leaves the influence radius.

Visual items are then laid out sequentially using their animated widths and the configured nine-DIP gap. The expanded group is centered on the resting group, causing neighbors to be pushed aside without overlap. The host window remains at its intrinsic resting width; the existing eight-DIP side insets absorb the bounded 1.22x expansion.

Separators move with the group but never scale. Resting item bounds remain the hit-test geometry.

## Temporal Model

The controller owns scalar springs for horizontal pointer position, icon approach strength, and material response. Each spring follows:

```text
acceleration = omega^2 * (target - value) - 2 * damping_ratio * omega * velocity
```

Use a near-critical damping ratio of `1.0`, a frequency tuned for a fast desktop response, semi-implicit integration, a clamped frame delta, and fixed substeps. This preserves consistent motion on 60, 120, and 144 Hz displays and avoids instability after a stalled frame.

On first entry, pointer position snaps to the cursor so the magnification does not sweep in from the left edge; only strength eases in. On pointer exit, the last horizontal position is retained while strength springs to zero.

The material spring uses a lower natural frequency than the icon spring. This slight delay gives the glass capsule more visual weight while keeping both systems critically damped and free of visible bounce.

## Material Response

The Direct2D material moves only inside the existing transparent composition surface. At rest it keeps a three-DIP horizontal inset and a two-DIP top inset. At full interaction it expands to the surface edges and lifts two DIP while retaining the same height. A narrow reflected-light band follows the animated pointer with bounded geometry. The Win32 window, auto-hide reveal zone, work-area placement, and resting hit regions never move.

## Runtime Integration

- `shell-renderer` owns the continuous influence curve, stable hit geometry, and pushed visual layout.
- `shell-renderer` also owns the bounded material geometry and moving reflected-light band.
- `DockAnimator` owns target/current spring state and deterministic time stepping.
- `DockController` retargets springs from pointer samples and publishes animated values in `DockScene`.
- The Win32 layer starts an eight-millisecond `WM_TIMER` only when motion becomes active, computes a monotonic delta per frame, redraws the dock, and destroys the timer when the animator settles.
- Reduced-motion mode calls the animator's snap operation instead of starting the timer.

No continuous wallpaper capture, shader, background thread, or permanent high-frequency timer is introduced.

## Verification

- Unit tests cover the continuous midpoint response, symmetric influence, outward displacement, non-overlap, stable hit regions, spring convergence, material lag/geometry, non-finite targets, and exit return.
- Platform tests cover pointer retargeting, intermediate motion, convergence, and reduced-motion snapping.
- Workspace formatting, renderer/platform tests, Clippy, release build, and a running native dock capture are required before completion.
