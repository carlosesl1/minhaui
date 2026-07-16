# Apple-like Dock Wave Tuning

## Context

This increment refines the continuous dock magnification described in
`2026-07-13-dock-magnification-motion-design.md`. The spatial model, hit regions,
maximum scale, timer lifecycle, and renderer boundaries remain unchanged.

The current wave is continuous but its horizontal spring follows pointer changes
so quickly that the motion can feel rigid. The selected direction is a balanced,
Apple-like response: a short, perceptible inertia that remains connected to the
pointer and never bounces.

## Decision

Tune the existing critically damped `DockAnimator` instead of introducing a new
animation layer:

- horizontal position frequency: `14.0 Hz` to `10.0 Hz`;
- magnification strength frequency: `10.0 Hz` to `8.5 Hz`;
- material frequency remains `7.5 Hz`, preserving a small visual lag behind the
  icons;
- damping remains critical, so retargeting preserves velocity without overshoot;
- first entry still snaps the horizontal origin to the pointer and eases only the
  magnification strength;
- reduced-motion behavior continues to snap directly to the target.

The raised-cosine spatial influence, `1.22x` scale cap, stable hit slots, frame
timer, and frame-delta clamp are not changed.

## Alternatives Considered

1. **Tune the existing springs — selected.** It preserves the current architecture,
   refresh-rate independence, and constant per-frame work.
2. **Low-pass filter the pointer.** It is small but creates a less physical,
   constant trailing delay and discards velocity continuity.
3. **Use one spring per icon.** It could create a more pronounced ripple but adds
   state, tuning complexity, and unnecessary per-frame work.

## Observable Behavior

- Moving between adjacent icons produces a softer lateral handoff.
- The wave begins responding on the next animation frame and remains visually
  attached to the pointer.
- Rapid pointer direction changes remain continuous and do not overshoot.
- Entry and exit stay responsive and settle to exact resting geometry.
- No allocations, threads, timers, or renderer resources are added.

## Verification

- Add a focused test that distinguishes the softer first-frame position response
  from the previous `14 Hz` tuning while proving meaningful forward progress.
- Preserve the existing convergence, exit, non-finite input, and 60/144 Hz
  equivalence tests.
- Run platform and renderer tests, Clippy with warnings denied, and the release
  build.
- Compare native Win32 responsiveness with `dock.performance` diagnostics kept
  modular and disabled by default outside the measurement session.
