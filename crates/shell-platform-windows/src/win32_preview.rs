use shell_renderer::{PhysicalRect, WindowPreviewCapture, WindowPreviewVisual};
use windows::Win32::Foundation::{HINSTANCE, HWND, RECT};
use windows::Win32::Graphics::Dwm::{
    DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION,
    DWM_TNP_SOURCECLIENTAREAONLY, DWM_TNP_VISIBLE, DWM_WINDOW_CORNER_PREFERENCE, DWMWA_CLOAK,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmFlush, DwmQueryThumbnailSourceSize,
    DwmRegisterThumbnail, DwmSetWindowAttribute, DwmUnregisterThumbnail,
    DwmUpdateThumbnailProperties,
};
use windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, DeleteObject, HGDIOBJ, SetWindowRgn};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
    SetWindowPos, ShowWindow, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{BOOL, Result};

use crate::win32_window::THUMBNAIL_HOST_CLASS_NAME;
pub(super) const PREVIEW_THUMBNAIL_OPACITY: u8 = 230;

pub(super) fn flush_preview_composition() -> Result<()> {
    // SAFETY: Category 8 (FFI boundary). `DwmFlush` accepts no pointers or
    // caller-owned buffers; Windows owns the compositor synchronization state.
    unsafe { DwmFlush() }
}

fn set_window_cloaked(hwnd: HWND, cloaked: bool) -> Result<()> {
    let cloaked = BOOL::from(cloaked);
    // SAFETY: Category 8 (FFI boundary). The live HWND and correctly sized BOOL
    // are read synchronously by the documented DWM cloak attribute.
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_CLOAK,
            std::ptr::from_ref(&cloaked).cast(),
            std::mem::size_of_val(&cloaked) as u32,
        )
    }
}

pub(super) fn stage_preview_composition(
    owner: HWND,
    thumbnails: &[DwmPreviewThumbnail],
    cloaked: bool,
) -> Result<()> {
    if thumbnails.is_empty() {
        return Ok(());
    }
    if cloaked {
        set_window_cloaked(owner, true)?;
        for thumbnail in thumbnails {
            set_window_cloaked(thumbnail.host.hwnd, true)?;
        }
    }
    for thumbnail in thumbnails {
        thumbnail.host.show();
    }
    Ok(())
}

pub(super) fn reveal_preview_composition(
    owner: HWND,
    thumbnails: &[DwmPreviewThumbnail],
    cloaked: bool,
) -> Result<()> {
    if thumbnails.is_empty() {
        return Ok(());
    }
    flush_preview_composition()?;
    if cloaked {
        set_window_cloaked(owner, false)?;
        for thumbnail in thumbnails {
            set_window_cloaked(thumbnail.host.hwnd, false)?;
        }
    }
    flush_preview_composition()
}

pub(super) struct DwmPreviewThumbnail {
    _thumbnail: RegisteredThumbnail<DwmThumbnailApi>,
    host: RoundedThumbnailHost,
    slot: PhysicalRect,
    corner_radius: i32,
}

struct RoundedThumbnailHost {
    hwnd: HWND,
    local_rect: PhysicalRect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NativeThumbnailClipPlan {
    region_diameter: i32,
    dwm_corner_preference: i32,
    dwm_before_region_fallback: bool,
}

fn native_thumbnail_clip_plan(corner_radius: i32) -> NativeThumbnailClipPlan {
    NativeThumbnailClipPlan {
        region_diameter: corner_radius.max(1).saturating_mul(2),
        dwm_corner_preference: DWMWCP_ROUND.0,
        dwm_before_region_fallback: true,
    }
}

impl RoundedThumbnailHost {
    fn create(
        owner: HWND,
        owner_rect: PhysicalRect,
        local_rect: PhysicalRect,
        corner_radius: i32,
    ) -> Result<Self> {
        // SAFETY: Category 8 (FFI boundary). The current module stays loaded for
        // the lifetime of the already registered thumbnail-host window class.
        let module = unsafe { GetModuleHandleW(None) }?;
        let screen = thumbnail_host_screen_rect(owner_rect, local_rect);
        // SAFETY: Category 8 (FFI boundary). The class and owner are live; the new
        // top-level owned popup is isolated from the shell's routed window set.
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                THUMBNAIL_HOST_CLASS_NAME,
                None,
                WS_POPUP,
                screen.x,
                screen.y,
                screen.width,
                screen.height,
                Some(owner),
                None,
                Some(HINSTANCE(module.0)),
                None,
            )
        }?;
        if let Err(error) = apply_native_thumbnail_clip(hwnd, local_rect, corner_radius) {
            // SAFETY: Category 8 (FFI boundary). This guard owns the host HWND.
            let _ = unsafe { DestroyWindow(hwnd) };
            return Err(error);
        }
        Ok(Self { hwnd, local_rect })
    }

    fn show(&self) {
        // SAFETY: Category 8 (FFI boundary). The no-activate host is fully configured.
        let _ = unsafe { ShowWindow(self.hwnd, SW_SHOWNOACTIVATE) };
    }

    fn reposition(&mut self, owner_rect: PhysicalRect) -> Result<()> {
        let screen = thumbnail_host_screen_rect(owner_rect, self.local_rect);
        // SAFETY: Category 8 (FFI boundary). The host remains live and only its
        // screen-space origin follows the animated owner popup.
        unsafe {
            SetWindowPos(
                self.hwnd,
                None,
                screen.x,
                screen.y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        }?;
        Ok(())
    }

    fn place(
        &mut self,
        owner_rect: PhysicalRect,
        local_rect: PhysicalRect,
        corner_radius: i32,
    ) -> Result<()> {
        let screen = thumbnail_host_screen_rect(owner_rect, local_rect);
        // SAFETY: Category 8 (FFI boundary). The owned host HWND remains live and
        // the fitted rectangle has positive dimensions bounded by the preview slot.
        unsafe {
            SetWindowPos(
                self.hwnd,
                None,
                screen.x,
                screen.y,
                screen.width,
                screen.height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        }?;
        self.local_rect = local_rect;
        apply_native_thumbnail_clip(self.hwnd, local_rect, corner_radius)
    }
}

impl Drop for RoundedThumbnailHost {
    fn drop(&mut self) {
        // SAFETY: Category 8 (FFI boundary). This guard is the sole host owner.
        let _ = unsafe { DestroyWindow(self.hwnd) };
    }
}

fn apply_native_thumbnail_clip(
    hwnd: HWND,
    local_rect: PhysicalRect,
    corner_radius: i32,
) -> Result<()> {
    let clip = native_thumbnail_clip_plan(corner_radius);
    let preference = DWM_WINDOW_CORNER_PREFERENCE(clip.dwm_corner_preference);
    // SAFETY: Category 8 (FFI boundary). The live top-level host and correctly sized
    // enum are read synchronously. A successful Windows 11 native clip must not be
    // combined with a window region, which would disable DWM corner rounding.
    let dwm_clip_applied = clip.dwm_before_region_fallback
        && unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                std::ptr::from_ref(&preference).cast(),
                std::mem::size_of_val(&preference) as u32,
            )
        }
        .is_ok();
    if dwm_clip_applied {
        return Ok(());
    }

    // SAFETY: Category 8 (FFI boundary). Positive host dimensions and the bounded
    // ellipse diameter define the Windows 10 fallback region.
    let region = unsafe {
        CreateRoundRectRgn(
            0,
            0,
            local_rect.width.saturating_add(1),
            local_rect.height.saturating_add(1),
            clip.region_diameter,
            clip.region_diameter,
        )
    };
    if region.0.is_null() {
        return Err(windows::core::Error::from_thread());
    }
    // SAFETY: Category 8 (FFI boundary). On success Windows owns the region; on
    // failure this function retains ownership and deletes it below.
    if unsafe { SetWindowRgn(hwnd, Some(region), true) } == 0 {
        // SAFETY: Category 8 (FFI boundary). SetWindowRgn failed, so ownership did
        // not transfer and the region must be released by this function.
        let _ = unsafe { DeleteObject(HGDIOBJ(region.0)) };
        return Err(windows::core::Error::from_thread());
    }

    Ok(())
}

trait ThumbnailApi {
    type Error;

    fn register(&mut self) -> std::result::Result<isize, Self::Error>;
    fn source_size(&mut self, thumbnail: isize) -> std::result::Result<(i32, i32), Self::Error>;
    fn update(&mut self, thumbnail: isize) -> std::result::Result<(), Self::Error>;
    fn unregister(&mut self, thumbnail: isize);
}

struct RegisteredThumbnail<Api: ThumbnailApi> {
    thumbnail: isize,
    api: Api,
}

type RegisteredThumbnailWithSize<Api> = (RegisteredThumbnail<Api>, (i32, i32));

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
        // SAFETY: Category 8 (FFI boundary). Host is an owned thumbnail HWND, source
        // is an EnumWindows HWND, and DWM validates access across processes.
        unsafe { DwmRegisterThumbnail(self.host, self.source) }
    }

    fn source_size(&mut self, thumbnail: isize) -> std::result::Result<(i32, i32), Self::Error> {
        // SAFETY: Category 8 (FFI boundary). The thumbnail handle was returned by
        // DwmRegisterThumbnail and the query returns an initialized SIZE value.
        let size = unsafe { DwmQueryThumbnailSourceSize(thumbnail) }?;
        if size.cx <= 0 || size.cy <= 0 {
            return Err(windows::core::Error::new(
                windows::core::HRESULT(0x8007_0057_u32 as i32),
                "thumbnail source has an empty DWM surface",
            ));
        }
        Ok((size.cx, size.cy))
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

#[cfg(test)]
fn register_updated_thumbnail<Api: ThumbnailApi>(
    mut api: Api,
) -> std::result::Result<RegisteredThumbnail<Api>, Api::Error> {
    let thumbnail = api.register()?;
    let mut guard = RegisteredThumbnail::new(thumbnail, api);
    guard.api.update(thumbnail)?;
    Ok(guard)
}

fn register_sized_thumbnail<Api: ThumbnailApi>(
    mut api: Api,
) -> std::result::Result<RegisteredThumbnailWithSize<Api>, Api::Error> {
    let thumbnail = api.register()?;
    let mut guard = RegisteredThumbnail::new(thumbnail, api);
    let source_size = guard.api.source_size(thumbnail)?;
    Ok((guard, source_size))
}

impl DwmPreviewThumbnail {
    pub(super) fn show(
        owner: HWND,
        owner_rect: PhysicalRect,
        preview: WindowPreviewVisual,
        destination: PhysicalRect,
        corner_radius: i32,
        opacity: u8,
    ) -> Result<Option<Self>> {
        if !matches!(preview.capture(), WindowPreviewCapture::DwmThumbnail) {
            return Ok(None);
        }
        let source = HWND(preview.window().value() as usize as *mut std::ffi::c_void);
        let mut host = RoundedThumbnailHost::create(owner, owner_rect, destination, corner_radius)?;
        let api = DwmThumbnailApi {
            host: host.hwnd,
            source,
            properties: DWM_THUMBNAIL_PROPERTIES::default(),
        };
        let (mut thumbnail, (source_width, source_height)) = register_sized_thumbnail(api)?;
        let fitted = fit_thumbnail_destination(destination, source_width, source_height);
        host.place(owner_rect, fitted, corner_radius)?;
        let local_destination = PhysicalRect::new(0, 0, fitted.width, fitted.height);
        thumbnail.api.properties = DWM_THUMBNAIL_PROPERTIES {
            dwFlags: DWM_TNP_RECTDESTINATION
                | DWM_TNP_VISIBLE
                | DWM_TNP_OPACITY
                | DWM_TNP_SOURCECLIENTAREAONLY,
            rcDestination: rect(local_destination),
            opacity,
            fVisible: BOOL::from(true),
            fSourceClientAreaOnly: BOOL::from(true),
            ..Default::default()
        };
        thumbnail.api.update(thumbnail.thumbnail)?;
        Ok(Some(Self {
            _thumbnail: thumbnail,
            host,
            slot: destination,
            corner_radius,
        }))
    }

    pub(super) fn set_opacity(&mut self, opacity: u8) -> Result<()> {
        self._thumbnail.api.properties.dwFlags = DWM_TNP_OPACITY;
        self._thumbnail.api.properties.opacity = opacity;
        self._thumbnail.api.update(self._thumbnail.thumbnail)
    }

    pub(super) fn reposition(&mut self, owner_rect: PhysicalRect) -> Result<()> {
        self.host.reposition(owner_rect)
    }

    pub(super) fn refit(&mut self, owner_rect: PhysicalRect) -> Result<()> {
        let (source_width, source_height) =
            self._thumbnail.api.source_size(self._thumbnail.thumbnail)?;
        let fitted = fit_thumbnail_destination(self.slot, source_width, source_height);
        if self.host.local_rect == fitted {
            return Ok(());
        }
        self.host.place(owner_rect, fitted, self.corner_radius)?;
        self._thumbnail.api.properties.dwFlags = DWM_TNP_RECTDESTINATION;
        self._thumbnail.api.properties.rcDestination =
            rect(PhysicalRect::new(0, 0, fitted.width, fitted.height));
        self._thumbnail.api.update(self._thumbnail.thumbnail)
    }
}

#[cfg(test)]
const fn client_area_size(client: RECT) -> Option<(i32, i32)> {
    let width = client.right - client.left;
    let height = client.bottom - client.top;
    if width > 0 && height > 0 {
        Some((width, height))
    } else {
        None
    }
}

fn fit_thumbnail_destination(
    slot: PhysicalRect,
    source_width: i32,
    source_height: i32,
) -> PhysicalRect {
    if source_width <= 0 || source_height <= 0 || slot.width <= 0 || slot.height <= 0 {
        return slot;
    }
    let scale =
        (slot.width as f32 / source_width as f32).min(slot.height as f32 / source_height as f32);
    let width = ((source_width as f32 * scale).round() as i32).clamp(1, slot.width);
    let height = ((source_height as f32 * scale).round() as i32).clamp(1, slot.height);
    PhysicalRect::new(
        slot.x + (slot.width - width) / 2,
        slot.y + (slot.height - height) / 2,
        width,
        height,
    )
}

const fn rect(value: PhysicalRect) -> RECT {
    RECT {
        left: value.x,
        top: value.y,
        right: value.x + value.width,
        bottom: value.y + value.height,
    }
}

const fn thumbnail_host_screen_rect(
    owner_rect: PhysicalRect,
    local_rect: PhysicalRect,
) -> PhysicalRect {
    PhysicalRect::new(
        owner_rect.x + local_rect.x,
        owner_rect.y + local_rect.y,
        local_rect.width,
        local_rect.height,
    )
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use shell_renderer::{PhysicalRect, WINDOW_PREVIEW_THUMBNAIL_RADIUS};
    use windows::Win32::Foundation::RECT;

    use super::{
        ThumbnailApi, client_area_size, fit_thumbnail_destination, native_thumbnail_clip_plan,
        register_sized_thumbnail, register_updated_thumbnail, thumbnail_host_screen_rect,
    };

    #[derive(Clone)]
    struct FakeThumbnailApi {
        calls: Rc<RefCell<Vec<&'static str>>>,
        update_fails: bool,
        source_size: (i32, i32),
    }

    impl ThumbnailApi for FakeThumbnailApi {
        type Error = &'static str;

        fn register(&mut self) -> Result<isize, Self::Error> {
            self.calls.borrow_mut().push("register");
            Ok(77)
        }

        fn source_size(&mut self, _thumbnail: isize) -> Result<(i32, i32), Self::Error> {
            self.calls.borrow_mut().push("source_size");
            Ok(self.source_size)
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
            source_size: (320, 600),
        };

        // When: the register-update sequence runs through the RAII helper.
        let result = register_updated_thumbnail(api);

        // Then: the registered handle is unregistered on the update error path.
        assert_eq!(result.err(), Some("update failed"));
        assert_eq!(&*calls.borrow(), &["register", "update", "unregister"]);
    }

    #[test]
    fn registered_thumbnail_queries_native_source_size_before_layout() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let api = FakeThumbnailApi {
            calls: Rc::clone(&calls),
            update_fails: false,
            source_size: (320, 600),
        };

        let (thumbnail, source_size) = register_sized_thumbnail(api).expect("query succeeds");

        assert_eq!(source_size, (320, 600));
        assert_eq!(&*calls.borrow(), &["register", "source_size"]);
        drop(thumbnail);
        assert_eq!(&*calls.borrow(), &["register", "source_size", "unregister"]);
    }

    #[test]
    fn rounded_host_preserves_thumbnail_size_and_tracks_preview_origin() {
        let owner = PhysicalRect::new(640, 300, 344, 212);
        let local = PhysicalRect::new(12, 48, 248, 140);

        assert_eq!(
            thumbnail_host_screen_rect(owner, local),
            PhysicalRect::new(652, 348, 248, 140)
        );
    }

    #[test]
    fn native_thumbnail_clip_replaces_the_colored_overlay_with_window_shape() {
        let clip = native_thumbnail_clip_plan(WINDOW_PREVIEW_THUMBNAIL_RADIUS as i32);

        assert_eq!(clip.region_diameter, 16);
        assert_eq!(clip.dwm_corner_preference, 2);
        assert!(clip.dwm_before_region_fallback);
    }

    #[test]
    fn thumbnail_destination_preserves_portrait_and_landscape_aspect_ratios() {
        let slot = PhysicalRect::new(10, 20, 248, 140);

        assert_eq!(
            fit_thumbnail_destination(slot, 320, 600),
            PhysicalRect::new(96, 20, 75, 140)
        );
        assert_eq!(
            fit_thumbnail_destination(slot, 1_000, 500),
            PhysicalRect::new(10, 28, 248, 124)
        );
    }

    #[test]
    fn client_area_size_matches_the_dwm_client_only_source() {
        assert_eq!(
            client_area_size(RECT {
                left: 0,
                top: 0,
                right: 320,
                bottom: 600,
            }),
            Some((320, 600))
        );
        assert_eq!(
            client_area_size(RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 600,
            }),
            None
        );
    }
}
