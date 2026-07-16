use std::ops::{Deref, DerefMut};

use shell_renderer::PhysicalRect;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::ABN_POSCHANGED;
use windows::Win32::UI::WindowsAndMessaging::WM_APP;
use windows::Win32::UI::WindowsAndMessaging::{SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos};
use windows::core::Result;

use crate::win32_appbar_ffi::{
    notify_activation as notify_shell_activation, notify_window_position as notify_shell_position,
    register_with_shell, remove_from_shell, reserve_top_edge,
};
use crate::win32_window::{OwnedWindow, WindowClass};
use crate::win32_windowing::{window_monitor_bounds, window_work_area};

pub(super) const APPBAR_CALLBACK_MESSAGE: u32 = WM_APP + 0x4D;

pub(super) trait AppBarAdapter {
    fn register(&self, hwnd: HWND) -> Result<()>;
    fn reserve_top_edge(&self, hwnd: HWND, requested: PhysicalRect) -> PhysicalRect;
    fn remove(&self, hwnd: HWND);
}

#[derive(Clone, Copy, Default)]
pub(super) struct WindowsAppBarAdapter;

impl AppBarAdapter for WindowsAppBarAdapter {
    fn register(&self, hwnd: HWND) -> Result<()> {
        register_with_shell(hwnd)
    }

    fn reserve_top_edge(&self, hwnd: HWND, requested: PhysicalRect) -> PhysicalRect {
        reserve_top_edge(hwnd, requested)
    }

    fn remove(&self, hwnd: HWND) {
        remove_from_shell(hwnd);
    }
}

#[derive(Clone, Copy)]
pub(super) struct TopbarCreateOptions {
    backdrop_enabled: bool,
    text_scale: f32,
}

impl TopbarCreateOptions {
    pub(super) const fn new(backdrop_enabled: bool, text_scale: f32) -> Self {
        Self {
            backdrop_enabled,
            text_scale,
        }
    }
}

pub(super) struct TopbarWindow {
    reservation: AppBarReservation,
    window: OwnedWindow,
}

impl TopbarWindow {
    pub(super) fn create(
        class: &WindowClass,
        monitor_bounds: PhysicalRect,
        options: TopbarCreateOptions,
    ) -> Result<Self> {
        let mut window = OwnedWindow::create(
            class,
            shell_renderer::native::ShowcaseRole::Topbar,
            monitor_bounds,
            options.backdrop_enabled,
        )?;
        window.set_topbar_text_scale(monitor_bounds, options.text_scale)?;
        let reservation =
            AppBarReservation::register(window.hwnd, monitor_bounds, window.rect.height)?;
        apply_reserved_rect(&mut window, reservation.rect())?;
        Ok(Self {
            reservation,
            window,
        })
    }

    pub(super) fn reconcile(&mut self, monitor_bounds: PhysicalRect) -> Result<()> {
        self.window.reposition(monitor_bounds)?;
        let approved = self
            .reservation
            .update(monitor_bounds, self.window.rect.height)?;
        apply_reserved_rect(&mut self.window, approved)
    }

    pub(super) fn work_area(&self) -> Result<PhysicalRect> {
        window_work_area(self.window.hwnd)
    }

    pub(super) fn reconcile_notification(&mut self) -> Result<Option<PhysicalRect>> {
        let work_area = self.work_area()?;
        if !reservation_needs_update(self.reservation.rect, self.window.rect, work_area) {
            return Ok(None);
        }
        self.reconcile(window_monitor_bounds(self.window.hwnd)?)?;
        self.work_area().map(Some)
    }

    pub(super) fn reregister(&mut self) -> Result<()> {
        self.reservation.reregister()?;
        self.reconcile(window_monitor_bounds(self.window.hwnd)?)
    }
}

impl Deref for TopbarWindow {
    type Target = OwnedWindow;

    fn deref(&self) -> &Self::Target {
        &self.window
    }
}

impl DerefMut for TopbarWindow {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.window
    }
}

pub(super) struct AppBarReservation<A: AppBarAdapter = WindowsAppBarAdapter> {
    adapter: A,
    hwnd: HWND,
    rect: PhysicalRect,
}

impl AppBarReservation<WindowsAppBarAdapter> {
    pub(super) fn register(hwnd: HWND, monitor_bounds: PhysicalRect, height: i32) -> Result<Self> {
        Self::register_with(WindowsAppBarAdapter, hwnd, monitor_bounds, height)
    }
}

impl<A: AppBarAdapter> AppBarReservation<A> {
    fn register_with(
        adapter: A,
        hwnd: HWND,
        monitor_bounds: PhysicalRect,
        height: i32,
    ) -> Result<Self> {
        adapter.register(hwnd)?;
        let mut reservation = Self {
            adapter,
            hwnd,
            rect: topbar_reservation_rect(monitor_bounds, height),
        };
        reservation.update(monitor_bounds, height)?;
        Ok(reservation)
    }

    pub(super) fn update(
        &mut self,
        monitor_bounds: PhysicalRect,
        height: i32,
    ) -> Result<PhysicalRect> {
        self.rect = self
            .adapter
            .reserve_top_edge(self.hwnd, topbar_reservation_rect(monitor_bounds, height));
        Ok(self.rect)
    }

    pub(super) const fn rect(&self) -> PhysicalRect {
        self.rect
    }

    pub(super) fn reregister(&mut self) -> Result<()> {
        self.adapter.remove(self.hwnd);
        self.adapter.register(self.hwnd)
    }
}

impl<A: AppBarAdapter> Drop for AppBarReservation<A> {
    fn drop(&mut self) {
        self.adapter.remove(self.hwnd);
    }
}

pub(super) const fn topbar_reservation_rect(
    monitor_bounds: PhysicalRect,
    height: i32,
) -> PhysicalRect {
    let monitor_height = if monitor_bounds.height > 0 {
        monitor_bounds.height
    } else {
        1
    };
    let reserved_height = if height < 1 {
        1
    } else if height > monitor_height {
        monitor_height
    } else {
        height
    };
    let monitor_width = if monitor_bounds.width > 0 {
        monitor_bounds.width
    } else {
        1
    };
    PhysicalRect::new(
        monitor_bounds.x,
        monitor_bounds.y,
        monitor_width,
        reserved_height,
    )
}

pub(super) const fn reservation_needs_update(
    reserved: PhysicalRect,
    window: PhysicalRect,
    work_area: PhysicalRect,
) -> bool {
    window.x != reserved.x
        || window.y != reserved.y
        || window.width != reserved.width
        || window.height != reserved.height
        || work_area.x != reserved.x
        || work_area.width != reserved.width
        || work_area.y != reserved.y.saturating_add(reserved.height)
}

pub(super) const fn is_position_notification(message: u32, notification: usize) -> bool {
    message == APPBAR_CALLBACK_MESSAGE && notification == ABN_POSCHANGED as usize
}

pub(super) fn notify_window_position(hwnd: HWND) {
    notify_shell_position(hwnd);
}

pub(super) fn notify_activation(hwnd: HWND, active: bool) {
    notify_shell_activation(hwnd, active);
}

fn apply_reserved_rect(window: &mut OwnedWindow, rect: PhysicalRect) -> Result<()> {
    let rect = PhysicalRect::new(rect.x, rect.y, rect.width.max(1), rect.height.max(1));
    // SAFETY: Category 8 (FFI boundary). The AppBar-approved rectangle has
    // positive dimensions and the wrapper owns the live topbar HWND.
    unsafe {
        SetWindowPos(
            window.hwnd,
            None,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    }?;
    window.rect = rect;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use shell_renderer::PhysicalRect;
    use windows::Win32::Foundation::HWND;
    use windows::core::Result;

    use super::{AppBarAdapter, AppBarReservation};

    #[derive(Clone)]
    struct RecordingAppBar {
        events: Rc<RefCell<Vec<&'static str>>>,
    }

    impl AppBarAdapter for RecordingAppBar {
        fn register(&self, _hwnd: HWND) -> Result<()> {
            self.events.borrow_mut().push("register");
            Ok(())
        }

        fn reserve_top_edge(&self, _hwnd: HWND, requested: PhysicalRect) -> PhysicalRect {
            self.events.borrow_mut().push("reserve");
            PhysicalRect::new(
                requested.x + 1,
                requested.y,
                requested.width,
                requested.height,
            )
        }

        fn remove(&self, _hwnd: HWND) {
            self.events.borrow_mut().push("remove");
        }
    }

    #[test]
    fn reservation_lifecycle_uses_the_injected_appbar_adapter() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let adapter = RecordingAppBar {
            events: Rc::clone(&events),
        };
        let monitor = PhysicalRect::new(-1280, 0, 1280, 720);
        let mut reservation =
            AppBarReservation::register_with(adapter, HWND::default(), monitor, 40)
                .expect("recording adapter should register");

        assert_eq!(reservation.rect(), PhysicalRect::new(-1279, 0, 1280, 40));
        reservation
            .update(monitor, 48)
            .expect("recording adapter should update");
        reservation
            .reregister()
            .expect("recording adapter should reregister");
        drop(reservation);

        assert_eq!(
            events.borrow().as_slice(),
            [
                "register", "reserve", "reserve", "remove", "register", "remove"
            ]
        );
    }
}
