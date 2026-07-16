# Responsive Liquid Glass Shell

## Objective

Translate the dock and menu-bar treatment from Figma node `682:3932` into the existing Windows-native Rust shell. Preserve the reference's proportions, layered glass, compact rhythm, and visual hierarchy while retaining Windows application identity, accessibility, multi-monitor behavior, and performance fallbacks.

The implementation is an original Windows translation. It does not ship Apple icons, SF Symbols, SF Pro, wallpaper assets, component exports, or other Apple-owned artwork from the reference.

## Scope

- Restyle the functional dock and topbar.
- Preserve Windows-resolved application icons and current dock actions.
- Preserve dynamic item count, running state, attention state, previews, auto-hide, keyboard focus, mixed-DPI placement, and multi-monitor behavior.
- Preserve the current topbar modules and popover intents. The reference controls visual structure, not unsupported macOS behavior.
- Do not redesign popover and settings content in this iteration, except where their anchor position must follow the new shell geometry.

## Reference Measurements

The Figma design context defines the following dock measurements:

| Element | Reference value | Native contract |
|---|---:|---:|
| Icon slot | 36 px | 36 DIP at 100% scale |
| Item height | 47 px | 47 DIP |
| Inter-item gap | 9 px | 9 DIP |
| Shell top padding | 8 px | 8 DIP |
| Shell horizontal padding | 8 px | 8 DIP |
| Item bottom padding | 4 px | 4 DIP |
| Dock height | 55 px | 55 DIP resting height |
| Background radius | 15 px | 15 DIP |
| Desktop edge offset | 4 px | 4 DIP from available work-area edge |
| Separator | 1 px by 36 px | One physical pixel by 36 DIP |

The width remains intrinsic rather than fixed:

`horizontal padding + item widths + inter-item gaps + separator groups`

The topbar remains 32 DIP high for Windows hit-testing and text metrics. It is flush with the physical top of each monitor, uses 10 DIP leading padding, 20 DIP trailing padding where space permits, and keeps all visible content on a single centered baseline.

## Visual Thesis

The shell should read as a thin optical layer above the desktop rather than an opaque dark capsule. The dock carries the stronger material and elevation; the topbar uses the same material family at lower visual weight so status information remains quiet.

Geometry follows the Figma reference. Typography and iconography remain native to Windows:

- `Segoe UI Variable` replaces SF Pro.
- Windows Shell and package-manifest icons replace all reference application artwork.
- Segoe/Fluent platform glyphs remain limited to owned status controls.

## Dock Material

The full-capability material is composed in this order:

1. Transparent composition surface.
2. Bounded Windows backdrop sampling where supported and stable. The compact
   popup dock uses deterministic Direct2D glass layers because native QA exposed
   intermittent black frames with DWM Acrylic; the full-width topbar may use the
   documented backdrop path.
3. Neutral luminance tint approximating the reference's 30% gray luminosity layer.
4. Dark 10% tonal veil to stabilize contrast over bright wallpapers.
5. White 8% reflected-light layer.
6. Half-pixel-equivalent dark outer separation stroke, snapped to one physical pixel.
7. Restrained top and bottom inner highlights.
8. Soft elevation shadow outside the content bounds.

Figma variables inform the effect strength rather than being copied as platform-independent shader values:

- Frost: `16`
- Depth: `30`
- Splay: `20`
- Refraction: `70`
- Dispersion: `20`
- Light angle: `0`

The initial Direct2D implementation represents these values through backdrop strength, tint opacity, rim contrast, and shadow spread. It must not introduce a continuous expensive blur or per-frame wallpaper capture.

## Dock Items and States

- Resting icons render at 36 DIP inside 36 DIP slots.
- Runtime icons retain their declared Windows artwork and aspect ratio.
- Transparent padding inside a third-party icon is not replaced with invented artwork.
- Hover magnification remains local and compositor-friendly: hovered item up to 1.22x, immediate neighbors up to 1.10x, second neighbors up to 1.04x.
- Magnification must not resize the host window or change intrinsic dock width during pointer movement.
- Running state uses a subtle three-DIP indicator below the icon.
- Focused state combines the running indicator with a low-opacity selected surface.
- Attention uses a contained warm badge, never a badge crossing the outer rim.
- Hover and pressed backgrounds are transient; resting items do not sit on permanent tiles.
- The separator is one physical pixel with reduced contrast.

## Topbar Material and Layout

- The topbar is a full-width, square-cornered strip at monitor `y = 0`.
- It uses the same backdrop/tint family as the dock at lower opacity and without dock elevation.
- A single subtle bottom separator provides edge definition; no top border or floating margin is present.
- Left content is app-owned: system menu/application label and any supported menu commands.
- Right content is app-owned status: network, volume, power, notifications, and clock.
- The implementation does not mirror or impersonate the Windows notification area.
- Text uses 13 DIP Semibold Segoe UI Variable with a 16 DIP line box.
- Icons and text share one vertical center. Values cannot wrap; narrow monitors collapse lower-priority modules into the existing overflow behavior.
- Network throughput stays compact on one line.

## Responsive Behavior

- Dock width follows the number of visible apps and separators.
- Dock and topbar recalculate physical pixels for the current monitor DPI.
- At high text scale, status labels collapse before clipping essential actions.
- The dock remains centered inside the monitor work area and respects the configured alignment when changed by the user.
- Auto-hide retains the configured reveal strip. The revealed dock uses the 55 DIP design height.
- Popovers re-anchor after geometry changes and remain clamped to the monitor work area.
- Fullscreen suppression and per-monitor visibility remain unchanged.

## Performance and Accessibility Fallbacks

The material degrades without changing layout or interaction:

1. Full: backdrop, layered tint, inner rim, shadow, and approved transform animation.
2. Reduced: cheaper backdrop, simplified shadow, and smaller magnification neighborhood.
3. Solid: opaque dimensional surface with rim and no backdrop sampling.
4. Essential/high contrast: Windows system colors with no decorative glass or shadow.

Reduced motion removes interpolation and keeps final states. Transparency-disabled and safe-mode paths must never render uncontrolled wallpaper directly behind text. Existing keyboard navigation, focus visibility, hit testing, and accessible labels remain release requirements.

## Implementation Boundaries

- `shell-renderer` owns material tokens, dock/topbar layout, and Direct2D drawing.
- `shell-platform-windows` owns backdrop capability, window placement, DPI, accessibility state, and runtime degradation selection.
- `shell-core` retains application identity and state; no visual effect logic enters the domain model.
- Theme configuration exposes semantic intensity/radius choices rather than raw Figma shader values.

## Verification Contract

The iteration is complete only when all of the following are observed in the running native app:

- Revealed dock is 55 DIP high at 100% scale and its width changes with item count.
- Dock uses 36 DIP icon slots, 9 DIP gaps, 8 DIP horizontal/top padding, and 15 DIP radius.
- Official Calculator and ChatGPT icons continue to resolve through Windows identity.
- Topbar is flush at the top on every monitor and its content does not wrap.
- Dock remains usable in revealed, hidden, hovered, focused, and running states.
- Mixed-DPI/multi-monitor placement and popover anchoring do not regress.
- Full and solid fallback materials both preserve readable contrast.
- Workspace tests, Clippy, release build, and native screenshot QA pass.

## Explicit Non-Goals

- Shipping Figma-exported Apple icons, wallpaper, fonts, or component assets.
- Reproducing macOS system menus or unsupported tray behavior.
- Applying the new material to every popover and settings page in this iteration.
- Adding a custom shader pipeline before the bounded native material has been profiled.
