use shell_renderer::{DipPoint, DipRect, Dpi, physical_from_dip, rounded_content_hit};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, HTCLIENT, HTTRANSPARENT};

pub(super) fn hit_test(hwnd: HWND, lparam: LPARAM) -> LRESULT {
    let mut rect = RECT::default();
    // SAFETY: Category 8 (FFI boundary). The callback supplies a live HWND and `rect`
    // is valid writable storage for the call.
    if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
        return LRESULT(HTTRANSPARENT as isize);
    }
    let screen_x = (lparam.0 as u16) as i16 as i32;
    let screen_y = ((lparam.0 >> 16) as u16) as i16 as i32;
    // SAFETY: Category 8 (FFI boundary). The window remains live during its callback,
    // so querying its effective DPI is valid.
    let dpi = Dpi::from_raw(unsafe { GetDpiForWindow(hwnd) }.max(96));
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let scale = dpi.scale();
    let point = DipPoint::new(
        (screen_x - rect.left) as f32 / scale,
        (screen_y - rect.top) as f32 / scale,
    );
    let bounds = DipRect::new(0.0, 0.0, width as f32 / scale, height as f32 / scale);
    if rounded_content_hit(bounds, radius(height, dpi), point) {
        LRESULT(HTCLIENT as isize)
    } else {
        LRESULT(HTTRANSPARENT as isize)
    }
}

fn radius(height: i32, dpi: Dpi) -> f32 {
    if height <= physical_from_dip(40.0, dpi) {
        12.0
    } else {
        22.0
    }
}
