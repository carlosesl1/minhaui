#![deny(unsafe_code)]

use shell_config::{DockAlignmentPreference, ShellConfigV1};
use shell_core::{AppId, DockItem, DockItemId, ShellState, WindowId};
use shell_renderer::{DockAlignment, DockLayoutConfig, TopbarDensity};
use windows::core::Result;

use crate::{
    DockController, DockRuntimeConfig, ObservedWindow, PreviewCapture, PreviewUnavailableReason,
    TopbarController,
};

pub(super) fn sample_dock_controller(config: &ShellConfigV1) -> Result<DockController> {
    let items = vec![
        DockItem::pinned(DockItemId::new(1), parse_app("notepad.exe")?),
        DockItem::pinned(DockItemId::new(2), parse_app("calc.exe")?),
        DockItem::pinned(DockItemId::new(3), parse_app("explorer.exe")?),
    ];
    let state = crate::win32_config::state_from_config(
        config,
        ShellState::default().with_dock_items(items),
    );
    let mut controller = DockController::new(state, dock_runtime_config(config))
        .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))?;
    if std::env::var_os("MINHA_UI_QA_RESTRICTED_PREVIEW").is_some() {
        controller
            .sync_running_windows_with_previews(&[ObservedWindow::new(
                WindowId::new(1),
                parse_app("notepad.exe")?,
                true,
                false,
            )
            .with_preview(PreviewCapture::restricted(
                PreviewUnavailableReason::CaptureRestricted,
            ))])
            .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))?;
        controller.hovered_item = Some(DockItemId::new(1));
    }
    Ok(controller)
}

pub(super) fn sample_topbar_controller(
    width: i32,
    config: &ShellConfigV1,
) -> Result<TopbarController> {
    TopbarController::new(
        crate::win32_config::state_from_config(config, ShellState::default()),
        density_for_width(width),
    )
    .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))
}

pub(super) fn dock_runtime_config(config: &ShellConfigV1) -> DockRuntimeConfig {
    let settings = config.dock();
    let alignment = match settings.alignment() {
        DockAlignmentPreference::Left => DockAlignment::Left,
        DockAlignmentPreference::Center => DockAlignment::Center,
        DockAlignmentPreference::Right => DockAlignment::Right,
    };
    let item_size = f32::from(settings.item_size());
    let magnified_item_size = item_size * f32::from(settings.magnification()) / 100.0;
    let padding = DockLayoutConfig::default()
        .padding()
        .max((magnified_item_size - item_size).max(0.0).ceil());
    DockRuntimeConfig::new(alignment)
        .with_item_size(item_size)
        .with_spacing(f32::from(settings.spacing()))
        .with_padding(padding)
        .with_magnified_item_size(magnified_item_size)
        .with_autohide(config.autohide())
}

pub(super) const fn density_for_width(width: i32) -> TopbarDensity {
    if width < 700 {
        TopbarDensity::Compact
    } else {
        TopbarDensity::Comfortable
    }
}

fn parse_app(value: &str) -> Result<AppId> {
    AppId::parse(value).map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))
}

const fn invalid_arg() -> windows::core::HRESULT {
    windows::core::HRESULT(0x8007_0057_u32 as i32)
}

#[cfg(test)]
mod tests {
    use shell_config::{DockAlignmentPreference, DockSettings, ShellConfigV1};
    use shell_renderer::DockAlignment;

    use super::dock_runtime_config;

    #[test]
    fn default_persisted_dock_settings_preserve_the_existing_geometry() {
        let runtime = dock_runtime_config(&ShellConfigV1::default());

        assert_eq!(runtime.alignment(), DockAlignment::Center);
        assert_eq!(runtime.layout().item_size(), 36.0);
        assert_eq!(runtime.layout().spacing(), 9.0);
        assert_eq!(runtime.layout().magnified_item_size(), 43.92);
        assert_eq!(runtime.dock_height_dip(), 55.0);
        assert!(!runtime.autohide());
    }

    #[test]
    fn maximum_persisted_size_expands_padding_and_surface_without_clipping() {
        let dock = DockSettings::default()
            .with_item_size(72)
            .with_spacing(20)
            .with_alignment(DockAlignmentPreference::Right)
            .with_magnification(140);
        let config = ShellConfigV1::default().with_dock(dock).with_autohide(true);

        let runtime = dock_runtime_config(&config);

        assert_eq!(runtime.alignment(), DockAlignment::Right);
        assert_eq!(runtime.layout().item_size(), 72.0);
        assert_eq!(runtime.layout().spacing(), 20.0);
        assert_eq!(runtime.layout().magnified_item_size(), 100.8);
        assert_eq!(runtime.layout().padding(), 29.0);
        assert_eq!(runtime.dock_height_dip(), 112.0);
        assert!(runtime.autohide());
    }
}
