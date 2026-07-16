use shell_core::WindowId;
use shell_renderer::PreviewSourceSize;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{
    DwmQueryThumbnailSourceSize, DwmRegisterThumbnail, DwmUnregisterThumbnail,
};
use windows::core::Result;

pub(super) fn preview_source_size(host: HWND, window: WindowId) -> Result<PreviewSourceSize> {
    let source = HWND(window.value() as usize as *mut std::ffi::c_void);
    // SAFETY: Category 8 (FFI boundary). Host is the live preview HWND, source
    // comes from EnumWindows, and DWM validates cross-process thumbnail access.
    let thumbnail = unsafe { DwmRegisterThumbnail(host, source) }?;
    // SAFETY: Category 8 (FFI boundary). The handle was returned by DWM above
    // and the query initializes its SIZE result before returning successfully.
    let source_size = unsafe { DwmQueryThumbnailSourceSize(thumbnail) };
    // SAFETY: Category 8 (FFI boundary). This function owns the registered handle
    // and releases it exactly once after the synchronous size query.
    let _ = unsafe { DwmUnregisterThumbnail(thumbnail) };
    let source_size = source_size?;
    if source_size.cx <= 0 || source_size.cy <= 0 {
        return Err(windows::core::Error::new(
            windows::core::HRESULT(0x8007_0057_u32 as i32),
            "thumbnail source has an empty DWM surface",
        ));
    }
    Ok(PreviewSourceSize::new(
        u32::try_from(source_size.cx).map_err(|_| windows::core::Error::from_thread())?,
        u32::try_from(source_size.cy).map_err(|_| windows::core::Error::from_thread())?,
    ))
}
