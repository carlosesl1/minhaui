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
    _thumbnail: RegisteredThumbnail<DwmThumbnailApi>,
    window: WindowId,
}

trait ThumbnailApi {
    type Error;

    fn register(&mut self) -> std::result::Result<isize, Self::Error>;
    fn update(&mut self, thumbnail: isize) -> std::result::Result<(), Self::Error>;
    fn unregister(&mut self, thumbnail: isize);
}

struct RegisteredThumbnail<Api: ThumbnailApi> {
    thumbnail: isize,
    api: Api,
}

impl<Api: ThumbnailApi> RegisteredThumbnail<Api> {
    const fn new(thumbnail: isize, api: Api) -> Self {
        Self { thumbnail, api }
    }
}

impl<Api: ThumbnailApi> Drop for RegisteredThumbnail<Api> {
    fn drop(&mut self) {
        self.api.unregister(self.thumbnail);
    }
}

#[derive(Clone, Copy)]
struct DwmThumbnailApi {
    host: HWND,
    source: HWND,
    properties: DWM_THUMBNAIL_PROPERTIES,
}

impl ThumbnailApi for DwmThumbnailApi {
    type Error = windows::core::Error;

    fn register(&mut self) -> std::result::Result<isize, Self::Error> {
        // SAFETY: Category 8 (FFI boundary). Host is an owned dock HWND, source is
        // an EnumWindows HWND, and DWM validates thumbnail access across processes.
        unsafe { DwmRegisterThumbnail(self.host, self.source) }
    }

    fn update(&mut self, thumbnail: isize) -> std::result::Result<(), Self::Error> {
        // SAFETY: Category 8 (FFI boundary). The thumbnail handle was registered
        // above and properties points to initialized stack storage for this call.
        unsafe { DwmUpdateThumbnailProperties(thumbnail, &self.properties) }
    }

    fn unregister(&mut self, thumbnail: isize) {
        // SAFETY: Category 8 (FFI boundary). The handle was returned by
        // DwmRegisterThumbnail and this guard unregisters it at most once.
        let _ = unsafe { DwmUnregisterThumbnail(thumbnail) };
    }
}

fn register_updated_thumbnail<Api: ThumbnailApi>(
    mut api: Api,
) -> std::result::Result<RegisteredThumbnail<Api>, Api::Error> {
    let thumbnail = api.register()?;
    let mut guard = RegisteredThumbnail::new(thumbnail, api);
    guard.api.update(thumbnail)?;
    Ok(guard)
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
        let thumbnail = register_updated_thumbnail(DwmThumbnailApi {
            host,
            source,
            properties,
        })?;
        Ok(Some(Self {
            _thumbnail: thumbnail,
            window: preview.window(),
        }))
    }

    pub(super) const fn window(&self) -> WindowId {
        self.window
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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::{ThumbnailApi, register_updated_thumbnail};

    #[derive(Clone)]
    struct FakeThumbnailApi {
        calls: Rc<RefCell<Vec<&'static str>>>,
        update_fails: bool,
    }

    impl ThumbnailApi for FakeThumbnailApi {
        type Error = &'static str;

        fn register(&mut self) -> Result<isize, Self::Error> {
            self.calls.borrow_mut().push("register");
            Ok(77)
        }

        fn update(&mut self, _thumbnail: isize) -> Result<(), Self::Error> {
            self.calls.borrow_mut().push("update");
            if self.update_fails {
                Err("update failed")
            } else {
                Ok(())
            }
        }

        fn unregister(&mut self, _thumbnail: isize) {
            self.calls.borrow_mut().push("unregister");
        }
    }

    #[test]
    fn failed_dwm_update_unregisters_registered_thumbnail() {
        // Given: DWM registration succeeds and update fails.
        let calls = Rc::new(RefCell::new(Vec::new()));
        let api = FakeThumbnailApi {
            calls: Rc::clone(&calls),
            update_fails: true,
        };

        // When: the register-update sequence runs through the RAII helper.
        let result = register_updated_thumbnail(api);

        // Then: the registered handle is unregistered on the update error path.
        assert_eq!(result.err(), Some("update failed"));
        assert_eq!(&*calls.borrow(), &["register", "update", "unregister"]);
    }
}
