use shell_core::WindowId;
use shell_renderer::{PhysicalRect, WindowPreviewCapture, WindowPreviewVisual};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{
    DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION,
    DWM_TNP_SOURCECLIENTAREAONLY, DWM_TNP_VISIBLE, DwmRegisterThumbnail, DwmUnregisterThumbnail,
    DwmUpdateThumbnailProperties,
};
use windows::core::{BOOL, Result};

use crate::{PreviewCapture, PreviewUnavailableReason};

pub(super) struct DwmPreviewThumbnail {
    thumbnail: isize,
    window: WindowId,
}

impl DwmPreviewThumbnail {
    pub(super) fn show(
        host: HWND,
        preview: WindowPreviewVisual,
        destination: PhysicalRect,
    ) -> Result<Option<Self>> {
        if !matches!(preview.capture(), WindowPreviewCapture::DwmThumbnail) {
            return Ok(None);
        }
        let source = HWND(preview.window().value() as usize as *mut std::ffi::c_void);
        // SAFETY: Category 8 (FFI boundary). Host is an owned dock HWND, source is
        // an EnumWindows HWND, and DWM validates thumbnail access across processes.
        let thumbnail = unsafe { DwmRegisterThumbnail(host, source) }?;
        let properties = DWM_THUMBNAIL_PROPERTIES {
            dwFlags: DWM_TNP_RECTDESTINATION
                | DWM_TNP_VISIBLE
                | DWM_TNP_OPACITY
                | DWM_TNP_SOURCECLIENTAREAONLY,
            rcDestination: rect(destination),
            opacity: 230,
            fVisible: BOOL::from(true),
            fSourceClientAreaOnly: BOOL::from(true),
            ..Default::default()
        };
        // SAFETY: Category 8 (FFI boundary). The thumbnail handle was registered
        // above and properties points to initialized stack storage for this call.
        unsafe { DwmUpdateThumbnailProperties(thumbnail, &properties) }?;
        Ok(Some(Self {
            thumbnail,
            window: preview.window(),
        }))
    }

    pub(super) const fn window(&self) -> WindowId {
        self.window
    }
}

impl Drop for DwmPreviewThumbnail {
    fn drop(&mut self) {
        // SAFETY: Category 8 (FFI boundary). The handle was returned by
        // DwmRegisterThumbnail and this guard unregisters it at most once.
        let _ = unsafe { DwmUnregisterThumbnail(self.thumbnail) };
    }
}

pub(super) fn probe_dwm_thumbnail(host: HWND, source: HWND) -> PreviewCapture {
    // SAFETY: Category 8 (FFI boundary). Both HWND values come from owned shell
    // surfaces or EnumWindows, and DWM validates cross-process thumbnail access.
    match unsafe { DwmRegisterThumbnail(host, source) } {
        Ok(thumbnail) => {
            // SAFETY: Category 8 (FFI boundary). The thumbnail handle was returned
            // by DwmRegisterThumbnail in this function and is released once.
            let _ = unsafe { DwmUnregisterThumbnail(thumbnail) };
            PreviewCapture::dwm_thumbnail()
        }
        Err(_) => PreviewCapture::restricted(PreviewUnavailableReason::CaptureRestricted),
    }
}

const fn rect(value: PhysicalRect) -> RECT {
    RECT {
        left: value.x,
        top: value.y,
        right: value.x + value.width,
        bottom: value.y + value.height,
    }
}
