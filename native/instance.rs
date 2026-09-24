//! Process layout, data directory, single-instance lock, and debug.log.
//! Session owns USB; packaging owns the binary and this module only locates it.

use std::path::PathBuf;

pub fn data_dir() -> std::io::Result<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base =
        std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Application Support"));
    #[cfg(all(unix, not(target_os = "macos")))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")));
    let path = base
        .ok_or_else(|| {
            std::io::Error::other(crate::language::text(
                "Нет пользовательского каталога",
                "User data directory unavailable",
            ))
        })?
        .join("ONIX DAC Control");
    #[cfg(windows)]
    if portable() && !path.exists() {
        // Older portable builds kept user data next to the downloaded EXE.
        // Move it out of the download folder without overwriting existing data.
        let legacy = program_dir()?.join("data");
        if legacy.is_dir() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let _ = std::fs::rename(legacy, &path);
        }
    }
    if let Some(parent) = path.parent() {
        for name in ["ONIX XI1 PC Control", "ONIX XI1 Remote", "ONIX XI1 Control"] {
            let legacy = parent.join(name);
            if legacy.exists() && !path.exists() {
                let _ = std::fs::rename(&legacy, &path);
                break;
            }
        }
    }
    std::fs::create_dir_all(&path)?;
    #[cfg(windows)]
    if !portable() {
        // Versions before the packaged layout copied executable files into the
        // data directory. They are never used now; remove them once unlocked.
        for legacy in ["onix-xi1-watch.exe", "onix-xi1-watch.tmp", "control.path"] {
            let _ = std::fs::remove_file(path.join(legacy));
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(path)
}

#[cfg(windows)]
fn program_dir() -> std::io::Result<PathBuf> {
    std::env::current_exe()?
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| std::io::Error::other("ONIX executable directory is unavailable"))
}

#[cfg(windows)]
pub fn portable() -> bool {
    cfg!(feature = "portable")
        // Keep manually upgraded legacy portable builds recognizable.
        || program_dir().is_ok_and(|dir| dir.join("portable.flag").is_file())
}

pub fn is_watcher_exe() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_stem().map(|stem| {
                stem.to_string_lossy()
                    .eq_ignore_ascii_case("onix-xi1-watch")
            })
        })
        .unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
pub fn control_exe() -> std::io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("ONIX_CONTROL_EXE") {
        return Ok(PathBuf::from(path));
    }
    if !is_watcher_exe() {
        return std::env::current_exe();
    }
    #[cfg(windows)]
    {
        let control = program_dir()?.join("onix-xi1-pc.exe");
        if control.is_file() {
            return Ok(control);
        }
    }
    Err(std::io::Error::other(
        "ONIX PC Control executable is not next to the watcher",
    ))
}

pub fn set_process_label() {
    let name = "ONIX DAC Control";
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadDescription};
        let wide: Vec<u16> = format!("{name}\0").encode_utf16().collect();
        let _ = SetThreadDescription(GetCurrentThread(), wide.as_ptr());
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::fs::write("/proc/self/comm", "onix-xi1-pc");
    }
    #[cfg(target_os = "macos")]
    unsafe {
        let label = std::ffi::CString::new("onix-xi1-pc");
        if let Ok(label) = label {
            unsafe extern "C" {
                fn pthread_setname_np(name: *const i8) -> i32;
            }
            let _ = pthread_setname_np(label.as_ptr());
        }
    }
    let _ = name;
}

pub fn debug_log(event: &str) {
    let Ok(dir) = data_dir() else {
        return;
    };
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("debug.log"))
    else {
        return;
    };
    use std::io::Write;
    let _ = writeln!(file, "{} {event}", now_stamp());
}

fn now_stamp() -> String {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}.{:03}", elapsed.as_secs(), elapsed.subsec_millis())
}

fn already_running() -> Box<dyn std::error::Error> {
    crate::language::text(
        "ONIX уже запущен; две версии не должны управлять ЦАПом одновременно",
        "ONIX is already running; two copies must not control the DAC at once",
    )
    .into()
}

pub struct Instance {
    #[cfg(windows)]
    mutex: windows_sys::Win32::Foundation::HANDLE,
    #[cfg(not(windows))]
    file: std::fs::File,
}

impl Instance {
    pub fn acquire() -> Result<Self, Box<dyn std::error::Error>> {
        Self::try_acquire()?.ok_or_else(already_running)
    }

    pub fn try_acquire() -> Result<Option<Self>, Box<dyn std::error::Error>> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::{
                Foundation::{
                    CloseHandle, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, GetLastError,
                    SetLastError,
                },
                System::Threading::CreateMutexW,
            };
            // Local\ = this logon session. Existence of the named kernel object is
            // the lock; bInitialOwner is FALSE so CreateMutex is only create-or-open
            // (MSDN CreateMutexW).
            let name: Vec<u16> = "Local\\ONIX_XI1_Control_SingleInstance\0"
                .encode_utf16()
                .collect();
            unsafe { SetLastError(0) };
            let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
            let status = unsafe { GetLastError() };
            if mutex.is_null() {
                if status == ERROR_ACCESS_DENIED {
                    return Ok(None);
                }
                return Err(std::io::Error::from_raw_os_error(status as i32).into());
            }
            if status == ERROR_ALREADY_EXISTS {
                unsafe {
                    CloseHandle(mutex);
                }
                return Ok(None);
            }
            Ok(Some(Self { mutex }))
        }
        #[cfg(not(windows))]
        {
            use std::fs::OpenOptions;
            let path = data_dir()?.join("application.lock");
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(path)?;
            match file.try_lock() {
                Ok(()) => Ok(Some(Self { file })),
                Err(std::fs::TryLockError::WouldBlock) => Ok(None),
                Err(std::fs::TryLockError::Error(error)) => Err(error.into()),
            }
        }
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.mutex);
        }
        #[cfg(not(windows))]
        {
            let _ = self.file.unlock();
        }
    }
}
