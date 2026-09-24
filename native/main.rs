#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod cli;
mod hotkeys;
mod instance;
mod ipc;
mod language;
mod platform;
mod protocol;
mod session;
mod shortcuts;
mod startup;
mod usb;
#[cfg(windows)]
mod windows_tray_theme;
#[cfg(windows)]
mod winusb_setup;

use protocol::Settings;
use session::{Operation, Reply, Session};
use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
slint::include_modules!();

thread_local! { static APP: RefCell<Option<std::rc::Weak<App>>> = const { RefCell::new(None) }; }

struct App {
    panel: VolumePanel,
    settings: SettingsPanel,
    tray: OnixTray,
    #[cfg(windows)]
    tray_theme: Option<windows_tray_theme::TrayTheme>,
    about: AboutWindow,
    preferences: PreferencesWindow,
    hotkeys: HotkeyWindow,
    hotkey_config: Cell<hotkeys::Config>,
    hotkey_draft: Cell<hotkeys::Config>,
    hotkey_editing: Cell<bool>,
    session: Option<Session>,
    visible: Cell<bool>,
    leaving: Cell<bool>,
    expanded: Cell<bool>,
    expand_on_show: Cell<bool>,
    positioned: Cell<bool>,
    revision: Cell<u64>,
    motion_revision: Cell<u64>,
    settings_revision: Cell<u64>,
    anchor: Cell<(i32, i32, f64)>,
    hide_timer: Timer,
    notice_timer: Timer,
    connection_seen: Cell<bool>,
    last_settings: Cell<Option<Settings>>,
    disconnecting: Cell<bool>,
    closing: Cell<bool>,
    demo: bool,
    shortcuts: RefCell<Option<shortcuts::Hook>>,
}

impl App {
    fn refresh_language_ui(&self) {
        self.preferences
            .set_language_title(language::text("Русский", "English").into());
        self.tray.set_startup_status(startup::label().into());
        self.preferences.set_startup_title(
            language::text(
                if startup::enabled() {
                    "Вкл"
                } else {
                    "Выкл"
                },
                if startup::enabled() { "On" } else { "Off" },
            )
            .into(),
        );
        self.preferences
            .set_hotkeys_title(language::text("Открыть", "Open").into());
        self.tray.set_status(if self.panel.get_connected() {
            language::xi1_connected().into()
        } else {
            language::xi1_disconnected().into()
        });
    }

    fn toggle_language(self: &Rc<Self>) {
        let next = language::current().toggle();
        if let Err(error) = next.save().and_then(|_| next.apply()) {
            self.tray.set_status(error.into());
            return;
        }
        self.refresh_language_ui();
        if self.hotkey_editing.get() {
            self.refresh_hotkey_labels();
            self.hotkeys.set_message(hotkey_help().into());
        } else if !self.demo && self.panel.get_connected() {
            self.shortcuts.borrow_mut().take();
            *self.shortcuts.borrow_mut() = Some(shortcuts::Hook::start(self.hotkey_config.get()));
        }
    }

    fn open_about(self: &Rc<Self>) {
        if self.closing.get() {
            return;
        }
        self.hide();
        if let Err(error) = self.about.show() {
            self.tray.set_status(
                format!(
                    "{}: {error}",
                    language::text("Не удалось открыть «О программе»", "Could not open About")
                )
                .into(),
            );
            return;
        }
        let weak = Rc::downgrade(self);
        let _ = slint::spawn_local(async move {
            let Some(app) = weak.upgrade() else {
                return;
            };
            let Ok(window) = app.about.window().winit_window().await else {
                return;
            };
            if app.closing.get() || !app.about.window().is_visible() {
                return;
            }
            platform::present_dialog(app.about.window());
            // show() is a no-op for an already visible window. Restore and
            // activate only in response to the user's explicit open command.
            window.set_minimized(false);
            window.focus_window();
        });
    }

    fn refresh_hotkey_labels(&self) {
        self.hotkeys.set_bindings(ModelRc::new(VecModel::from(
            self.hotkey_draft
                .get()
                .labels()
                .into_iter()
                .map(SharedString::from)
                .collect::<Vec<_>>(),
        )));
    }

    fn resume_shortcuts(&self) {
        if !self.demo
            && !self.closing.get()
            && self.panel.get_connected()
            && self.shortcuts.borrow().is_none()
        {
            *self.shortcuts.borrow_mut() = Some(shortcuts::Hook::start(self.hotkey_config.get()));
        }
    }

    fn open_hotkeys(self: &Rc<Self>) {
        if self.closing.get() {
            return;
        }
        let newly_opened = !self.hotkey_editing.replace(true);
        if newly_opened {
            self.hotkey_draft.set(self.hotkey_config.get());
            self.hotkeys.set_recording(-1);
            self.refresh_hotkey_labels();
            self.hotkeys.set_message(hotkey_help().into());
        }
        self.hide();
        if let Err(error) = self.hotkeys.show() {
            self.tray.set_status(error.to_string().into());
            self.close_hotkeys(false);
            return;
        }
        let weak = Rc::downgrade(self);
        let _ = slint::spawn_local(async move {
            let Some(app) = weak.upgrade() else {
                return;
            };
            let Ok(window) = app.hotkeys.window().winit_window().await else {
                return;
            };
            if !app.hotkey_editing.get() {
                return;
            }
            platform::present_dialog(app.hotkeys.window());
            window.set_minimized(false);
            if newly_opened
                && let Some(monitor) = window
                    .current_monitor()
                    .or_else(|| window.primary_monitor())
            {
                let size = window.outer_size();
                let screen = monitor.size();
                let origin = monitor.position();
                window.set_outer_position(slint::winit_030::winit::dpi::PhysicalPosition::new(
                    origin.x + (screen.width as i32 - size.width as i32) / 2,
                    origin.y + (screen.height as i32 - size.height as i32) / 2,
                ));
            }
            window.focus_window();
        });
    }

    fn open_preferences(self: &Rc<Self>) {
        if self.closing.get() {
            return;
        }
        self.hide();
        self.refresh_language_ui();
        if let Err(error) = self.preferences.show() {
            self.tray.set_status(error.to_string().into());
            return;
        }
        let weak = Rc::downgrade(self);
        let _ = slint::spawn_local(async move {
            let Some(app) = weak.upgrade() else {
                return;
            };
            let Ok(window) = app.preferences.window().winit_window().await else {
                return;
            };
            if app.closing.get() || !app.preferences.window().is_visible() {
                return;
            }
            platform::present_dialog(app.preferences.window());
            window.set_minimized(false);
            window.focus_window();
        });
    }

    fn toggle_startup(self: &Rc<Self>) {
        if self.demo {
            self.tray.set_status(
                language::text(
                    "Демонстрация: автозапуск не изменяется",
                    "Demo: startup setting is unchanged",
                )
                .into(),
            );
            return;
        }
        match startup::set_enabled(!startup::enabled()) {
            Ok(()) => self.refresh_language_ui(),
            Err(error) => self.tray.set_status(error.to_string().into()),
        }
    }

    fn close_hotkeys(&self, save: bool) {
        if !self.hotkey_editing.get() {
            return;
        }
        if save {
            let config = self.hotkey_draft.get();
            let result = if self.demo {
                config.validate()
            } else {
                config.save()
            };
            if let Err(error) = result {
                self.hotkeys.set_message(error.into());
                return;
            }
            self.hotkey_config.set(config);
            // The active hook still has the previously saved configuration.
            self.shortcuts.borrow_mut().take();
        }
        self.hotkeys.set_recording(-1);
        self.hotkey_editing.set(false);
        let _ = self.hotkeys.hide();
        self.resume_shortcuts();
    }

    fn show(self: &Rc<Self>, open_settings: bool) {
        if self.closing.get() || (open_settings && self.disconnecting.get()) {
            return;
        }
        if open_settings {
            self.expand_on_show.set(true);
        }
        if !self.visible.get() || self.leaving.get() {
            self.motion_revision.set(self.motion_revision.get() + 1);
        }
        self.leaving.set(false);
        if !self.visible.replace(true) {
            self.positioned.set(false);
            if let Err(error) = self.panel.show() {
                self.visible.set(false);
                self.tray.set_status(
                    format!(
                        "{}: {error}",
                        language::text("Не удалось показать панель", "Could not show panel")
                    )
                    .into(),
                );
                return;
            }
        }
        if !self.positioned.get() {
            let weak = Rc::downgrade(self);
            let panel = self.panel.as_weak();
            let revision = self.motion_revision.get();
            // Wait for the actual native window instead of guessing the first-frame delay.
            let _ = slint::spawn_local(async move {
                let Some(panel) = panel.upgrade() else {
                    return;
                };
                if panel.window().winit_window().await.is_err() {
                    return;
                }
                let Some(app) = weak.upgrade() else {
                    return;
                };
                if app.motion_revision.get() != revision {
                    return;
                }
                if !app.visible.get() || app.leaving.get() || app.positioned.get() {
                    return;
                }
                platform::present_overlay(app.panel.window());
                app.anchor.set(platform::anchor(
                    app.panel.window(),
                    f64::from(app.panel.get_panel_width()),
                ));
                let (x, y, _) = app.anchor.get();
                platform::position(app.panel.window(), x, y);
                app.positioned.set(true);
                app.panel.set_appearing(true);
                if app.expand_on_show.replace(false) && !app.expanded.get() {
                    app.toggle();
                }
                app.arm_hide();
            });
        } else {
            platform::present_overlay(self.panel.window());
            self.panel.set_appearing(true);
            if self.expand_on_show.replace(false) && !self.expanded.get() {
                self.toggle();
            }
            self.arm_hide();
        }
    }

    fn toggle(self: &Rc<Self>) {
        if self.closing.get() || self.disconnecting.get() || !self.positioned.get() {
            return;
        }
        self.notice_timer.stop();
        self.panel.set_notice(SharedString::default());
        if self.leaving.get() {
            self.show(false);
        }
        let expanded = !self.expanded.get();
        self.expanded.set(expanded);
        self.panel.set_expanded(expanded);
        self.settings_revision.set(self.settings_revision.get() + 1);
        self.hide_timer.stop();
        if expanded {
            let (x, y, scale) = self.anchor.get();
            platform::position(
                self.settings.window(),
                x,
                y - (f64::from(self.settings.get_panel_offset()) * scale).round() as i32,
            );
            let _ = self.settings.show();
            platform::present_overlay(self.settings.window());
            self.settings.set_expanded(true);
        } else {
            self.collapse();
        }
        self.arm_hide();
    }

    fn collapse(self: &Rc<Self>) {
        self.expanded.set(false);
        self.panel.set_expanded(false);
        self.settings.set_expanded(false);
        let revision = self.settings_revision.get() + 1;
        self.settings_revision.set(revision);
        let weak = Rc::downgrade(self);
        Timer::single_shot(Duration::from_millis(100), move || {
            if let Some(app) = weak.upgrade()
                && app.settings_revision.get() == revision
                && !app.expanded.get()
            {
                let _ = app.settings.hide();
            }
        });
    }

    fn hide(self: &Rc<Self>) {
        if !self.visible.get() || self.leaving.replace(true) {
            return;
        }
        self.expand_on_show.set(false);
        self.hide_timer.stop();
        self.notice_timer.stop();
        self.collapse();
        self.panel.set_appearing(false);
        let revision = self.motion_revision.get() + 1;
        self.motion_revision.set(revision);
        let weak = Rc::downgrade(self);
        Timer::single_shot(Duration::from_millis(200), move || {
            if let Some(app) = weak.upgrade() {
                if app.motion_revision.get() != revision || !app.leaving.get() {
                    return;
                }
                let _ = app.panel.hide();
                app.visible.set(false);
                app.leaving.set(false);
                app.panel.set_notice(SharedString::default());
            }
        });
    }

    fn arm_hide(self: &Rc<Self>) {
        self.hide_timer.stop();
        if self.leaving.get()
            || !self.visible.get()
            || !self.panel.get_notice().is_empty()
            || self.interacting()
        {
            return;
        }
        let weak = Rc::downgrade(self);
        self.hide_timer
            .start(TimerMode::SingleShot, Duration::from_secs(2), move || {
                if let Some(app) = weak.upgrade()
                    && !app.interacting()
                {
                    app.hide();
                }
            });
    }

    fn interacting(&self) -> bool {
        self.panel.get_hovered()
            || self.panel.get_dragging()
            || (self.expanded.get() && (self.settings.get_hovered() || self.settings.get_pressed()))
    }

    fn send(&self, operation: Operation) {
        if self.disconnecting.get() {
            return;
        }
        let revision = self.revision.get() + 1;
        if let Some(session) = &self.session {
            if session.send(operation, revision) {
                self.revision.set(revision);
            } else {
                self.tray.set_status(
                    language::text(
                        "Очередь ЦАПа занята; повторите действие",
                        "DAC queue is busy; try again",
                    )
                    .into(),
                );
            }
        }
    }

    fn shortcut_volume(self: &Rc<Self>, direction: i32) {
        if self.disconnecting.get() || !self.panel.get_connected() {
            return;
        }
        let current = self.panel.get_volume();
        let target = (current + direction).clamp(0, i32::from(protocol::VOLUME_WRITE_MAX));
        if target != current {
            self.panel.set_volume(target);
            self.send(Operation::SetVolume(target as u8));
        }
        self.show(false);
    }

    fn shortcut_setting(self: &Rc<Self>, setting: protocol::Setting, direction: i32) {
        if self.disconnecting.get() || !self.panel.get_connected() {
            return;
        }
        self.send(Operation::Cycle(setting, direction));
    }

    fn reply(self: &Rc<Self>, reply: Reply) {
        if self.closing.get() {
            return;
        }
        if reply.removed {
            self.device_removed();
            return;
        }
        if self.disconnecting.get() && reply.settings.is_none() {
            return;
        }
        if reply.settings.is_some() && reply.revision < self.revision.get() {
            return;
        }
        if reply.settings.is_none() {
            self.shortcuts.borrow_mut().take();
            session::CONNECTED.store(false, std::sync::atomic::Ordering::Release);
        }
        self.panel.set_connected(reply.settings.is_some());
        self.settings.set_connected(reply.settings.is_some());
        #[cfg(windows)]
        self.preferences.set_device_access_visible(
            reply.settings.is_none() && reply.winusb_required && !winusb_setup::ready(),
        );
        self.tray.set_status(if reply.settings.is_some() {
            language::xi1_connected().into()
        } else {
            crate::instance::debug_log(&format!(
                "ui error reply removed={} winusb_required={} {}",
                reply.removed, reply.winusb_required, reply.error
            ));
            #[cfg(windows)]
            if reply.winusb_required {
                winusb_setup::offer_if_needed();
            }
            reply.error.into()
        });
        if let Some(settings) = reply.settings {
            crate::instance::debug_log("ui connected reply");
            let previous = self.last_settings.replace(Some(settings));
            let new_connection = !self.connection_seen.replace(true);
            self.disconnecting.set(false);
            let _ = self.tray.show();
            if !self.demo && !self.hotkey_editing.get() && self.shortcuts.borrow().is_none() {
                *self.shortcuts.borrow_mut() =
                    Some(shortcuts::Hook::start(self.hotkey_config.get()));
            }
            if !self.panel.get_dragging() {
                self.panel.set_volume(i32::from(settings.volume));
            }
            self.settings.set_values(ModelRc::new(VecModel::from(
                settings
                    .labels()
                    .into_iter()
                    .map(SharedString::from)
                    .collect::<Vec<_>>(),
            )));
            if new_connection {
                self.connection_notice(true);
            } else if reply.announce_change {
                if let Some(notice) = previous.and_then(|old| setting_notice(old, settings)) {
                    self.transient_notice(notice);
                } else if self.panel.get_notice().is_empty() {
                    // A knob-only change has no setting label, so show volume.
                    // A duplicate USB notification after our own command must
                    // not erase the shortcut confirmation already on screen.
                    self.show(false);
                }
            }
        }
    }

    fn connection_notice(self: &Rc<Self>, connected: bool) {
        self.transient_notice(if connected {
            language::dac_connected()
        } else {
            language::dac_disconnected()
        });
    }

    fn transient_notice(self: &Rc<Self>, text: impl Into<SharedString>) {
        self.notice_timer.stop();
        self.expand_on_show.set(false);
        if self.expanded.get() {
            self.collapse();
        }
        self.panel.set_notice(text.into());
        // Notice text can widen the compact window. Re-anchor after its size
        // binding changes so longer values grow symmetrically on every OS.
        self.positioned.set(false);
        self.show(false);
        let weak = Rc::downgrade(self);
        self.notice_timer
            .start(TimerMode::SingleShot, Duration::from_secs(2), move || {
                if let Some(app) = weak.upgrade() {
                    if !app.disconnecting.get()
                        && (app.panel.get_hovered()
                            || app.panel.get_dragging()
                            || (app.expanded.get()
                                && (app.settings.get_hovered() || app.settings.get_pressed())))
                    {
                        app.panel.set_notice(SharedString::default());
                        app.arm_hide();
                    } else {
                        app.hide();
                    }
                }
            });
    }

    fn watcher_removed(self: &Rc<Self>) {
        if usb::present() == Ok(false) {
            self.device_removed();
        }
    }

    fn device_removed(self: &Rc<Self>) {
        if self.closing.get() || self.disconnecting.replace(true) {
            return;
        }
        crate::instance::debug_log("device_removed; showing notice");
        self.connection_seen.set(false);
        self.last_settings.set(None);
        session::CONNECTED.store(false, std::sync::atomic::Ordering::Release);
        self.shortcuts.borrow_mut().take();
        self.panel.set_connected(false);
        self.settings.set_connected(false);
        self.collapse();
        let _ = self.tray.show();
        self.tray.set_status(language::xi1_disconnected().into());
        self.connection_notice(false);

        let weak = Rc::downgrade(self);
        Timer::single_shot(Duration::from_secs(2), move || {
            if let Some(app) = weak.upgrade()
                && app.disconnecting.get()
            {
                crate::instance::debug_log("disconnect notice finished; continuing to watch");
                app.hide();
            }
        });
    }

    fn quit(&self, _user_requested: bool) {
        crate::instance::debug_log(&format!("quit user={_user_requested}"));
        self.closing.set(true);
        session::CONNECTED.store(false, std::sync::atomic::Ordering::Release);
        self.shortcuts.borrow_mut().take();
        self.hide_timer.stop();
        self.notice_timer.stop();
        let _ = self.panel.hide();
        let _ = self.settings.hide();
        let _ = self.tray.hide();
        let _ = slint::quit_event_loop();
    }
}

fn setting_notice(previous: Settings, current: Settings) -> Option<String> {
    let old = previous.labels();
    let new = current.labels();
    let changed = protocol::Setting::ALL
        .iter()
        .filter(|setting| old[setting.index()] != new[setting.index()])
        .collect::<Vec<_>>();
    match changed.as_slice() {
        [] => None,
        [setting] => Some(format!(
            "{}: {}",
            setting.notice_name(),
            new[setting.index()]
        )),
        _ => Some(language::text("Настройки изменены", "Settings changed").into()),
    }
}

fn hotkey_help() -> &'static str {
    if cfg!(target_os = "linux") {
        language::text(
            "Выберите поле, затем нажмите одну клавишу или сочетание с Ctrl, Alt, Shift либо Super (клавишами-модификаторами).\nПримеры: одна клавиша — F8; сочетание — Ctrl + Shift + F (Ctrl и Shift — модификаторы, F — основная клавиша).\nДля каждого параметра можно назначить одно направление («Вперёд» или «Назад») или оба. Варианты повторяются в каждом направлении: после последнего идёт первый, а перед первым — последний.",
            "Select a field, then press one key or a shortcut with Ctrl, Alt, Shift, or Super (modifier keys).\nExamples: one key — F8; shortcut — Ctrl + Shift + F (Ctrl and Shift are modifiers; F is the main key).\nFor each setting, assign one direction (Forward or Back) or both. Options repeat in either direction: after the last comes the first, and before the first comes the last.",
        )
    } else if cfg!(target_os = "macos") {
        language::text(
            "Выберите поле, затем нажмите одну клавишу или сочетание с Control, Option, Shift либо Command (клавишами-модификаторами).\nПримеры: одна клавиша — F8; сочетание — Command + Shift + F (Command и Shift — модификаторы, F — основная клавиша).\nДля каждого параметра можно назначить одно направление («Вперёд» или «Назад») или оба. Варианты повторяются в каждом направлении: после последнего идёт первый, а перед первым — последний.",
            "Select a field, then press one key or a shortcut with Control, Option, Shift, or Command (modifier keys).\nExamples: one key — F8; shortcut — Command + Shift + F (Command and Shift are modifiers; F is the main key).\nFor each setting, assign one direction (Forward or Back) or both. Options repeat in either direction: after the last comes the first, and before the first comes the last.",
        )
    } else {
        language::text(
            "Выберите поле, затем нажмите одну клавишу или сочетание с Ctrl, Alt, Shift либо Win (клавишами-модификаторами).\nПримеры: одна клавиша — F8; сочетание — Ctrl + Shift + F (Ctrl и Shift — модификаторы, F — основная клавиша).\nДля каждого параметра можно назначить одно направление («Вперёд» или «Назад») или оба. Варианты повторяются в каждом направлении: после последнего идёт первый, а перед первым — последний.",
            "Select a field, then press one key or a shortcut with Ctrl, Alt, Shift, or Win (modifier keys).\nExamples: one key — F8; shortcut — Ctrl + Shift + F (Ctrl and Shift are modifiers; F is the main key).\nFor each setting, assign one direction (Forward or Back) or both. Options repeat in either direction: after the last comes the first, and before the first comes the last.",
        )
    }
}

fn with_app(action: impl FnOnce(Rc<App>)) {
    APP.with(|slot| {
        if let Some(app) = slot.borrow().as_ref().and_then(|w| w.upgrade()) {
            action(app);
        }
    });
}

fn install_panic_log() {
    std::panic::set_hook(Box::new(|info| {
        let location = info
            .location()
            .map(|location| format!("{}:{}", location.file(), location.line()))
            .unwrap_or_else(|| "unknown".into());
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("panic");
        let text = format!("{location}\n{payload}");
        if let Ok(dir) = instance::data_dir() {
            let _ = std::fs::write(dir.join("last-panic.log"), &text);
        }
    }));
}

fn main() -> std::process::ExitCode {
    install_panic_log();
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ONIX XI1: {error}");
            if let Ok(dir) = instance::data_dir() {
                let _ = std::fs::write(dir.join("last-startup-error.log"), error.to_string());
            }
            #[cfg(windows)]
            if !std::env::args().any(|arg| arg.starts_with("--")) {
                use windows_sys::Win32::UI::WindowsAndMessaging::{
                    MB_ICONERROR, MB_OK, MessageBoxW,
                };
                let text: Vec<u16> = format!("ONIX XI1: {error}\0").encode_utf16().collect();
                let title: Vec<u16> = "ONIX DAC Control\0".encode_utf16().collect();
                unsafe {
                    MessageBoxW(
                        std::ptr::null_mut(),
                        text.as_ptr(),
                        title.as_ptr(),
                        MB_OK | MB_ICONERROR,
                    );
                }
            }
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let preferred_language = language::load();
    language::prepare(preferred_language);
    let Some(gui) = cli::prepare_gui(&args)? else {
        return Ok(());
    };
    let demo = gui.demo;
    let _instance = match instance::Instance::try_acquire()? {
        Some(instance) => instance,
        None => {
            for _ in 0..5 {
                if cli::signal_running(&args) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            return Ok(());
        }
    };
    // Listen for commands before Slint init so a second launch can activate us.
    let server = if demo {
        None
    } else {
        Some(ipc::Server::start()?)
    };
    if !demo {
        startup::refresh();
    }
    let initially_present = demo || usb::present() == Ok(true);
    platform::backend()?;
    let _translation_anchor = TranslationAnchor::new()?;
    preferred_language.apply()?;
    let panel = VolumePanel::new()?;
    let settings = SettingsPanel::new()?;
    #[cfg(windows)]
    let tray_theme = windows_tray_theme::TrayTheme::new();
    let tray = OnixTray::new()?;
    let about = AboutWindow::new()?;
    let preferences = PreferencesWindow::new()?;
    let package_version = env!("CARGO_PKG_VERSION");
    about.set_version(package_version.into());
    let hotkey_window = HotkeyWindow::new()?;
    let loaded_hotkeys = hotkeys::Config::load();
    // A corrupt file must not silently bind ordinary keys to volume actions.
    let hotkey_config = loaded_hotkeys
        .as_ref()
        .copied()
        .unwrap_or(hotkeys::Config([None; hotkeys::ACTION_COUNT]));
    let app = Rc::new(App {
        panel,
        settings,
        tray,
        #[cfg(windows)]
        tray_theme,
        about,
        preferences,
        hotkeys: hotkey_window,
        hotkey_config: Cell::new(hotkey_config),
        hotkey_draft: Cell::new(hotkey_config),
        hotkey_editing: Cell::new(false),
        session: if demo {
            None
        } else {
            Some(Session::start(|reply| {
                let _ = slint::invoke_from_event_loop(move || with_app(|app| app.reply(reply)));
            }))
        },
        visible: Cell::new(false),
        leaving: Cell::new(false),
        expanded: Cell::new(false),
        expand_on_show: Cell::new(false),
        positioned: Cell::new(false),
        revision: Cell::new(0),
        motion_revision: Cell::new(0),
        settings_revision: Cell::new(0),
        anchor: Cell::new((0, 0, 1.0)),
        hide_timer: Timer::default(),
        notice_timer: Timer::default(),
        connection_seen: Cell::new(false),
        last_settings: Cell::new(None),
        disconnecting: Cell::new(!initially_present),
        closing: Cell::new(false),
        demo,
        shortcuts: RefCell::new(None),
    });
    APP.with(|slot| *slot.borrow_mut() = Some(Rc::downgrade(&app)));
    app.refresh_language_ui();
    #[cfg(windows)]
    if !demo {
        app.preferences
            .set_device_access_visible(winusb_setup::needed());
        winusb_setup::offer_if_needed();
        app.refresh_language_ui();
    }
    let _ = app.tray.show();
    crate::instance::debug_log("tray shown");
    app.hotkeys.on_record(|index| {
        with_app(|app| {
            if !(0..hotkeys::ACTION_COUNT as i32).contains(&index) {
                return;
            }
            // Registered global shortcuts must be released only while the
            // selected field records a replacement. Merely viewing this
            // window must not disable ONIX controls.
            app.shortcuts.borrow_mut().take();
            app.hotkeys.set_recording(index);
            app.hotkeys.set_message(
                language::text(
                    "Нажмите клавишу или сочетание, которое хотите назначить.",
                    "Press the key or shortcut you want to assign.",
                )
                .into(),
            );
        })
    });
    app.hotkeys.on_clear(|index| {
        with_app(|app| {
            if !(0..hotkeys::ACTION_COUNT as i32).contains(&index) {
                return;
            }
            let mut config = app.hotkey_draft.get();
            config.0[index as usize] = None;
            app.hotkey_draft.set(config);
            app.hotkeys.set_recording(-1);
            app.refresh_hotkey_labels();
            app.resume_shortcuts();
        })
    });
    app.hotkeys.on_defaults(|| {
        with_app(|app| {
            app.hotkey_draft.set(hotkeys::Config::default());
            app.hotkeys.set_recording(-1);
            app.refresh_hotkey_labels();
            app.resume_shortcuts();
        })
    });
    app.hotkeys
        .on_save(|| with_app(|app| app.close_hotkeys(true)));
    app.hotkeys
        .on_cancel(|| with_app(|app| app.close_hotkeys(false)));
    app.hotkeys.window().on_close_requested(|| {
        with_app(|app| app.close_hotkeys(false));
        slint::CloseRequestResponse::KeepWindowShown
    });
    {
        use slint::winit_030::{EventResult, winit::event::WindowEvent};
        let mut capture = hotkeys::Capture::default();
        app.hotkeys.window().on_winit_window_event(move |_, event| {
            match event {
                WindowEvent::Focused(false) => {
                    capture.focus_lost();
                    with_app(|app| {
                        app.hotkeys.set_recording(-1);
                        app.resume_shortcuts();
                    });
                }
                WindowEvent::KeyboardInput {
                    event,
                    is_synthetic: false,
                    ..
                } => {
                    let mut swallow = false;
                    with_app(|app| {
                        let index = app.hotkeys.get_recording();
                        let recording =
                            app.hotkey_editing.get() && (0..hotkeys::ACTION_COUNT as i32).contains(&index);
                        match capture.key(
                            event.physical_key,
                            event.state,
                            event.repeat,
                            recording,
                        ) {
                            hotkeys::CaptureAction::Ignore => {}
                            hotkeys::CaptureAction::Swallow => swallow = true,
                            hotkeys::CaptureAction::Message(text) => {
                                swallow = true;
                                app.hotkeys.set_message(text.into());
                            }
                            hotkeys::CaptureAction::Bound(binding) => {
                                swallow = true;
                                let one = binding.is_single();
                                let mut config = app.hotkey_draft.get();
                                config.0[index as usize] = Some(binding);
                                match config.validate() {
                                    Ok(()) => {
                                        capture.commit();
                                        app.hotkey_draft.set(config);
                                        app.hotkeys.set_recording(-1);
                                        app.refresh_hotkey_labels();
                                        app.resume_shortcuts();
                                        app.hotkeys.set_message(
                                            if one {
                                                language::text(
                                                    "Клавиша записана. Нажмите «Сохранить», чтобы применить.",
                                                    "Key recorded. Select Save to apply it.",
                                                )
                                            } else {
                                                language::text(
                                                    "Сочетание записано. Нажмите «Сохранить», чтобы применить.",
                                                    "Shortcut recorded. Select Save to apply it.",
                                                )
                                            }
                                            .into(),
                                        );
                                    }
                                    Err(error) => {
                                        capture.abort_combo();
                                        app.hotkeys.set_message(error.into());
                                    }
                                }
                            }
                        }
                    });
                    if swallow {
                        return EventResult::PreventDefault;
                    }
                }
                _ => {}
            }
            EventResult::Propagate
        });
    }
    app.panel.on_toggle(|| with_app(|app| app.toggle()));
    app.panel.on_interaction(|| {
        with_app(|app| {
            if app.leaving.get() && !app.disconnecting.get() && app.panel.get_hovered() {
                app.notice_timer.stop();
                app.panel.set_notice(SharedString::default());
                app.show(false);
            }
            app.arm_hide();
        })
    });
    app.panel.on_set_volume(|value| {
        with_app(|app| {
            app.send(Operation::SetVolume(
                value.clamp(0, i32::from(protocol::VOLUME_WRITE_MAX)) as u8,
            ))
        })
    });
    app.settings.on_adjust(|setting, direction| {
        if let Some(setting) = protocol::Setting::from_index(setting) {
            with_app(|app| app.send(Operation::Adjust(setting, direction)));
        }
    });
    app.settings
        .on_interaction(|| with_app(|app| app.arm_hide()));
    app.panel.on_quit(|| with_app(|app| app.quit(true)));
    app.panel
        .on_preferences(|| with_app(|app| app.open_preferences()));
    app.panel.on_about(|| with_app(|app| app.open_about()));
    app.panel.window().on_close_requested(|| {
        with_app(|app| app.hide());
        slint::CloseRequestResponse::KeepWindowShown
    });
    app.tray.on_open_panel(|| with_app(|app| app.show(true)));
    app.tray.on_quit(|| with_app(|app| app.quit(true)));
    app.tray
        .on_preferences(|| with_app(|app| app.open_preferences()));
    app.tray.on_about(|| with_app(|app| app.open_about()));
    app.tray.set_startup_status(startup::label().into());
    app.preferences
        .on_hotkeys(|| with_app(|app| app.open_hotkeys()));
    app.preferences
        .on_toggle_startup(|| with_app(|app| app.toggle_startup()));
    app.preferences
        .on_toggle_language(|| with_app(|app| app.toggle_language()));
    #[cfg(windows)]
    app.preferences
        .on_configure_access(winusb_setup::configure_again);
    app.preferences.on_dismiss(|| {
        with_app(|app| {
            let _ = app.preferences.hide();
        })
    });
    app.preferences.window().on_close_requested(|| {
        with_app(|app| {
            let _ = app.preferences.hide();
        });
        slint::CloseRequestResponse::KeepWindowShown
    });
    if app.demo {
        app.panel.set_connected(true);
        app.panel.set_volume(25);
        app.settings.set_values(ModelRc::new(VecModel::from(
            [
                "HIGH", "SLOW", "CENTER", "DAC", "0", "20S", "0°", "1M", "FONT 1",
            ]
            .map(SharedString::from)
            .to_vec(),
        )));
        app.tray.set_status(
            language::text(
                "ONIX: демонстрация интерфейса, USB отключён",
                "ONIX: interface demo, USB disabled",
            )
            .into(),
        );
    }
    if !demo && !initially_present {
        app.tray.set_status(language::xi1_disconnected().into());
    }
    if !demo && let Err(error) = loaded_hotkeys {
        app.tray
            .set_shortcuts(format!("{}: {error}", language::hotkeys_disabled()).into());
    }
    if gui.open_hotkeys {
        app.open_hotkeys();
    } else if gui.open_about {
        app.open_about();
    } else if !gui.background {
        app.show(true);
    }
    Timer::single_shot(Duration::from_millis(0), || {
        with_app(|app| {
            let _ = app.tray.show();
            crate::instance::debug_log("tray shown after event loop");
        });
    });
    crate::instance::debug_log("event loop start");
    let result = slint::run_event_loop_until_quit();
    crate::instance::debug_log(&format!("event loop end ok={}", result.is_ok()));
    drop(server);
    APP.with(|slot| *slot.borrow_mut() = None);
    drop(app);
    drop(_instance);
    result?;
    Ok(())
}
