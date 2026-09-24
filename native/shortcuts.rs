use crate::session::CONNECTED;
use std::sync::atomic::Ordering;

pub fn status(text: impl Into<String>) {
    let text = text.into();
    let _ = slint::invoke_from_event_loop(move || {
        crate::with_app(|app| app.tray.set_shortcuts(text.into()))
    });
}

pub fn dispatch(action: crate::hotkeys::Action) {
    if !CONNECTED.load(Ordering::Acquire) {
        return;
    }
    let _ = slint::invoke_from_event_loop(move || {
        crate::with_app(|app| {
            if app.panel.get_connected() {
                match action {
                    crate::hotkeys::Action::Volume(direction) => app.shortcut_volume(direction),
                    crate::hotkeys::Action::Setting(setting, direction) => {
                        app.shortcut_setting(setting, direction)
                    }
                }
            }
        })
    });
}

#[cfg(windows)]
pub use windows::Hook;

#[cfg(windows)]
mod windows {
    use super::*;
    use crate::hotkeys::Config;
    use std::{
        sync::{Arc, atomic::AtomicU32, mpsc},
        thread,
        time::Duration,
    };
    use windows_sys::Win32::{
        System::Threading::GetCurrentThreadId,
        UI::{
            Input::KeyboardAndMouse::{
                MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey,
                UnregisterHotKey,
            },
            WindowsAndMessaging::*,
        },
    };

    pub struct Hook {
        thread_id: Arc<AtomicU32>,
        worker: Option<thread::JoinHandle<()>>,
    }

    impl Hook {
        pub fn start(config: Config) -> Self {
            let thread_id = Arc::new(AtomicU32::new(0));
            let id = thread_id.clone();
            let (ready_tx, ready_rx) = mpsc::sync_channel(1);
            let worker = thread::spawn(move || unsafe {
                let mut message: MSG = std::mem::zeroed();
                PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);
                id.store(GetCurrentThreadId(), Ordering::Release);
                let mut registered = Vec::new();
                let mut unavailable = 0;
                for (index, binding) in config.0.iter().copied().enumerate() {
                    let Some(binding) = binding else { continue };
                    let (mut modifiers, key) = registered_hotkey(binding);
                    if matches!(
                        crate::hotkeys::ACTIONS[index],
                        crate::hotkeys::Action::Setting(_, _)
                    ) {
                        modifiers |= MOD_NOREPEAT;
                    }
                    let hotkey_id = index as i32 + 1;
                    if RegisterHotKey(std::ptr::null_mut(), hotkey_id, modifiers, u32::from(key))
                        != 0
                    {
                        registered.push(hotkey_id);
                    } else {
                        unavailable += 1;
                    }
                }
                crate::instance::debug_log(&format!(
                    "windows hotkeys registered={} unavailable={unavailable}",
                    registered.len(),
                ));
                let _ = ready_tx.send(true);
                let assigned = config.0.iter().flatten().count() - unavailable;
                status(if unavailable == 0 {
                    format!(
                        "{}: {assigned}",
                        crate::language::text("Назначено горячих клавиш", "Assigned hotkeys")
                    )
                } else {
                    format!(
                        "{}: {assigned}; {}: {unavailable}",
                        crate::language::text("Назначено", "Assigned"),
                        crate::language::text("недоступно", "unavailable")
                    )
                });
                while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                    if message.message == WM_HOTKEY {
                        let index = message.wParam.saturating_sub(1);
                        if let Some(action) = crate::hotkeys::ACTIONS.get(index).copied() {
                            dispatch(action);
                        }
                    } else {
                        TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
                for id in registered {
                    UnregisterHotKey(std::ptr::null_mut(), id);
                }
            });
            // Do not return until Windows is listening again. Otherwise the
            // first shortcut can leak through to the system volume overlay.
            let _ = ready_rx.recv_timeout(Duration::from_secs(2));
            Self {
                thread_id,
                worker: Some(worker),
            }
        }
    }

    fn registered_hotkey(binding: crate::hotkeys::Binding) -> (u32, u16) {
        let mut flags = 0;
        for (modifier, native) in [
            (crate::hotkeys::MOD_CTRL, MOD_CONTROL),
            (crate::hotkeys::MOD_ALT, MOD_ALT),
            (crate::hotkeys::MOD_SHIFT, MOD_SHIFT),
            (crate::hotkeys::MOD_META, MOD_WIN),
        ] {
            if binding.modifiers() & modifier != 0 {
                flags |= native;
            }
        }
        (flags, binding.key())
    }

    impl Drop for Hook {
        fn drop(&mut self) {
            let id = self.thread_id.load(Ordering::Acquire);
            if id != 0 {
                unsafe {
                    PostThreadMessageW(id, WM_QUIT, 0, 0);
                }
            }
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }
}

#[cfg(target_os = "linux")]
#[path = "shortcuts_linux.rs"]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::Hook;
#[cfg(target_os = "macos")]
#[path = "shortcuts_macos.rs"]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::Hook;
