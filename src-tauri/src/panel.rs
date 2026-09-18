use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalRect, PhysicalSize, Rect, WebviewWindow,
};

pub fn toggle(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
            return;
        }
    }
    show(app);
}

pub fn show(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    // Query the current rect, including for Dock clicks and first launch, when
    // no tray event has happened yet. Never rely on a cached monitor/position.
    let rect = tray_rect(app);
    if let Err(error) = position(&window, rect) {
        eprintln!("Could not position panel: {error}");
    }
    if let Err(error) = window.show().and_then(|()| window.set_focus()) {
        eprintln!("Could not show panel: {error}");
    }
}

fn tray_rect(app: &AppHandle) -> Option<(Rect, f64)> {
    let tray = app.tray_by_id(crate::TRAY_ID)?;
    #[cfg(target_os = "macos")]
    {
        tray.with_inner_tray_icon(|tray| {
            let mtm = objc2::MainThreadMarker::new()?;
            let window = tray.ns_status_item()?.button(mtm)?.window()?;
            let rect = tray.rect()?;
            Some((
                Rect {
                    position: rect.position.into(),
                    size: rect.size.into(),
                },
                window.backingScaleFactor(),
            ))
        })
        .ok()
        .flatten()
    }
    #[cfg(not(target_os = "macos"))]
    {
        tray.rect().ok().flatten().map(|rect| (rect, 1.0))
    }
}

fn position(window: &WebviewWindow, rect: Option<(Rect, f64)>) -> tauri::Result<()> {
    // TrayIcon::rect returns physical coordinates on macOS and Windows.
    let tray_scale = rect.as_ref().map(|(_, scale)| *scale);
    let tray = rect.map(|(rect, _)| {
        (
            rect.position.to_physical::<f64>(1.0),
            rect.size.to_physical::<f64>(1.0),
        )
    });
    let tray_monitor = tray.and_then(|(origin, size)| {
        window.available_monitors().ok().and_then(|monitors| {
            monitors.into_iter().find(|monitor| {
                // macOS scales each monitor's global coordinates separately,
                // so their physical bounds can overlap on mixed-DPI setups.
                if cfg!(target_os = "macos") && Some(monitor.scale_factor()) != tray_scale {
                    return false;
                }
                contains_point(
                    monitor.position(),
                    monitor.size(),
                    PhysicalPosition::new(
                        origin.x + size.width / 2.0,
                        origin.y + size.height / 2.0,
                    ),
                )
            })
        })
    });
    // A hidden window may refer to a disconnected monitor. Missing monitors
    // must be handled without the positioner plugin's current_monitor().unwrap().
    let anchor = tray_monitor.as_ref().and(tray);
    let monitor = tray_monitor
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return Ok(()); // Still show the window if macOS is rearranging displays.
    };
    let size = window
        .outer_size()?
        .to_logical::<f64>(window.scale_factor()?)
        .to_physical::<u32>(monitor.scale_factor());
    let position = panel_position(monitor.work_area(), size, anchor);
    // AppKit interprets physical positions using the window's OLD scale factor.
    // Use the destination screen's logical coordinates when moving between a
    // Retina display and a non-Retina display.
    #[cfg(target_os = "macos")]
    let position = position.to_logical::<f64>(monitor.scale_factor());
    window.set_position(position)
}

fn contains_point(
    origin: &PhysicalPosition<i32>,
    size: &PhysicalSize<u32>,
    point: PhysicalPosition<f64>,
) -> bool {
    point.x >= origin.x as f64
        && point.x < origin.x as f64 + size.width as f64
        && point.y >= origin.y as f64
        && point.y < origin.y as f64 + size.height as f64
}

fn panel_position(
    area: &PhysicalRect<i32, u32>,
    window: PhysicalSize<u32>,
    tray: Option<(PhysicalPosition<f64>, PhysicalSize<f64>)>,
) -> PhysicalPosition<i32> {
    let left = area.position.x as f64;
    let top = area.position.y as f64;
    let right = left + area.size.width as f64;
    let bottom = top + area.size.height as f64;
    let width = window.width as f64;
    let height = window.height as f64;
    let (x, y) = match tray {
        Some((origin, size)) => {
            let below = origin.y + size.height;
            let y = if below + height > bottom {
                origin.y - height // Taskbars along the bottom of a screen.
            } else {
                below
            };
            (origin.x + size.width / 2.0 - width / 2.0, y)
        }
        None => (left + (right - left - width) / 2.0, top),
    };
    PhysicalPosition::new(
        x.clamp(left, (right - width).max(left)).round() as i32,
        y.clamp(top, (bottom - height).max(top)).round() as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(x: i32, y: i32, width: u32, height: u32) -> PhysicalRect<i32, u32> {
        PhysicalRect {
            position: PhysicalPosition::new(x, y),
            size: PhysicalSize::new(width, height),
        }
    }

    fn tray(x: f64, y: f64) -> Option<(PhysicalPosition<f64>, PhysicalSize<f64>)> {
        Some((PhysicalPosition::new(x, y), PhysicalSize::new(24.0, 24.0)))
    }

    #[test]
    fn first_open_without_a_tray_rect_uses_the_available_screen() {
        assert_eq!(
            panel_position(&area(0, 24, 1440, 876), PhysicalSize::new(320, 460), None),
            PhysicalPosition::new(560, 24)
        );
    }

    #[test]
    fn panel_stays_inside_right_edge_under_menu_bar() {
        assert_eq!(
            panel_position(
                &area(0, 24, 1440, 876),
                PhysicalSize::new(320, 460),
                tray(1400.0, 0.0)
            ),
            PhysicalPosition::new(1120, 24)
        );
    }

    #[test]
    fn external_monitor_can_have_negative_coordinates() {
        assert_eq!(
            panel_position(
                &area(-1920, -1056, 1920, 1056),
                PhysicalSize::new(320, 460),
                tray(-1000.0, -1080.0)
            ),
            PhysicalPosition::new(-1148, -1056)
        );
    }

    #[test]
    fn bottom_taskbar_opens_panel_above_it() {
        assert_eq!(
            panel_position(
                &area(0, 0, 1920, 1056),
                PhysicalSize::new(320, 460),
                tray(1800.0, 1056.0)
            ),
            PhysicalPosition::new(1600, 596)
        );
    }

    #[test]
    fn panel_larger_than_work_area_does_not_panic() {
        assert_eq!(
            panel_position(
                &area(0, 24, 200, 200),
                PhysicalSize::new(320, 460),
                tray(180.0, 0.0)
            ),
            PhysicalPosition::new(0, 24)
        );
    }

    #[test]
    fn retina_tray_coordinates_are_matched_in_physical_pixels() {
        let point = PhysicalPosition::new(2800.0, 24.0);
        assert!(contains_point(
            &PhysicalPosition::new(0, 0),
            &PhysicalSize::new(2880, 1800),
            point
        ));
        assert!(!contains_point(
            &PhysicalPosition::new(-1920, 0),
            &PhysicalSize::new(1920, 1080),
            point
        ));
    }
}
