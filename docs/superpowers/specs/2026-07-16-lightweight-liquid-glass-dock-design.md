# Lightweight Liquid Glass Dock Design

## Objective

Create an opt-in visual prototype that makes the dock feel more like curved,
refractive glass without capturing desktop pixels, adding continuous idle
animation, or weakening the existing low-end hardware fallbacks.

The prototype is disabled by default and enabled only with:

```powershell
shell-app.exe --liquid-glass
```

## Scope

The prototype applies only to the dock material. It does not change topbar,
popover, preview, settings, layout, hit testing, icons, text, window discovery,
autohide behavior, or dock magnification geometry.

The experiment adds:

- a directional specular highlight;
- a restrained inner rim;
- a subtle cool/warm chromatic edge pair;
- optical depth driven by the existing dock material motion strength;
- static rendering when motion is reduced.

It does not add:

- desktop or monitor capture;
- physical refraction of windows behind the dock;
- a new blur pass;
- SVG, WebView, Chromium, WinUI, or Windows App SDK dependencies;
- a permanent render loop;
- distortion of icons, labels, or hit targets.

## Visual thesis

The dock should look like a compact convex glass capsule catching a controlled
light source from the upper-left. The body remains dark and legible. Liquid
character comes from edge energy and changing light response rather than from
large blur, transparency, or exaggerated rainbow fringing.

The memorable detail is a narrow specular band that subtly shifts toward the
active hover region while the dock body itself remains geometrically stable.

## Interaction thesis

The existing `material_motion_strength` is the only animation input. Pointer
interaction may change highlight intensity and optical depth, but it must not
start another timer or independent animation.

Reduced motion freezes the material in a restrained static state. With no
pointer interaction, the material performs no visual updates beyond the
existing dock lifecycle.

## Rendering architecture

### Feature ownership

`shell-app` owns command-line parsing and maps `--liquid-glass` into the private
Windows showcase configuration.

The Windows platform runtime carries the feature flag into the existing native
surface options. It does not own visual formulas.

`shell-renderer` owns the Liquid Glass material policy and Direct2D rendering.
The existing `CompositionRenderer` and dock surface continue to own device
resources.

### Material layers

The dock material remains a bounded rounded body and is rendered back-to-front:

1. existing outer rim;
2. existing luminance and veil;
3. subtle cool edge on one side;
4. subtle warm edge on the opposite side;
5. inner rim for thickness;
6. directional specular band;
7. existing cached inset depth texture;
8. icons and indicators through the unchanged scene renderer.

The chromatic edges must remain subordinate to content. At rest they should be
barely perceptible and must not tint icon interiors.

### Resource policy

Solid color or gradient resources are created with the surface resources and
reused across redraws. No brush, bitmap, text format, displacement map, or
effect graph is created inside the per-frame dock drawing path.

The first prototype uses geometric highlights and cached brushes. A real
Direct2D displacement map is explicitly deferred because the current DWM
backdrop is not available as a Direct2D input texture.

## Runtime modes

| Runtime condition | Behavior |
|---|---|
| Default launch | Current material; prototype disabled |
| `--liquid-glass`, hardware renderer | Dynamic lightweight material |
| `--liquid-glass --reduced-motion` | Static lightweight material |
| `--liquid-glass --force-warp` | Static or simplified lightweight material |
| Safe mode | Current solid material; Liquid Glass suppressed |
| High contrast | Current high-contrast/solid behavior; Liquid Glass suppressed |
| Backdrop unavailable | Solid readable fallback; no simulated transparency |

Explicit WARP remains independently selectable. The prototype must never force
WARP or change the hardware-first device policy.

## Data flow

```text
--liquid-glass
  -> AppConfig
  -> ShowcaseRunConfig
  -> SlotFeatures / RuntimeOptions
  -> NativeSurfaceOptions
  -> CompositionRenderer style
  -> dock material layers
```

The existing hover animator updates `material_motion_strength`. That value is
clamped and converted into highlight intensity and a small horizontal optical
bias. It does not alter dock bounds.

## Performance budget

The prototype targets:

- no additional background timer;
- no desktop capture or frame copy;
- no additional full-scene construction;
- no resource allocation in the redraw loop;
- no sustained dock redraw over 16 ms caused by the new material;
- idle CPU equivalent to the disabled path;
- working-set increase no greater than 3 MB in the representative two-monitor
  release test.

Diagnostics compare the existing `dock.redraw.slow` fields:

- `scene_us`;
- `present_us`;
- total redraw duration.

The A/B procedure runs the same release executable once normally and once with
`--liquid-glass`, using identical monitor layout and interaction sequence.

## Accessibility and fallbacks

The material does not carry state or meaning. Disabling it cannot remove
information.

Text and icon contrast must remain at least as strong as the current dock.
Reduced motion removes dynamic light movement. High contrast and safe mode
suppress the prototype entirely.

## Testing

Hermetic tests cover:

- command-line parsing defaults and opt-in behavior;
- launch configuration propagation;
- runtime material selection for hardware, WARP, safe mode, high contrast, and
  reduced motion;
- clamping and deterministic highlight geometry;
- unchanged dock material bounds at rest and during hover;
- absence of Liquid Glass layers when disabled.

Native validation covers:

- release startup with the flag disabled;
- release startup with `--liquid-glass`;
- release startup with `--liquid-glass --force-warp`;
- release startup with `--liquid-glass --safe-mode`;
- visual inspection of rest, hover, animation, autohide, and two-monitor DPI;
- A/B memory, CPU, and redraw timing measurements.

## Acceptance criteria

The prototype is accepted for further refinement when:

- the disabled path is visually and behaviorally unchanged;
- the enabled dock visibly gains convex glass depth without harming content;
- no overlap, hit-test, autohide, or projection regression appears;
- idle CPU does not measurably increase;
- working-set growth stays within 3 MB;
- representative hover/redraw measurements do not show sustained frame misses;
- safe mode, high contrast, reduced motion, and WARP degrade predictably.

If the performance budget fails, the dynamic specular movement is removed
first, followed by chromatic edging. The current material remains the final
rollback.

## Architectural impact

- Delivery class: M, because launch configuration, platform runtime, and native
  renderer are integrated.
- Modules affected: app argument parser, Slot Runtime configuration, Native
  Surface Runtime options, and dock material renderer.
- Interfaces: private configuration fields only; no public platform facade.
- State ownership: the launch flag is immutable; existing dock animation owns
  transient material strength.
- Dependencies: none.
- UI thread: bounded extra draw calls only; no capture, I/O, waiting, or new
  concurrency.
- Diagnostics: reuse the existing asynchronous dock performance module.
- Migration: none; the feature is not persisted.
- Rollback: remove the private flag and additional material layers.
- ADR: not required because no public interface, persistence, dependency,
  concurrency model, or process boundary changes.
