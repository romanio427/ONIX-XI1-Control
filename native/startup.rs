use std::fs;
use std::path::PathBuf;

fn preference_path() -> std::io::Result<PathBuf> {
    let dir = crate::instance::data_dir()?;
    Ok(dir.join("startup.preference"))
}

pub fn enabled() -> bool {
    match preference_path()
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
    {
        Some(value) => value.trim() == "enabled",
        None => true,
    }
}

pub fn set_enabled(enabled: bool) -> Result<(), Box<dyn std::error::Error>> {
    register(enabled)?;
    fs::write(
        preference_path()?,
        if enabled { "enabled" } else { "disabled" },
    )?;
    Ok(())
}

pub fn refresh() {
    if !enabled() {
        return;
    }
    let _ = register(true);
}

pub fn label() -> String {
    #[cfg(target_os = "macos")]
    if enabled() && !macos_approved() {
        return crate::language::text(
            "Автозапуск: разрешите в настройках входа macOS",
            "Startup: allow ONIX in macOS Login Items",
        )
        .into();
    }
    if crate::language::current() == crate::language::Language::Russian {
        format!("Автозагрузка: {}", if enabled() { "вкл" } else { "выкл" })
    } else {
        format!("Start at sign-in: {}", if enabled() { "on" } else { "off" })
    }
}

#[cfg(target_os = "macos")]
fn macos_approved() -> bool {
    use objc2_service_management::SMAppService;
    unsafe { SMAppService::mainAppService().status().0 == 1 }
}

#[cfg(windows)]
fn register(enabled: bool) -> Result<(), Box<dyn std::error::Error>> {
    use windows_sys::Win32::System::Registry::*;
    let path: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Run\0"
        .encode_utf16()
        .collect();
    let name: Vec<u16> = "ONIX DAC Control\0".encode_utf16().collect();
    let legacy_pc_control: Vec<u16> = "ONIX XI1 PC Control\0".encode_utf16().collect();
    let legacy_control: Vec<u16> = "ONIX XI1 Control\0".encode_utf16().collect();
    let legacy_remote: Vec<u16> = "ONIX XI1 Remote\0".encode_utf16().collect();
    let mut key = std::ptr::null_mut();
    unsafe {
        let result = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            path.as_ptr(),
            0,
            std::ptr::null(),
            0,
            KEY_SET_VALUE | KEY_QUERY_VALUE,
            std::ptr::null(),
            &mut key,
            std::ptr::null_mut(),
        );
        if result != 0 {
            return Err(std::io::Error::from_raw_os_error(result as i32).into());
        }
        let _ = RegDeleteValueW(key, legacy_pc_control.as_ptr());
        let _ = RegDeleteValueW(key, legacy_control.as_ptr());
        let _ = RegDeleteValueW(key, legacy_remote.as_ptr());
        let mut existing = [0u8; 4096];
        let mut existing_size = existing.len() as u32;
        let mut existing_type = 0;
        let preserve_existing = enabled
            && crate::instance::portable()
            && RegQueryValueExW(
                key,
                name.as_ptr(),
                std::ptr::null_mut(),
                &mut existing_type,
                existing.as_mut_ptr(),
                &mut existing_size,
            ) == 0
            && existing_type == REG_SZ
            && {
                let wide = existing[..existing_size as usize]
                    .chunks_exact(2)
                    .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
                    .take_while(|unit| *unit != 0)
                    .collect::<Vec<_>>();
                let command = String::from_utf16_lossy(&wide);
                command
                    .strip_prefix('"')
                    .and_then(|quoted| quoted.split_once('"'))
                    .is_some_and(|(exe, arguments)| {
                        std::path::Path::new(exe).is_file()
                            && arguments.contains("--background")
                            && !exe.eq_ignore_ascii_case(
                                &crate::instance::control_exe()
                                    .unwrap_or_default()
                                    .to_string_lossy(),
                            )
                    })
            };
        let result = if preserve_existing {
            0
        } else if enabled {
            let command: Vec<u16> = format!(
                "\"{}\" --background\0",
                crate::instance::control_exe()?.display()
            )
            .encode_utf16()
            .collect();
            RegSetValueExW(
                key,
                name.as_ptr(),
                0,
                REG_SZ,
                command.as_ptr().cast(),
                (command.len() * 2) as u32,
            )
        } else {
            RegDeleteValueW(key, name.as_ptr())
        };
        RegCloseKey(key);
        if result != 0 && !(result == 2 && !enabled) {
            return Err(std::io::Error::from_raw_os_error(result as i32).into());
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn register(enabled: bool) -> Result<(), Box<dyn std::error::Error>> {
    const SYSTEM_ENTRY: &str = "/etc/xdg/autostart/io.github.onix-xi1-control.desktop";
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
        .ok_or(crate::language::text(
            "Нет пользовательского каталога",
            "User directory is unavailable",
        ))?;
    let dir = base.join("autostart");
    fs::create_dir_all(&dir)?;
    let entry = dir.join("io.github.onix-xi1-control.desktop");
    let _ = fs::remove_file(dir.join("onix-xi1-control.desktop"));
    // Native packages own the system entry. Disabled startup exits immediately,
    // so no per-user override is left behind.
    if std::path::Path::new(SYSTEM_ENTRY).is_file() {
        let _ = fs::remove_file(entry);
        return Ok(());
    }
    let exe = crate::instance::control_exe()?
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('%', "%%");
    fs::write(
        entry,
        format!(
            "[Desktop Entry]\nType=Application\nName=ONIX DAC Control\nExec=\"{exe}\" --background\nTerminal=false\nHidden={}\n",
            !enabled
        ),
    )?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn register(enabled: bool) -> Result<(), Box<dyn std::error::Error>> {
    use objc2_foundation::NSString;
    use objc2_service_management::SMAppService;
    unsafe {
        let service = SMAppService::mainAppService();
        let status = service.status().0;
        if enabled && status != 1 && status != 2 {
            service
                .registerAndReturnError()
                .map_err(|e| e.to_string())?;
        }
        if !enabled && status != 0 {
            service
                .unregisterAndReturnError()
                .map_err(|e| e.to_string())?;
        }
        // Retain the old plist in the bundle for one upgrade cycle so that an
        // already registered Launch Agent can be removed.
        let legacy = SMAppService::agentServiceWithPlistName(&NSString::from_str(
            "io.github.onix-xi1-control.login.plist",
        ));
        if matches!(legacy.status().0, 1 | 2) {
            legacy
                .unregisterAndReturnError()
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
