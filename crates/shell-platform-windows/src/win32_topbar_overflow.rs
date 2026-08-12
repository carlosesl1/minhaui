use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, MF_STRING, PostMessageW, SetForegroundWindow,
    TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, TRACK_POPUP_MENU_FLAGS,
    TrackPopupMenu, WM_NULL,
};
use windows::core::PCWSTR;

use crate::TopbarOverflowItem;

pub(super) fn track_topbar_overflow(
    hwnd: HWND,
    mut client_point: POINT,
    items: &[TopbarOverflowItem],
) -> Option<shell_core::TopbarModuleKind> {
    if items.is_empty() {
        return None;
    }
    // SAFETY: Category 8 (FFI boundary). The topbar HWND is live during action
    // dispatch and `client_point` is writable storage for the conversion.
    if !unsafe { ClientToScreen(hwnd, &mut client_point) }.as_bool() {
        return None;
    }
    // SAFETY: Category 8 (FFI boundary). This creates a process-owned menu that
    // is synchronously tracked and destroyed before the function returns.
    let menu = unsafe { CreatePopupMenu() }.ok()?;
    let mut appended_kinds = Vec::with_capacity(items.len());
    for item in items {
        let label = item
            .label()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let command_id = appended_kinds.len().saturating_add(1);
        // SAFETY: Category 8 (FFI boundary). `menu` is valid and the terminated
        // UTF-16 label remains alive for this synchronous copy operation.
        let appended = unsafe { AppendMenuW(menu, MF_STRING, command_id, PCWSTR(label.as_ptr())) };
        if appended.is_ok() {
            appended_kinds.push(item.kind());
        }
    }
    if appended_kinds.is_empty() {
        // SAFETY: Category 8 (FFI boundary). No menu is being tracked yet and
        // this branch releases the process-owned handle before returning.
        let _ = unsafe { DestroyMenu(menu) };
        return None;
    }
    let flags = TRACK_POPUP_MENU_FLAGS(
        TPM_RETURNCMD.0 | TPM_RIGHTBUTTON.0 | TPM_NONOTIFY.0 | TPM_LEFTALIGN.0,
    );
    // SAFETY: Category 8 (FFI boundary). This follows TrackPopupMenu's ownership
    // contract: the top-level owner is foreground before menu tracking so mouse
    // dismissal and native keyboard navigation work reliably.
    let _ = unsafe { SetForegroundWindow(hwnd) };
    // SAFETY: Category 8 (FFI boundary). The valid menu is tracked modally at a
    // point derived from the focused/clicked overflow control.
    let selected = unsafe {
        TrackPopupMenu(
            menu,
            flags,
            client_point.x,
            client_point.y,
            None,
            hwnd,
            None,
        )
    };
    // SAFETY: Category 8 (FFI boundary). No submenu survives TrackPopupMenu;
    // this guard releases the process-owned menu before returning.
    let _ = unsafe { DestroyMenu(menu) };
    // SAFETY: Category 8 (FFI boundary). Microsoft documents posting a benign
    // message after TrackPopupMenu so repeated invocations do not immediately
    // dismiss; the HWND remains live during this synchronous dispatch.
    let _ = unsafe { PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0)) };
    let index = usize::try_from(selected.0).ok()?.checked_sub(1)?;
    appended_kinds.get(index).copied()
}
