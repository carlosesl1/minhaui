use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, MF_STRING, TPM_NONOTIFY, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, TRACK_POPUP_MENU_FLAGS, TrackPopupMenu,
};
use windows::core::w;

use crate::ContextMenuCommand;

pub(super) fn track_dock_context_menu(
    hwnd: HWND,
    mut client_point: POINT,
) -> Option<ContextMenuCommand> {
    // SAFETY: Category 8 (FFI boundary). The dock HWND is live during message
    // dispatch and `client_point` is valid writable storage for conversion.
    if !unsafe { ClientToScreen(hwnd, &mut client_point) }.as_bool() {
        return None;
    }
    // SAFETY: Category 8 (FFI boundary). Creates a process-owned popup menu that
    // is destroyed before returning from this function.
    let menu = unsafe { CreatePopupMenu() }.ok()?;
    let selected = populate_and_track(menu, hwnd, client_point);
    // SAFETY: Category 8 (FFI boundary). The popup menu handle was created above
    // and is no longer needed after TrackPopupMenu returns.
    let _ = unsafe { DestroyMenu(menu) };
    selected
}

fn populate_and_track(
    menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
    hwnd: HWND,
    point: POINT,
) -> Option<ContextMenuCommand> {
    append(menu, ContextMenuCommand::Open, w!("Open"));
    append(menu, ContextMenuCommand::Pin, w!("Pin"));
    append(menu, ContextMenuCommand::Unpin, w!("Unpin"));
    append(menu, ContextMenuCommand::Quit, w!("Quit"));
    let flags = TRACK_POPUP_MENU_FLAGS(TPM_RETURNCMD.0 | TPM_RIGHTBUTTON.0 | TPM_NONOTIFY.0);
    // SAFETY: Category 8 (FFI boundary). `menu` is a valid popup menu, `hwnd` is
    // the owner window, and TPM_RETURNCMD returns the selected numeric command.
    let selected = unsafe { TrackPopupMenu(menu, flags, point.x, point.y, None, hwnd, None) };
    ContextMenuCommand::from_native_id(selected.0 as u16)
}

fn append(
    menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
    command: ContextMenuCommand,
    label: windows::core::PCWSTR,
) {
    // SAFETY: Category 8 (FFI boundary). Menu and static UTF-16 labels are valid
    // for this synchronous append call; failures leave the menu without that row.
    let _ = unsafe { AppendMenuW(menu, MF_STRING, usize::from(command.native_id()), label) };
}
