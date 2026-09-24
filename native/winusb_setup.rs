//! One-time Windows step: bind inbox WinUSB to XI1 MI_02 only.
//! Does not touch the audio interface (MI_00).

use crate::language;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

static INSTALLING: AtomicBool = AtomicBool::new(false);

fn skipped_path() -> Option<PathBuf> {
    crate::instance::data_dir()
        .ok()
        .map(|dir| dir.join("winusb-setup.skipped"))
}

fn mark_skipped() {
    if let Some(path) = skipped_path() {
        let _ = std::fs::write(path, b"1");
    }
}

fn was_skipped() -> bool {
    skipped_path().is_some_and(|path| path.is_file())
}

pub fn ready() -> bool {
    mi02_service().is_some_and(|service| service.eq_ignore_ascii_case("WinUSB"))
}

pub fn needed() -> bool {
    crate::usb::present() == Ok(true) && !ready()
}

fn mi02_service() -> Option<String> {
    use windows_sys::Win32::{
        Devices::DeviceAndDriverInstallation::{
            DIGCF_ALLCLASSES, DIGCF_PRESENT, SP_DEVINFO_DATA, SPDRP_SERVICE,
            SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
            SetupDiGetDeviceInstanceIdW, SetupDiGetDeviceRegistryPropertyW,
        },
        Foundation::{GetLastError, INVALID_HANDLE_VALUE},
    };
    let enumerator: Vec<u16> = "USB\0".encode_utf16().collect();
    unsafe {
        let set = SetupDiGetClassDevsW(
            std::ptr::null(),
            enumerator.as_ptr(),
            std::ptr::null_mut(),
            DIGCF_ALLCLASSES | DIGCF_PRESENT,
        );
        if set == INVALID_HANDLE_VALUE as isize || set == 0 {
            let _ = GetLastError();
            return None;
        }
        let mut info = SP_DEVINFO_DATA {
            cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };
        let mut index = 0;
        let mut found = None;
        while SetupDiEnumDeviceInfo(set, index, &mut info) != 0 {
            index += 1;
            let mut id = [0u16; 512];
            if SetupDiGetDeviceInstanceIdW(
                set,
                &info,
                id.as_mut_ptr(),
                id.len() as u32,
                std::ptr::null_mut(),
            ) == 0
            {
                continue;
            }
            let id = String::from_utf16_lossy(&id)
                .trim_end_matches('\0')
                .to_ascii_uppercase();
            if !id.contains("VID_26B6&PID_60C0&MI_02") {
                continue;
            }
            let mut service = [0u16; 256];
            let mut kind = 0u32;
            if SetupDiGetDeviceRegistryPropertyW(
                set,
                &info,
                SPDRP_SERVICE,
                &mut kind,
                service.as_mut_ptr().cast(),
                (service.len() * 2) as u32,
                std::ptr::null_mut(),
            ) == 0
            {
                found = Some(String::new());
                break;
            }
            found = Some(
                String::from_utf16_lossy(&service)
                    .trim_end_matches('\0')
                    .to_string(),
            );
            break;
        }
        SetupDiDestroyDeviceInfoList(set);
        found
    }
}

fn is_admin() -> bool {
    unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn message(text: &str, flags: u32) -> i32 {
    let text = wide(text);
    let title = wide("ONIX DAC Control");
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            flags,
        )
    }
}

fn elevate(quiet: bool) -> Result<i32, String> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject},
        UI::{
            Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW},
            WindowsAndMessaging::SW_SHOW,
        },
    };
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let file = wide(&exe.to_string_lossy());
    let verb = wide("runas");
    let parameters = wide(if quiet {
        "--install-driver-quiet"
    } else {
        "--install-driver"
    });
    let mut info = unsafe { std::mem::zeroed::<SHELLEXECUTEINFOW>() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = parameters.as_ptr();
    info.nShow = SW_SHOW;
    let ok = unsafe { ShellExecuteExW(&mut info) };
    if ok == 0 || info.hProcess.is_null() {
        return Err(
            language::text("Настройка доступа отменена", "Access setup was cancelled").into(),
        );
    }
    unsafe {
        if WaitForSingleObject(info.hProcess, INFINITE) != WAIT_OBJECT_0 {
            CloseHandle(info.hProcess);
            return Err(language::text(
                "Не удалось завершить настройку доступа",
                "Could not complete access setup",
            )
            .into());
        }
        let mut code = 1u32;
        GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
        Ok(code as i32)
    }
}

#[cfg(target_env = "msvc")]
fn install_now() -> Result<(), String> {
    use wdi_rs::{
        CreateListOptions, InstallDriverOptions, PrepareDriverOptions, create_list, install_driver,
        prepare_driver,
    };
    if ready() {
        return Ok(());
    }
    crate::instance::debug_log("winusb install begin");
    let devices = create_list(CreateListOptions {
        list_all: true,
        list_hubs: false,
        trim_whitespaces: true,
    })
    .map_err(|error| error.to_string())?;
    let device = devices
        .iter()
        .find(|device| {
            device.vid == crate::protocol::VID
                && device.pid == crate::protocol::PID
                && device.is_composite
                && device.mi == 2
        })
        .ok_or_else(|| {
            language::text(
                "Подключите ONIX XI1 и повторите настройку доступа",
                "Connect the ONIX XI1 and configure access again",
            )
            .to_string()
        })?;
    if device
        .driver
        .as_deref()
        .is_some_and(|driver| driver.eq_ignore_ascii_case("WinUSB"))
    {
        return Ok(());
    }
    let dir = std::env::temp_dir().join(format!("onix-xi1-winusb-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir
        .to_str()
        .ok_or_else(|| "Invalid temporary directory".to_string())?;
    let inf = "onix-xi1-winusb.inf";
    let prepare = PrepareDriverOptions {
        vendor_name: Some("ONIX DAC Control".to_string()),
        device_guid: Some("{ABA7FCDE-E61E-4124-A8E6-63771CF86C65}".to_string()),
        cert_subject: Some("ONIX XI1 WinUSB (libwdi generated)".to_string()),
        ..Default::default()
    };
    let result = prepare_driver(&device, path, inf, &prepare)
        .and_then(|()| install_driver(&device, path, inf, &InstallDriverOptions::default()))
        .map_err(|error| error.to_string());
    let _ = std::fs::remove_dir_all(&dir);
    if let Err(error) = result {
        crate::instance::debug_log(&format!("winusb install failed: {error}"));
        return Err(language::text(
            "Windows не удалось настроить доступ к управлению ONIX XI1",
            "Windows could not configure access to ONIX XI1 controls",
        )
        .into());
    }
    crate::instance::debug_log("winusb install ok (libwdi)");
    Ok(())
}

#[cfg(not(target_env = "msvc"))]
fn install_now() -> Result<(), String> {
    Err(language::text(
        "Настройка доступа доступна в официальной Windows-сборке",
        "Access setup is available in the official Windows build",
    )
    .into())
}

pub fn install_cli(quiet: bool) -> Result<(), Box<dyn std::error::Error>> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONINFORMATION, MB_ICONWARNING, MB_OK};
    if ready() {
        return Ok(());
    }
    if !is_admin() {
        if quiet {
            return Err(language::text(
                "Для настройки доступа нужны права администратора",
                "Administrator permission is required to configure access",
            )
            .into());
        }
        match elevate(false) {
            Ok(0) => return Ok(()),
            Ok(_) => {
                return Err(language::text(
                    "Не удалось настроить доступ к ONIX XI1",
                    "Could not configure access to the ONIX XI1",
                )
                .into());
            }
            Err(error) => return Err(error.into()),
        }
    }
    match install_now() {
        Ok(()) => {
            if !quiet {
                message(
                    language::text(
                        "Доступ к управлению ONIX XI1 настроен. Аудиодрайвер не изменён.",
                        "ONIX XI1 control access is configured. The audio driver was not changed.",
                    ),
                    MB_OK | MB_ICONINFORMATION,
                );
            }
            Ok(())
        }
        Err(error) => {
            if !quiet {
                message(&error, MB_OK | MB_ICONWARNING);
            }
            Err(error.into())
        }
    }
}

pub fn offer_if_needed() {
    offer(false);
}

pub fn configure_again() {
    offer(true);
}

fn offer(force: bool) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{IDYES, MB_ICONINFORMATION, MB_OK, MB_YESNO};
    if INSTALLING.load(Ordering::Acquire) {
        return;
    }
    if ready() {
        return;
    }
    if force && crate::usb::present() != Ok(true) {
        message(
            language::text(
                "Подключите ONIX XI1 и повторите настройку доступа",
                "Connect the ONIX XI1 and configure access again",
            ),
            MB_OK | MB_ICONINFORMATION,
        );
        return;
    }
    if !force && (!needed() || was_skipped()) {
        return;
    }
    let answer = message(
        language::text(
            "Для управления ONIX XI1 приложению нужен доступ к отдельному USB-интерфейсу устройства. Windows установит стандартный драйвер WinUSB только для этого интерфейса; аудиодрайвер и воспроизведение звука не изменятся.\n\nНастроить доступ сейчас? Windows запросит права администратора.",
            "To control the ONIX XI1, the app needs access to a separate USB interface on the device. Windows will install its standard WinUSB driver for that interface only; the audio driver and sound playback will not change.\n\nConfigure access now? Windows will request administrator permission.",
        ),
        MB_YESNO | MB_ICONINFORMATION,
    );
    if answer != IDYES {
        mark_skipped();
        return;
    }
    if INSTALLING.swap(true, Ordering::AcqRel) {
        return;
    }
    std::thread::spawn(|| {
        let result = if is_admin() {
            install_now().map(|()| 0)
        } else {
            elevate(true)
        };
        let _ = slint::invoke_from_event_loop(move || finish_offer(result));
    });
}

fn finish_offer(result: Result<i32, String>) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONINFORMATION, MB_ICONWARNING, MB_OK};
    INSTALLING.store(false, Ordering::Release);
    match result {
        Ok(0) => {
            if let Some(path) = skipped_path() {
                let _ = std::fs::remove_file(path);
            }
            crate::with_app(|app| app.preferences.set_device_access_visible(!ready()));
            message(
                language::text(
                    "Доступ к управлению ONIX XI1 настроен. Аудиодрайвер не изменён.",
                    "ONIX XI1 control access is configured. The audio driver was not changed.",
                ),
                MB_OK | MB_ICONINFORMATION,
            );
        }
        Ok(_) | Err(_) => {
            mark_skipped();
            message(
                language::text(
                    "Не удалось настроить доступ к ONIX XI1",
                    "Could not configure access to the ONIX XI1",
                ),
                MB_OK | MB_ICONWARNING,
            );
        }
    }
}
