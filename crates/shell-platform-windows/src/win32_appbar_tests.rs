#![deny(unsafe_code)]

use shell_renderer::PhysicalRect;

use crate::win32_appbar::{reservation_needs_update, topbar_reservation_rect};

#[test]
fn reservation_spans_the_top_of_a_signed_monitor() {
    // Given: a secondary monitor positioned left of the primary display.
    let monitor = PhysicalRect::new(-1920, 0, 1920, 1080);

    // When: a 32-pixel topbar requests exclusive desktop space.
    let reservation = topbar_reservation_rect(monitor, 32);

    // Then: the request spans that monitor's top edge only.
    assert_eq!(reservation, PhysicalRect::new(-1920, 0, 1920, 32));
}

#[test]
fn reservation_height_stays_inside_monitor_bounds() {
    // Given: invalid and oversized height requests.
    let monitor = PhysicalRect::new(0, -1080, 1920, 1080);

    // When: both requests are converted into topbar reservations.
    let minimum = topbar_reservation_rect(monitor, 0);
    let maximum = topbar_reservation_rect(monitor, 2048);

    // Then: the reserved strip remains positive and inside the monitor.
    assert_eq!(minimum.height, 1);
    assert_eq!(maximum.height, monitor.height);
}

#[test]
fn notification_updates_when_shell_or_dpi_geometry_changes() {
    // Given: a 32-pixel reservation whose topbar currently matches it.
    let reserved = PhysicalRect::new(0, 0, 1920, 32);
    let matching_window = reserved;
    let matching_work = PhysicalRect::new(0, 32, 1920, 1048);

    // When: Shell work-area placement, width, or the physical topbar height changes.
    let shell_changed = reservation_needs_update(
        reserved,
        matching_window,
        PhysicalRect::new(0, 40, 1920, 1040),
    );
    let dpi_changed =
        reservation_needs_update(reserved, PhysicalRect::new(0, 0, 1920, 48), matching_work);
    let lateral_appbar_changed = reservation_needs_update(
        reserved,
        matching_window,
        PhysicalRect::new(48, 32, 1872, 1048),
    );

    // Then: either change requires AppBar renegotiation, while stable geometry does not.
    assert!(shell_changed);
    assert!(dpi_changed);
    assert!(lateral_appbar_changed);
    assert!(!reservation_needs_update(
        reserved,
        matching_window,
        matching_work
    ));
}
