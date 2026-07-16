#![deny(unsafe_code)]

use shell_core::{AppId, DockItem, DockItemId, ShellState, WindowId};
use shell_renderer::TopbarDensity;
use windows::core::Result;

use crate::{
    DockController, DockRuntimeConfig, ObservedWindow, PreviewCapture, PreviewUnavailableReason,
    TopbarController,
};

pub(super) fn sample_dock_controller() -> Result<DockController> {
    let items = vec![
        DockItem::pinned(DockItemId::new(1), parse_app("notepad.exe")?),
        DockItem::pinned(DockItemId::new(2), parse_app("calc.exe")?),
        DockItem::pinned(DockItemId::new(3), parse_app("explorer.exe")?),
    ];
    let state = crate::win32_config::load_dock_state(ShellState::default().with_dock_items(items));
    let mut controller =
        DockController::new(state, DockRuntimeConfig::default().with_autohide(true))
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

pub(super) fn sample_topbar_controller(width: i32) -> Result<TopbarController> {
    TopbarController::new(ShellState::default(), density_for_width(width))
        .map_err(|error| windows::core::Error::new(invalid_arg(), error.to_string()))
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
