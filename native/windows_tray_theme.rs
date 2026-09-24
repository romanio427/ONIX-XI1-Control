//! Follow the Windows app theme for the native Win32 tray menu.
//!
//! The theme setting and change notification are public WinRT APIs. The menu
//! opt-in is an undocumented uxtheme API, so missing exports leave the default
//! Windows menu untouched.

use std::cell::Cell;
use windows::{
    Foundation::TypedEventHandler,
    UI::ViewManagement::{UIColorType, UISettings},
};
use windows_sys::Win32::{
    Foundation::FreeLibrary,
    System::LibraryLoader::{GetProcAddress, LoadLibraryW},
};

type SetPreferredAppMode = unsafe extern "system" fn(i32) -> i32;
type FlushMenuThemes = unsafe extern "system" fn();

const ALLOW_DARK: i32 = 1;
const FORCE_DARK: i32 = 2;
const FORCE_LIGHT: i32 = 3;

#[link(name = "runtimeobject")]
unsafe extern "system" {
    fn RoInitialize(init_type: u32) -> i32;
    fn RoUninitialize();
}

pub struct TrayTheme {
    module: windows_sys::Win32::Foundation::HMODULE,
    set_mode: SetPreferredAppMode,
    flush: FlushMenuThemes,
    settings: Option<UISettings>,
    change_token: Option<i64>,
    current_mode: Cell<Option<i32>>,
    ro_initialized: bool,
}

impl TrayTheme {
    pub fn new() -> Option<Self> {
        const UXTHEME: &[u16] = &[
            b'u' as u16,
            b'x' as u16,
            b't' as u16,
            b'h' as u16,
            b'e' as u16,
            b'm' as u16,
            b'e' as u16,
            b'.' as u16,
            b'd' as u16,
            b'l' as u16,
            b'l' as u16,
            0,
        ];

        unsafe {
            let module = LoadLibraryW(UXTHEME.as_ptr());
            if module.is_null() {
                return None;
            }
            // GetProcAddress encodes an ordinal as a pointer (MAKEINTRESOURCEA).
            let set_mode = GetProcAddress(module, 135usize as *const u8);
            let flush = GetProcAddress(module, 136usize as *const u8);
            let (Some(set_mode), Some(flush)) = (set_mode, flush) else {
                FreeLibrary(module);
                return None;
            };

            // RO_INIT_SINGLETHREADED. If COM is already initialized in another
            // mode, UISettings may still work; only uninitialize on success.
            let ro_initialized = RoInitialize(0) >= 0;
            let settings = UISettings::new().ok();
            let change_token = settings.as_ref().and_then(|settings| {
                let handler = TypedEventHandler::new(|_, _| {
                    let _ = slint::invoke_from_event_loop(|| {
                        crate::with_app(|app| {
                            if let Some(theme) = &app.tray_theme {
                                theme.refresh();
                            }
                        });
                    });
                    Ok(())
                });
                settings.ColorValuesChanged(&handler).ok()
            });

            let theme = Self {
                module,
                set_mode: std::mem::transmute(set_mode),
                flush: std::mem::transmute(flush),
                settings,
                change_token,
                current_mode: Cell::new(None),
                ro_initialized,
            };
            theme.refresh();
            Some(theme)
        }
    }

    fn refresh(&self) {
        let mode = self
            .settings
            .as_ref()
            .and_then(|settings| settings.GetColorValue(UIColorType::Foreground).ok())
            .map(|foreground| {
                // Microsoft's foreground-brightness threshold for identifying
                // the user's Windows app color mode.
                let brightness = 5 * u16::from(foreground.G)
                    + 2 * u16::from(foreground.R)
                    + u16::from(foreground.B);
                if brightness > 8 * 128 {
                    FORCE_DARK
                } else {
                    FORCE_LIGHT
                }
            })
            .unwrap_or(ALLOW_DARK);
        if self.current_mode.get() == Some(mode) {
            return;
        }
        unsafe {
            (self.set_mode)(mode);
            (self.flush)();
        }
        self.current_mode.set(Some(mode));
    }
}

impl Drop for TrayTheme {
    fn drop(&mut self) {
        if let (Some(settings), Some(token)) = (&self.settings, self.change_token) {
            let _ = settings.RemoveColorValuesChanged(token);
        }
        self.settings.take();
        unsafe {
            FreeLibrary(self.module);
            if self.ro_initialized {
                RoUninitialize();
            }
        }
    }
}
