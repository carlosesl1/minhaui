use windows::Win32::Foundation::WPARAM;
use windows::Win32::UI::Shell::{DragFinish, DragQueryFileW, HDROP};

pub(super) fn first_drop_path(wparam: WPARAM) -> Option<String> {
    let hdrop = HDROP(wparam.0 as *mut std::ffi::c_void);
    // SAFETY: Category 8 (FFI boundary). WM_DROPFILES supplies a valid HDROP until
    // DragFinish is called below.
    let length = unsafe { DragQueryFileW(hdrop, 0, None) };
    if length == 0 {
        // SAFETY: Category 8 (FFI boundary). Releases the HDROP supplied by Windows.
        unsafe { DragFinish(hdrop) };
        return None;
    }
    let mut buffer = vec![0; length as usize + 1];
    // SAFETY: Category 8 (FFI boundary). The buffer is writable and sized from the
    // preceding query including room for a terminator.
    let written = unsafe { DragQueryFileW(hdrop, 0, Some(&mut buffer)) };
    // SAFETY: Category 8 (FFI boundary). Releases the HDROP supplied by Windows.
    unsafe { DragFinish(hdrop) };
    if written == 0 {
        None
    } else {
        Some(String::from_utf16_lossy(&buffer[..written as usize]))
    }
}
