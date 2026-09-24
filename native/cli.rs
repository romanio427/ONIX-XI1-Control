//! Process modes that never construct the flyout.

use crate::{instance, ipc, protocol, startup, usb};

pub struct Gui {
    pub demo: bool,
    pub background: bool,
    pub open_hotkeys: bool,
    pub open_about: bool,
}

pub fn signal_running(args: &[String]) -> bool {
    if ipc::ping().is_err() {
        return false;
    }
    if args.iter().any(|s| s == "--hotkeys") {
        ipc::notify(ipc::Command::Hotkeys).is_ok()
    } else if args.iter().any(|s| s == "--about") {
        ipc::notify(ipc::Command::About).is_ok()
    } else if !args
        .iter()
        .any(|s| s == "--background" || s == "--watch-device")
        && !instance::is_watcher_exe()
    {
        ipc::notify(ipc::Command::Show).is_ok()
    } else {
        true
    }
}

/// `Ok(None)` means this process is done (probe, second instance, …).
pub fn prepare_gui(args: &[String]) -> Result<Option<Gui>, Box<dyn std::error::Error>> {
    instance::set_process_label();
    if let Some(language) = args
        .windows(2)
        .find(|pair| pair[0] == "--init-language")
        .map(|pair| pair[1].as_str())
    {
        crate::language::initialize_from_installer(language)?;
        return Ok(None);
    }
    let background = args
        .iter()
        .any(|s| s == "--background" || s == "--watch-device")
        || instance::is_watcher_exe();
    if args.iter().any(|s| s == "--install-driver") {
        #[cfg(windows)]
        crate::winusb_setup::install_cli(false)?;
        return Ok(None);
    }
    if args.iter().any(|s| s == "--install-driver-quiet") {
        #[cfg(windows)]
        crate::winusb_setup::install_cli(true)?;
        return Ok(None);
    }
    if args.iter().any(|s| s == "--device-connected") && usb::present() != Ok(true) {
        return Ok(None);
    }
    if args.iter().any(|s| s == "--device-presence") {
        std::process::exit(match usb::present() {
            Ok(true) => 0,
            Ok(false) => 2,
            Err(_) => 3,
        });
    }
    if args.iter().any(|s| s == "--probe") {
        probe()?;
        return Ok(None);
    }
    if let Some(value) = args
        .windows(2)
        .find(|pair| pair[0] == "--set-volume")
        .and_then(|pair| pair[1].parse::<u8>().ok())
    {
        set_volume(value)?;
        return Ok(None);
    }
    if args.iter().any(|s| s == "--enable-startup") {
        startup::set_enabled(true)?;
        return Ok(None);
    }
    if args.iter().any(|s| s == "--disable-startup") {
        startup::set_enabled(false)?;
        return Ok(None);
    }
    if background && !startup::enabled() {
        return Ok(None);
    }
    let demo = args.iter().any(|s| s == "--demo");
    if !demo && signal_running(args) {
        return Ok(None);
    }
    Ok(Some(Gui {
        demo,
        // Accept the old autostart argument during upgrades.
        background,
        open_hotkeys: args.iter().any(|s| s == "--hotkeys"),
        open_about: args.iter().any(|s| s == "--about"),
    }))
}

fn probe() -> Result<(), Box<dyn std::error::Error>> {
    let _instance = instance::Instance::acquire()?;
    match usb::Device::open() {
        Ok(device) => {
            println!("{:?}", device.settings);
            Ok(())
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}

fn set_volume(value: u8) -> Result<(), Box<dyn std::error::Error>> {
    let _instance = instance::Instance::acquire()?;
    match usb::Device::open() {
        Ok(mut device) => match device.set(
            protocol::COMMAND_VOLUME,
            value.min(protocol::VOLUME_WRITE_MAX),
        ) {
            Ok(settings) => {
                println!("{:?}", settings);
                Ok(())
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(2);
            }
        },
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
