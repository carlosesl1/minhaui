use shell_renderer::native::ShowcaseRole;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{
    DWM_SYSTEMBACKDROP_TYPE, DWMSBT_NONE, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
    DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute,
};
use windows::core::BOOL;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BackdropRequest {
    None,
    Transient,
}

const fn request_for(role: ShowcaseRole, enabled: bool) -> BackdropRequest {
    match (enabled, role) {
        (true, ShowcaseRole::Settings) => BackdropRequest::Transient,
        (false, _)
        | (
            true,
            ShowcaseRole::Topbar
            | ShowcaseRole::Dock
            | ShowcaseRole::Popover
            | ShowcaseRole::Preview,
        ) => BackdropRequest::None,
    }
}

const fn value_for(request: BackdropRequest) -> DWM_SYSTEMBACKDROP_TYPE {
    match request {
        BackdropRequest::None => DWMSBT_NONE,
        BackdropRequest::Transient => DWMSBT_TRANSIENTWINDOW,
    }
}

pub(super) fn apply_if_supported(hwnd: HWND, role: ShowcaseRole, enabled: bool) -> bool {
    let request = request_for(role, enabled);
    if request == BackdropRequest::Transient {
        let dark_mode = BOOL::from(true);
        // SAFETY: Category 8 (FFI boundary). The live HWND and correctly sized BOOL
        // are read synchronously by the documented DWM dark-mode attribute.
        let _ = unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                std::ptr::from_ref(&dark_mode).cast(),
                std::mem::size_of_val(&dark_mode) as u32,
            )
        };
    }
    let backdrop = value_for(request);
    // SAFETY: Category 8 (FFI boundary). The live HWND is owned by the caller;
    // `backdrop` is a correctly sized documented DWM enum read synchronously.
    let applied = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            std::ptr::from_ref(&backdrop).cast(),
            std::mem::size_of_val(&backdrop) as u32,
        )
    }
    .is_ok();
    request == BackdropRequest::Transient && applied
}

#[cfg(test)]
mod tests {
    use super::{BackdropRequest, request_for, value_for};
    use shell_renderer::native::ShowcaseRole;
    use windows::Win32::Graphics::Dwm::{DWMSBT_NONE, DWMSBT_TRANSIENTWINDOW};

    #[test]
    fn custom_alpha_surfaces_never_request_dwm_backdrop() {
        assert_eq!(request_for(ShowcaseRole::Dock, true), BackdropRequest::None);
        assert_eq!(
            request_for(ShowcaseRole::Topbar, true),
            BackdropRequest::None
        );
        assert_eq!(
            request_for(ShowcaseRole::Popover, true),
            BackdropRequest::None
        );
        assert_eq!(
            request_for(ShowcaseRole::Settings, true),
            BackdropRequest::Transient
        );
        assert_eq!(
            request_for(ShowcaseRole::Preview, true),
            BackdropRequest::None
        );
        assert_eq!(
            request_for(ShowcaseRole::Dock, false),
            BackdropRequest::None
        );
        assert_eq!(
            request_for(ShowcaseRole::Popover, false),
            BackdropRequest::None
        );
    }

    #[test]
    fn disabled_backdrop_explicitly_clears_the_existing_window_material() {
        assert_eq!(value_for(BackdropRequest::None), DWMSBT_NONE);
        assert_eq!(
            value_for(BackdropRequest::Transient),
            DWMSBT_TRANSIENTWINDOW
        );
    }
}
