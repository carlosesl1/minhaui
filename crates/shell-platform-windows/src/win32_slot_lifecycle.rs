#![deny(unsafe_code)]

use shell_core::MonitorId;
use windows::core::Result;

use crate::win32_appbar::{TopbarCreateOptions, TopbarWindow};
use crate::win32_owner::{RuntimeOptions, RuntimeSurfaces, SurfaceWindows};
use crate::win32_sample_state::{sample_dock_controller, sample_topbar_controller};
use crate::win32_shell_observation::ShellObservation;
use crate::win32_slots::{ShellSlot, SlotFeatures, print_monitor_placements};
use crate::win32_timer::TimerGuard;
use crate::win32_window::{OwnedWindow, WindowClass};
use crate::win32_windowing::monitor_placement_inputs;
use crate::{MonitorPlacementInput, SlotReconcileAction, reconcile_monitor_slots};

pub(super) fn create_slot(
    class: &WindowClass,
    monitor: MonitorPlacementInput,
    features: SlotFeatures,
    config: &shell_config::ShellConfigV1,
    observation: &ShellObservation,
) -> Result<ShellSlot> {
    let text_scale = configured_text_scale();
    let topbar = TopbarWindow::create(
        class,
        monitor.bounds(),
        TopbarCreateOptions::new(features.backdrop_enabled, text_scale),
    )?;
    let work_area = topbar.work_area()?;
    let mut dock = OwnedWindow::create(
        class,
        shell_renderer::native::ShowcaseRole::Dock,
        work_area,
        features.backdrop_enabled,
    )?;
    let popover = OwnedWindow::create(
        class,
        shell_renderer::native::ShowcaseRole::Popover,
        work_area,
        features.backdrop_enabled,
    )?;
    let app_menu = OwnedWindow::create(
        class,
        shell_renderer::native::ShowcaseRole::AppMenu,
        work_area,
        features.backdrop_enabled,
    )?;
    let preview = OwnedWindow::create(
        class,
        shell_renderer::native::ShowcaseRole::Preview,
        work_area,
        features.backdrop_enabled,
    )?;
    let settings = OwnedWindow::create(
        class,
        shell_renderer::native::ShowcaseRole::Settings,
        work_area,
        features.backdrop_enabled,
    )?;
    let mut dock_controller = sample_dock_controller(config)?;
    let actions = dock_controller
        .sync_running_windows_with_previews(observation.windows())
        .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))?;
    crate::win32_actions::apply_dock_actions(&actions, dock_controller.state())?;
    dock.set_dock_width(
        work_area,
        dock_controller.preferred_width_dip(),
        dock_controller.config(),
        false,
    )?;
    let mut topbar_controller = sample_topbar_controller(topbar.rect.width, config)?;
    topbar_controller.update_snapshot(observation.topbar().clone());
    topbar_controller.update_text_scale(text_scale);
    let runtime = RuntimeSurfaces::new(
        RuntimeOptions {
            force_warp: features.force_warp,
            solid_material: features.solid_material,
            reduced_motion: features.reduced_motion,
            liquid_glass: features.liquid_glass,
        },
        SurfaceWindows {
            topbar: &topbar,
            dock: &dock,
            popover: &popover,
            preview: &preview,
            settings: &settings,
        },
        &app_menu,
        dock_controller,
        topbar_controller,
        config.clone(),
    )?;
    let sync_timer = TimerGuard::start_sync(dock.hwnd)?;
    Ok(ShellSlot {
        _sync_timer: sync_timer,
        runtime,
        monitor: monitor.monitor(),
        topbar,
        dock,
        popover,
        app_menu,
        preview,
        settings,
    })
}

pub(super) fn reconcile_slots(
    class: &WindowClass,
    slots: &mut Vec<ShellSlot>,
    features: SlotFeatures,
    config: &shell_config::ShellConfigV1,
    observation: &ShellObservation,
) -> Result<()> {
    let monitors = monitor_placement_inputs()?;
    if monitors.is_empty() {
        return Err(windows::core::Error::new(
            invalid_arg(),
            "no display monitors were enumerated",
        ));
    }
    print_monitor_placements(&monitors);
    let current = slots.iter().map(|slot| slot.monitor).collect::<Vec<_>>();
    let actions = reconcile_monitor_slots(&current, &monitors);
    let mut old = std::mem::take(slots);
    let mut next = Vec::new();
    for action in actions {
        match action {
            SlotReconcileAction::Remove(monitor) => remove_slot(&mut old, monitor),
            SlotReconcileAction::Create(monitor) => next.push(create_slot(
                class,
                monitor_input(&monitors, monitor)?,
                features,
                config,
                observation,
            )?),
            SlotReconcileAction::Reuse(monitor) => {
                if let Some(mut slot) = take_slot(&mut old, monitor) {
                    slot.reconcile_monitor(monitor_input(&monitors, monitor)?)?;
                    next.push(slot);
                }
            }
        }
    }
    drop(old);
    for slot in &next {
        slot.show_shells();
        slot.print_windows();
    }
    *slots = next;
    Ok(())
}

fn remove_slot(slots: &mut Vec<ShellSlot>, monitor: MonitorId) {
    if let Some(index) = slots.iter().position(|slot| slot.monitor == monitor) {
        drop(slots.remove(index));
        println!("SLOT_REMOVED monitor={}", monitor.value());
    }
}

fn take_slot(slots: &mut Vec<ShellSlot>, monitor: MonitorId) -> Option<ShellSlot> {
    slots
        .iter()
        .position(|slot| slot.monitor == monitor)
        .map(|index| slots.remove(index))
}

fn monitor_input(
    monitors: &[MonitorPlacementInput],
    monitor: MonitorId,
) -> Result<MonitorPlacementInput> {
    monitors
        .iter()
        .copied()
        .find(|input| input.monitor() == monitor)
        .ok_or_else(|| windows::core::Error::new(invalid_arg(), "monitor missing during reconcile"))
}

const fn invalid_arg() -> windows::core::HRESULT {
    windows::core::HRESULT(-2_147_024_809)
}

fn configured_text_scale() -> f32 {
    std::env::var("MINHA_UI_TEXT_SCALE")
        .ok()
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(1.0)
        .clamp(1.0, 2.5)
}
