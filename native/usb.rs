use crate::protocol::{self, Settings};
use nusb::{
    DeviceInfo, MaybeFuture,
    transfer::{ControlIn, ControlOut, ControlType, Recipient},
};
use std::{thread, time::Duration};

// Coalesce Windows usbccgp parent+MI_* storms (and udev bursts). nusb also
// requires a short delay before claiming a composite interface on Windows.
pub const SETTLE: Duration = Duration::from_millis(400);

const TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Debug)]
pub struct Failure {
    message: String,
    pub retryable: bool,
    winusb_required: bool,
}

impl Failure {
    pub fn temporary(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: true,
            winusb_required: false,
        }
    }

    #[cfg(windows)]
    fn winusb(error: nusb::Error, context: &str) -> Self {
        let mut failure = open_failure(error, context);
        failure.winusb_required = true;
        failure
    }

    pub fn requires_winusb(&self) -> bool {
        self.winusb_required
    }

    fn context(mut self, context: &str) -> Self {
        self.message = format!("{context}: {}", self.message);
        self
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Failure {}
impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self {
            message,
            retryable: false,
            winusb_required: false,
        }
    }
}
impl From<&str> for Failure {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}
impl From<nusb::Error> for Failure {
    fn from(error: nusb::Error) -> Self {
        let retryable = matches!(
            error.kind(),
            nusb::ErrorKind::Busy | nusb::ErrorKind::Disconnected | nusb::ErrorKind::NotFound
        );
        Self {
            message: error.to_string(),
            retryable,
            winusb_required: false,
        }
    }
}

fn open_failure(error: nusb::Error, context: &str) -> Failure {
    // nusb: on Windows, open/claim of a composite interface can fail as
    // Other/Unsupported/NotFound until usbccgp has started the WinUSB PDO.
    let retryable = !matches!(error.kind(), nusb::ErrorKind::PermissionDenied);
    Failure {
        message: format!("{context}: {error}"),
        retryable,
        winusb_required: false,
    }
}
impl From<nusb::transfer::TransferError> for Failure {
    fn from(error: nusb::transfer::TransferError) -> Self {
        use nusb::transfer::TransferError;
        let retryable = matches!(
            error,
            TransferError::Cancelled | TransferError::Disconnected
        );
        Self {
            message: error.to_string(),
            retryable,
            winusb_required: false,
        }
    }
}
#[cfg(not(windows))]
impl From<async_hid::HidError> for Failure {
    fn from(error: async_hid::HidError) -> Self {
        use async_hid::HidError;
        let retryable = match &error {
            HidError::Disconnected | HidError::NotConnected => true,
            HidError::Other(inner) => inner.downcast_ref::<std::io::Error>().is_some_and(|e| {
                matches!(
                    e.kind(),
                    std::io::ErrorKind::Interrupted
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::WouldBlock
                )
            }),
            _ => false,
        };
        Self {
            message: error.to_string(),
            retryable,
            winusb_required: false,
        }
    }
}

// Opt-in diagnostic output only; normal launches do not create logs or timers.
fn tracing() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("ONIX_TRACE_USB").is_some())
}

#[cfg(windows)]
type ControlHandle = nusb::Interface;
#[cfg(not(windows))]
type ControlHandle = nusb::Device;

pub fn is_xi1(info: &DeviceInfo) -> bool {
    info.vendor_id() == protocol::VID && info.product_id() == protocol::PID
}

pub fn present() -> Result<bool, String> {
    nusb::list_devices()
        .wait()
        .map(|mut devices| devices.any(|d| is_xi1(&d)))
        .map_err(|e| e.to_string())
}

pub struct Device {
    control: ControlHandle,
    pub id: nusb::DeviceId,
    #[cfg(not(windows))]
    serial: Option<String>,
    pub settings: Settings,
}

impl Device {
    pub fn open() -> Result<Self, Failure> {
        let info = nusb::list_devices()
            .wait()?
            .find(|d| d.vendor_id() == protocol::VID && d.product_id() == protocol::PID)
            .ok_or_else(|| crate::language::text("Подключите ONIX XI1", "Connect ONIX XI1"))?;
        let device = info.open().wait().map_err(|e| {
            open_failure(
                e,
                crate::language::text("Не удалось открыть ЦАП", "Could not open the DAC"),
            )
        })?;
        // Windows requires a WinUSB interface handle. Linux/macOS can submit the
        // same vendor requests through endpoint zero without claiming the HID
        // interface, which would otherwise conflict with the OS keyboard driver.
        #[cfg(windows)]
        let control = device
            .claim_interface(protocol::INTERFACE)
            .wait()
            .map_err(|e| {
                Failure::winusb(
                    e,
                    crate::language::text(
                        "Интерфейс управления ЦАПом недоступен (нужен WinUSB для MI_02)",
                        "The DAC control interface is unavailable (WinUSB is required for MI_02)",
                    ),
                )
            })?;
        #[cfg(not(windows))]
        let control = device;
        let settings = Settings::parse(&Self::exchange(&control, protocol::COMMAND_SNAPSHOT, 0)?)?;
        Ok(Self {
            control,
            id: info.id(),
            #[cfg(not(windows))]
            serial: info.serial_number().map(str::to_owned),
            settings,
        })
    }

    #[cfg(windows)]
    pub async fn inputs(&self) -> Result<Inputs, Failure> {
        use nusb::transfer::{Buffer, In, Interrupt};
        let mut endpoint = self.control.endpoint::<Interrupt, In>(0x82).map_err(|e| {
            Failure::from(e).context(crate::language::text(
                "Канал уведомлений ЦАПа недоступен",
                "The DAC notification channel is unavailable",
            ))
        })?;
        // Keep reads queued while the actor performs a control exchange.
        for _ in 0..4 {
            endpoint.submit(Buffer::new(endpoint.max_packet_size()));
        }
        Ok(Inputs(endpoint))
    }

    #[cfg(not(windows))]
    pub async fn inputs(&self) -> Result<Inputs, Failure> {
        use futures_lite::StreamExt;
        let backend = async_hid::HidBackend::default();
        let mut devices = backend.enumerate().await?;
        let mut matched = None;
        while let Some(info) = devices.next().await {
            if !info.matches(1, 0x80, protocol::VID, protocol::PID) {
                continue;
            }
            if let (Some(wanted), Some(found)) = (&self.serial, &info.serial_number)
                && !found.is_empty()
                && wanted != found
            {
                continue;
            }
            if matched.is_some() {
                return Err(crate::language::text(
                    "Подключено несколько одинаковых ЦАПов: оставь один для управления",
                    "Multiple identical DACs are connected; leave one connected for control",
                )
                .into());
            }
            matched = Some(info);
        }
        let info = matched.ok_or_else(|| {
            crate::language::text(
                "HID-уведомления ЦАПа недоступны: проверьте разрешения системы",
                "DAC HID notifications are unavailable; check system permissions",
            )
        })?;
        // hidraw / IOHIDManager: read-only, without detaching/seizing the OS driver.
        let reader = info
            .open_readable()
            .await
            .map_err(|e| Failure::from(e).context(crate::language::text("HID ЦАПа", "DAC HID")))?;
        Ok(Inputs(reader))
    }

    fn exchange(control: &ControlHandle, command: u8, value: u8) -> Result<Vec<u8>, Failure> {
        if tracing() {
            eprintln!("ONIX USB command={command:02x}");
        }
        let data = protocol::packet(command, value);
        // Recipient::Other is intentional: preserve bmRequestType 0x43 / 0xc3.
        control
            .control_out(
                ControlOut {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Other,
                    request: 0xa0,
                    value: 0,
                    index: protocol::CONTROL_INDEX,
                    data: &data,
                },
                TIMEOUT,
            )
            .wait()?;
        thread::sleep(Duration::from_millis(10));
        let reply = control
            .control_in(
                ControlIn {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Other,
                    request: 0xa1,
                    value: 0,
                    index: protocol::CONTROL_INDEX,
                    length: protocol::REPLY_LEN as u16,
                },
                TIMEOUT,
            )
            .wait()?;
        if reply.len() != protocol::REPLY_LEN {
            return Err(Failure::temporary(crate::language::text(
                "Неполный ответ ЦАПа",
                "Incomplete response from the DAC",
            )));
        }
        Ok(reply)
    }

    pub fn refresh(&mut self) -> Result<Settings, Failure> {
        let bytes = Self::exchange(&self.control, protocol::COMMAND_SNAPSHOT, 0)?;
        match Settings::parse(&bytes) {
            Ok(settings) => self.settings = settings,
            Err(error) => {
                crate::instance::debug_log(&format!(
                    "kept last settings after bad snapshot: {error}"
                ));
            }
        }
        if tracing() {
            eprintln!("ONIX snapshot {:?}", self.settings);
        }
        Ok(self.settings)
    }

    pub fn set(&mut self, command: u8, value: u8) -> Result<Settings, Failure> {
        Self::exchange(&self.control, command, value)?;
        self.refresh()
    }
}

#[cfg(windows)]
pub struct Inputs(nusb::Endpoint<nusb::transfer::Interrupt, nusb::transfer::In>);

#[cfg(not(windows))]
pub struct Inputs(async_hid::DeviceReader);

#[cfg(not(windows))]
impl Inputs {
    pub async fn next(&mut self) -> Result<bool, Failure> {
        use async_hid::AsyncHidRead;
        let mut report = [0u8; 64];
        let length =
            self.0.read_input_report(&mut report).await.map_err(|e| {
                Failure::from(e).context(crate::language::text("HID ЦАПа", "DAC HID"))
            })?;
        if length == 0 {
            return Err(crate::language::text(
                "Канал уведомлений ЦАПа закрыт",
                "The DAC notification channel closed",
            )
            .into());
        }
        Ok(input_changed(&report[..length]))
    }
}

#[cfg(windows)]
impl Inputs {
    pub async fn next(&mut self) -> Result<bool, Failure> {
        let completion = self.0.next_complete().await;
        completion.status.map_err(|e| {
            Failure::from(e).context(crate::language::text(
                "Канал уведомлений ЦАПа",
                "DAC notification channel",
            ))
        })?;
        let changed = input_changed(&completion.buffer);
        self.0.submit(completion.buffer);
        Ok(changed)
    }
}

fn input_changed(report: &[u8]) -> bool {
    if tracing() {
        eprintln!("ONIX input {report:02x?}");
    }
    protocol::input_changes_settings(report)
}
