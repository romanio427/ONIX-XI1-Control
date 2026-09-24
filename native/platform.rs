use slint::winit_030::WinitWindowAccessor;
#[cfg(any(windows, target_os = "linux"))]
use slint::winit_030::winit;

pub fn backend() -> Result<(), slint::PlatformError> {
    let backend = slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("femtovg".into());
    #[cfg(target_os = "linux")]
    let backend = {
        use winit::platform::x11::EventLoopBuilderExtX11;
        if std::env::var_os("DISPLAY").is_none() {
            return Err(crate::language::text(
                "Для панели ONIX сейчас требуется X11 или XWayland; чистый Wayland ещё не поддержан",
                "The ONIX panel currently requires X11 or XWayland; native Wayland is not supported yet",
            ).into());
        }
        let mut builder: slint::winit_030::EventLoopBuilder =
            winit::event_loop::EventLoop::with_user_event();
        builder.with_x11();
        backend.with_winit_event_loop_builder(builder)
    };
    backend
        .with_winit_window_attributes_hook(|attributes| {
            // Every ONIX window has a deliberate compact layout. Dialogs remain
            // active/taskbar-visible, but none of the windows may be resized.
            let attributes = attributes.with_resizable(false);
            if !overlay_title(&attributes.title) {
                return attributes;
            }
            let attributes = attributes.with_active(false);
            #[cfg(windows)]
            let attributes = {
                use winit::platform::windows::WindowAttributesExtWindows;
                attributes.with_skip_taskbar(true)
            };
            attributes
        })
        .select()
}

fn overlay_title(title: &str) -> bool {
    // Only the volume flyout, its attached settings grid, and the invisible
    // translation anchor stay out of the taskbar. Normal dialogs are apps.
    matches!(title, "" | "ONIX DAC Control" | "ONIX XI1 Settings")
}

pub fn anchor(window: &slint::Window, logical_width: f64) -> (i32, i32, f64) {
    let scaling = f64::from(window.scale_factor());
    #[cfg(windows)]
    if let Some((x, y)) = windows_anchor(scaling, logical_width) {
        return (x, y, scaling);
    }
    #[cfg(target_os = "macos")]
    if let Some(anchor) = macos_anchor(logical_width) {
        return anchor;
    }
    window
        .with_winit_window(|w| {
            let Some(monitor) = w.current_monitor().or_else(|| w.primary_monitor()) else {
                return (0, 0, scaling);
            };
            let scale = monitor.scale_factor();
            let size = monitor.size();
            let pos = monitor.position();
            #[cfg(target_os = "linux")]
            if let Some((left, top, width, height)) = linux_work_area() {
                let x1 = pos.x.max(left);
                let y1 = pos.y.max(top);
                let x2 = (pos.x + size.width as i32).min(left + width);
                let y2 = (pos.y + size.height as i32).min(top + height);
                if x2 - x1 >= (logical_width * scale) as i32 && y2 - y1 >= (114.0 * scale) as i32 {
                    return (
                        x1 + (x2 - x1 - (logical_width * scale) as i32) / 2,
                        y2 - (114.0 * scale) as i32,
                        scale,
                    );
                }
            }
            (
                pos.x + (size.width as i32 - (logical_width * scale) as i32) / 2,
                pos.y + size.height as i32 - (114.0 * scale) as i32,
                scale,
            )
        })
        .unwrap_or((0, 0, scaling))
}

#[cfg(target_os = "linux")]
fn linux_work_area() -> Option<(i32, i32, i32, i32)> {
    use x11rb::{
        connection::Connection,
        protocol::xproto::{AtomEnum, ConnectionExt},
    };
    let (connection, screen) = x11rb::connect(None).ok()?;
    let root = connection.setup().roots.get(screen)?.root;
    let property = |name: &[u8], count: u32| -> Option<Vec<u32>> {
        let atom = connection.intern_atom(true, name).ok()?.reply().ok()?.atom;
        if atom == 0 {
            return None;
        }
        let reply = connection
            .get_property(false, root, atom, AtomEnum::CARDINAL, 0, count)
            .ok()?
            .reply()
            .ok()?;
        Some(reply.value32()?.collect())
    };
    let desktop = *property(b"_NET_CURRENT_DESKTOP", 1)?.first()? as usize;
    let areas = property(b"_NET_WORKAREA", 4096)?;
    let offset = desktop.checked_mul(4)?;
    let values = areas.get(offset..offset.checked_add(4)?)?;
    Some((
        values[0] as i32,
        values[1] as i32,
        i32::try_from(values[2]).ok()?,
        i32::try_from(values[3]).ok()?,
    ))
}

#[cfg(target_os = "macos")]
fn macos_anchor(logical_width: f64) -> Option<(i32, i32, f64)> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSScreen;
    let screens = NSScreen::screens(MainThreadMarker::new()?);
    let primary = screens.firstObject()?;
    let frame = primary.frame();
    let work = primary.visibleFrame();
    let scale = primary.backingScaleFactor();
    // AppKit uses a bottom-left origin; Winit's desktop position starts top-left.
    Some((
        ((work.origin.x + (work.size.width - logical_width) / 2.0) * scale).round() as i32,
        ((frame.size.height - work.origin.y - 114.0) * scale).round() as i32,
        scale,
    ))
}

pub fn position(window: &slint::Window, x: i32, y: i32) {
    window.set_position(slint::PhysicalPosition::new(x, y));
    present_overlay(window);
}

pub fn present_overlay(window: &slint::Window) {
    #[cfg(windows)]
    window.with_winit_window(|w| {
        use slint::winit_030::winit::platform::windows::WindowExtWindows;
        use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        w.set_skip_taskbar(true);
        let Ok(handle) = w.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(win32) = handle.as_raw() else {
            return;
        };
        unsafe {
            apply_overlay_hwnd(win32.hwnd.get() as windows_sys::Win32::Foundation::HWND);
        }
    });
    #[cfg(not(windows))]
    let _ = window;
}

pub fn present_dialog(window: &slint::Window) {
    #[cfg(windows)]
    window.with_winit_window(|w| {
        use slint::winit_030::winit::platform::windows::WindowExtWindows;
        use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        w.set_skip_taskbar(false);
        let Ok(handle) = w.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(win32) = handle.as_raw() else {
            return;
        };
        unsafe {
            apply_dialog_hwnd(win32.hwnd.get() as windows_sys::Win32::Foundation::HWND);
        }
    });
    #[cfg(not(windows))]
    let _ = window;
}

#[cfg(windows)]
unsafe fn apply_dialog_hwnd(hwnd: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, HWND_NOTOPMOST, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE,
        SWP_SHOWWINDOW, SetWindowLongPtrW, SetWindowPos, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW,
    };
    if hwnd.is_null() {
        return;
    }
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let style = (style | WS_EX_APPWINDOW) & !(WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style as isize);
        SetWindowPos(
            hwnd,
            HWND_NOTOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED | SWP_SHOWWINDOW,
        );
    }
}

#[cfg(windows)]
unsafe fn apply_overlay_hwnd(hwnd: windows_sys::Win32::Foundation::HWND) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongPtrW, HWND_TOPMOST, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SetWindowLongPtrW, SetWindowPos,
        ShowWindow, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };
    if hwnd.is_null() {
        return;
    }
    // winit marks unowned windows WS_EX_APPWINDOW and, after the first
    // SW_SHOWNOACTIVATE, subsequent ShowWindow calls use SW_SHOW. That puts
    // the flyout on the taskbar and steals focus during volume adjustment.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let style = style & !WS_EX_APPWINDOW | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style as isize);
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED | SWP_SHOWWINDOW,
        );
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
}

#[cfg(windows)]
fn windows_anchor(scale: f64, logical_width: f64) -> Option<(i32, i32)> {
    use windows_sys::Win32::{
        Foundation::RECT,
        UI::{
            Shell::{ABM_GETTASKBARPOS, APPBARDATA, SHAppBarMessage},
            WindowsAndMessaging::{
                FindWindowW, GetWindowRect, SPI_GETWORKAREA, SystemParametersInfoW,
            },
        },
    };
    // All pointers below refer to correctly sized live stack values; the shell owns HWND.
    unsafe {
        let mut work: RECT = std::mem::zeroed();
        if SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut work as *mut RECT).cast(), 0) == 0 {
            return None;
        }
        let mut bar: APPBARDATA = std::mem::zeroed();
        bar.cbSize = std::mem::size_of::<APPBARDATA>() as u32;
        let mut left = work.left;
        let mut right = work.right;
        let mut bottom = work.bottom;
        if SHAppBarMessage(ABM_GETTASKBARPOS, &mut bar) != 0
            && bar.uEdge == 3
            && bar.rc.right > bar.rc.left
        {
            left = bar.rc.left;
            right = bar.rc.right;
            bottom = bar.rc.top;
            let class: Vec<u16> = "Shell_TrayWnd\0".encode_utf16().collect();
            let hwnd = FindWindowW(class.as_ptr(), std::ptr::null());
            let mut actual: RECT = std::mem::zeroed();
            if !hwnd.is_null() && GetWindowRect(hwnd, &mut actual) != 0 {
                bottom = actual.top.clamp(bar.rc.top.min(work.bottom), work.bottom);
                if work.bottom - bottom <= 2 {
                    bottom = work.bottom;
                }
            }
        }
        Some((
            left + (right - left - (logical_width * scale).round() as i32) / 2,
            bottom - (114.0 * scale).round() as i32,
        ))
    }
}
