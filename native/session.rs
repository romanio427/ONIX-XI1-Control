use crate::{
    protocol::Settings,
    usb::{self, Device, Failure, Inputs},
};
use futures_lite::{
    StreamExt,
    future::{block_on, or, pending},
};
use nusb::{DeviceId, hotplug::HotplugEvent};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

pub static CONNECTED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
pub enum Operation {
    SetVolume(u8),
    Adjust(crate::protocol::Setting, i32),
    Cycle(crate::protocol::Setting, i32),
}
struct Request {
    operation: Operation,
    generation: u64,
    revision: u64,
    created: Instant,
}
pub struct Reply {
    pub settings: Option<Settings>,
    pub error: String,
    pub winusb_required: bool,
    pub announce_change: bool,
    pub removed: bool,
    pub revision: u64,
}
pub struct Session {
    sender: async_channel::Sender<Request>,
    generation: Arc<AtomicU64>,
    stop: async_channel::Sender<()>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Session {
    pub fn start(publish: impl Fn(Reply) + Send + 'static) -> Self {
        let (sender, receiver) = async_channel::bounded(32);
        let (stop, stopping) = async_channel::bounded(1);
        let generation = Arc::new(AtomicU64::new(0));
        let gen_worker = generation.clone();
        let worker = thread::spawn(move || {
            crate::instance::debug_log("session worker start");
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                block_on(run(receiver, stopping, gen_worker, move |reply| {
                    if reply.removed {
                        CONNECTED.store(false, Ordering::Release);
                    } else if reply.settings.is_some() {
                        CONNECTED.store(true, Ordering::Release);
                    }
                    publish(reply);
                }))
            }));
            match result {
                Ok(()) => crate::instance::debug_log("session worker end"),
                Err(_) => crate::instance::debug_log("session worker panic"),
            }
        });
        Self {
            sender,
            generation,
            stop,
            worker: Some(worker),
        }
    }
    pub fn send(&self, operation: Operation, revision: u64) -> bool {
        self.sender
            .try_send(Request {
                operation,
                generation: self.generation.load(Ordering::Acquire),
                revision,
                created: Instant::now(),
            })
            .is_ok()
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.stop.close();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
// This is one event value in one USB worker. Boxing the rare hotplug event
// would add allocation and indirection without reducing retained memory.
#[allow(clippy::large_enum_variant)]
enum Event {
    Stop,
    Request(Result<Request, async_channel::RecvError>),
    Usb(Option<HotplugEvent>),
    Input(Result<bool, Failure>),
    Deadline,
}

// One recovery episode: three retries, increasing delays, ten seconds total.
// A healthy connection earns a fresh budget on its NEXT failure, not on each
// successful reopen. No timer runs to check whether it is healthy.
const RECOVERY_WINDOW: Duration = Duration::from_secs(10);
struct Recovery {
    until: Instant,
    retries: usize,
}
impl Recovery {
    fn new() -> Self {
        Self {
            until: Instant::now() + RECOVERY_WINDOW,
            retries: 0,
        }
    }

    fn retry(&mut self) -> Option<Instant> {
        let delay = [500, 1000, 2000].get(self.retries).copied()?;
        let next = Instant::now() + Duration::from_millis(delay);
        if next >= self.until {
            return None;
        }
        self.retries += 1;
        Some(next)
    }
}
fn dac_gone() -> Failure {
    Failure::from(crate::language::dac_disconnected())
}

async fn run(
    receiver: async_channel::Receiver<Request>,
    stop: async_channel::Receiver<()>,
    generation: Arc<AtomicU64>,
    publish: impl Fn(Reply),
) {
    // USB owner and the only device watcher in this process.
    // nusb: watch_devices first, then list, so a device that appears between
    // the two is not missed. Disconnected is DeviceId-only.
    let (mut watch, mut watch_retry_at) = match nusb::watch_devices() {
        Ok(watch) => (Some(watch), None),
        Err(error) => {
            publish(Reply {
                settings: None,
                error: error.to_string(),
                winusb_required: false,
                announce_change: false,
                removed: false,
                revision: 0,
            });
            (None, Some(Instant::now() + Duration::from_secs(1)))
        }
    };
    let mut watch_backoff = Duration::from_secs(1);
    let mut device: Option<Device> = None;
    let mut inputs: Option<Inputs> = None;
    let mut slider: Option<Request> = None;
    let mut slider_at: Option<Instant> = None;
    let mut next_volume_write = Instant::now();
    let mut volume_guard_until = Instant::now();
    let mut refresh_at: Option<Instant> = None;
    let mut retry_at = if usb::present() == Ok(true) {
        Some(Instant::now() + usb::SETTLE)
    } else {
        None
    };
    let mut confirm_absent_at: Option<Instant> = None;
    let mut tracked: Option<DeviceId> = None;
    let mut recovery = Recovery::new();
    let mut healthy_since: Option<Instant> = None;
    let mut revision = 0;
    let mut was_connected = false;
    let mut hardware_refresh = false;
    loop {
        let deadline = [
            watch_retry_at,
            retry_at,
            confirm_absent_at,
            refresh_at.filter(|_| slider.is_none()),
            slider_at,
        ]
        .into_iter()
        .flatten()
        .min();
        // At rest there is NO timer. Deadlines are only for an actual input,
        // slider coalescing, composite settle, or a bounded recovery attempt.
        let event = or(
            async {
                let _ = stop.recv().await;
                Event::Stop
            },
            or(
                async {
                    match watch.as_mut() {
                        Some(watch) => Event::Usb(watch.next().await),
                        None => pending().await,
                    }
                },
                or(
                    async {
                        if let Some(at) = deadline {
                            async_io::Timer::at(at).await;
                        } else {
                            pending::<()>().await;
                        }
                        Event::Deadline
                    },
                    or(async { Event::Request(receiver.recv().await) }, async {
                        match inputs.as_mut() {
                            Some(input) => Event::Input(input.next().await),
                            None => pending().await,
                        }
                    }),
                ),
            ),
        )
        .await;
        let mut failure = None;
        let mut removed = false;
        let mut operation = None;
        let mut execute = false;
        match event {
            Event::Stop | Event::Request(Err(_)) => break,
            Event::Usb(None) => {
                watch = None;
                watch_retry_at = Some(Instant::now() + watch_backoff);
                continue;
            }
            Event::Usb(Some(HotplugEvent::Connected(info))) => {
                if usb::is_xi1(&info) {
                    tracked = Some(info.id());
                    confirm_absent_at = None;
                    if device.is_none() {
                        recovery = Recovery::new();
                        retry_at = Some(Instant::now() + usb::SETTLE);
                    }
                }
            }
            Event::Usb(Some(HotplugEvent::Disconnected(id))) => {
                let ours = tracked == Some(id) || device.as_ref().is_some_and(|d| d.id == id);
                if !ours {
                    continue;
                }
                confirm_absent_at = Some(Instant::now() + usb::SETTLE);
                retry_at = confirm_absent_at;
                if device.is_some() {
                    failure = Some(Failure::temporary(crate::language::dac_disconnected()));
                }
            }
            Event::Input(Ok(true)) => {
                hardware_refresh = true;
                refresh_at.get_or_insert(Instant::now() + Duration::from_millis(20));
            }
            Event::Input(Ok(false)) => {}
            Event::Input(Err(error)) => {
                // Cancelled interrupt IN is not unplug. Dropping WinUSB MI_02
                // makes list_devices flicker and used to quit the whole app.
                if error.retryable {
                    if let Some(current) = device.as_mut() {
                        crate::instance::debug_log(&format!(
                            "hid input lost, keeping control: {error}"
                        ));
                        inputs = None;
                        match current.inputs().await {
                            Ok(next) => {
                                inputs = Some(next);
                                crate::instance::debug_log("hid inputs reopened");
                            }
                            Err(reopen) => {
                                crate::instance::debug_log(&format!("hid reopen failed: {reopen}"));
                                failure = Some(reopen);
                            }
                        }
                    } else if was_connected {
                        confirm_absent_at.get_or_insert(Instant::now() + usb::SETTLE);
                        retry_at = confirm_absent_at;
                        failure = Some(error);
                    }
                } else {
                    if was_connected {
                        confirm_absent_at.get_or_insert(Instant::now() + usb::SETTLE);
                        retry_at = confirm_absent_at;
                    }
                    failure = Some(error);
                }
            }
            Event::Request(Ok(request)) => {
                revision = revision.max(request.revision);
                if request.generation != generation.load(Ordering::Acquire)
                    || request.created.elapsed() > Duration::from_secs(1)
                {
                    if device.is_some() {
                        refresh_at.get_or_insert(Instant::now());
                    }
                    continue;
                }
                if device.is_none() {
                    recovery = Recovery::new();
                    retry_at = Some(Instant::now() + usb::SETTLE);
                    continue;
                }
                if matches!(request.operation, Operation::SetVolume(_)) {
                    slider = Some(request);
                    slider_at.get_or_insert(next_volume_write.max(Instant::now()));
                    continue;
                }
                slider = None;
                operation = Some(request.operation);
                execute = true;
            }
            Event::Deadline => {
                if watch_retry_at.is_some_and(|at| at <= Instant::now()) {
                    match nusb::watch_devices() {
                        Ok(next) => {
                            watch = Some(next);
                            watch_retry_at = None;
                            watch_backoff = Duration::from_secs(1);
                            if device.is_none() {
                                retry_at = Some(Instant::now() + usb::SETTLE);
                            }
                        }
                        Err(_) => {
                            watch_backoff =
                                watch_backoff.saturating_mul(2).min(Duration::from_secs(60));
                            watch_retry_at = Some(Instant::now() + watch_backoff);
                        }
                    }
                }
                if confirm_absent_at.is_some_and(|at| at <= Instant::now()) {
                    confirm_absent_at = None;
                    if usb::present() == Ok(false) {
                        if was_connected {
                            if Instant::now() < volume_guard_until {
                                crate::instance::debug_log(
                                    "confirm_absent ignored; volume write still in guard",
                                );
                                retry_at = Some(Instant::now() + usb::SETTLE);
                                continue;
                            }
                            removed = true;
                            failure = Some(dac_gone());
                        }
                        retry_at = None;
                        if !removed {
                            continue;
                        }
                    }
                }
                if device.is_none() && !removed {
                    retry_at = None;
                    if usb::present() != Ok(true) {
                        continue;
                    }
                    if Instant::now() >= recovery.until {
                        recovery = Recovery::new();
                    }
                    let open_until = recovery.until.min(Instant::now() + Duration::from_secs(5));
                    let opened = or(
                        async {
                            let _ = stop.recv().await;
                            Err(Failure::from(crate::language::text(
                                "отменено",
                                "cancelled",
                            )))
                        },
                        or(
                            async {
                                crate::instance::debug_log("usb open begin");
                                let mut current = Device::open()?;
                                crate::instance::debug_log("usb open ok");
                                let input = current.inputs().await?;
                                crate::instance::debug_log("usb inputs ok");
                                current.refresh()?;
                                crate::instance::debug_log("usb refresh ok");
                                Ok((current, input))
                            },
                            async {
                                async_io::Timer::at(open_until).await;
                                Err(Failure::temporary(crate::language::text(
                                    "Не удалось открыть уведомления ЦАПа вовремя",
                                    "Timed out while opening DAC notifications",
                                )))
                            },
                        ),
                    )
                    .await;
                    if stop.is_closed() {
                        break;
                    }
                    match opened {
                        Ok((current, input)) => {
                            tracked = Some(current.id);
                            crate::instance::debug_log("usb connected, publishing");
                            publish(Reply {
                                settings: Some(current.settings),
                                error: String::new(),
                                winusb_required: false,
                                announce_change: false,
                                removed: false,
                                revision,
                            });
                            device = Some(current);
                            inputs = Some(input);
                            healthy_since = Some(Instant::now());
                            was_connected = true;
                            confirm_absent_at = None;
                        }
                        Err(error) => failure = Some(error),
                    }
                } else if device.is_some() && !removed {
                    if slider_at.is_some_and(|at| at <= Instant::now()) {
                        slider_at = None;
                        if let Some(request) = slider.take() {
                            if request.generation == generation.load(Ordering::Acquire)
                                && request.created.elapsed() <= Duration::from_secs(1)
                            {
                                operation = Some(request.operation);
                            }
                            execute = true;
                        }
                    }
                    execute |=
                        slider.is_none() && refresh_at.is_some_and(|at| at <= Instant::now());
                }
            }
        }
        if execute {
            refresh_at = None;
            let Some(current) = device.as_mut() else {
                continue;
            };
            let volume_write = matches!(operation, Some(Operation::SetVolume(_)));
            let result = (|| -> Result<Settings, Failure> {
                match operation {
                    None => current.refresh(),
                    Some(Operation::SetVolume(value)) => current.set(
                        crate::protocol::COMMAND_VOLUME,
                        value.min(crate::protocol::VOLUME_WRITE_MAX),
                    ),
                    Some(Operation::Adjust(setting, direction)) => {
                        let fresh = current.refresh()?;
                        let (command, value) = fresh.adjustment(setting, direction);
                        current.set(command, value)
                    }
                    Some(Operation::Cycle(setting, direction)) => {
                        let fresh = current.refresh()?;
                        let (command, value) = fresh.cycle(setting, direction);
                        current.set(command, value)
                    }
                }
            })();
            if volume_write {
                next_volume_write = Instant::now() + Duration::from_millis(100);
                volume_guard_until = Instant::now() + Duration::from_secs(1);
            }
            match result {
                Ok(settings) => {
                    // Physical DAC changes and successful shortcut cycles are
                    // both user-facing actions that need the same notice.
                    let announce_change = (operation.is_none() && hardware_refresh)
                        || matches!(operation, Some(Operation::Cycle(_, _)));
                    hardware_refresh = false;
                    publish(Reply {
                        settings: Some(settings),
                        error: String::new(),
                        winusb_required: false,
                        announce_change,
                        removed: false,
                        revision,
                    })
                }
                Err(error) => failure = Some(error),
            }
        }
        if let Some(error) = failure {
            if healthy_since
                .take()
                .is_some_and(|at| at.elapsed() >= RECOVERY_WINDOW)
            {
                recovery = Recovery::new();
            }
            inputs = None;
            device = None;
            slider = None;
            slider_at = None;
            refresh_at = None;
            hardware_refresh = false;
            generation.fetch_add(1, Ordering::AcqRel);
            if removed {
                confirm_absent_at = None;
                retry_at = None;
                tracked = None;
                was_connected = false;
            } else if confirm_absent_at.is_some() {
                retry_at = confirm_absent_at;
            } else if error.retryable {
                retry_at = recovery.retry();
            } else {
                retry_at = None;
            }
            let winusb_required = error.requires_winusb();
            let message = if !removed && error.retryable && retry_at.is_none() {
                format!(
                    "{error}. {}",
                    crate::language::text(
                        "Восстановление остановлено; переподключите ЦАП",
                        "Recovery stopped; reconnect the DAC"
                    )
                )
            } else {
                error.to_string()
            };
            crate::instance::debug_log(&format!(
                "session failure removed={removed} retryable={} retry={} msg={message}",
                error.retryable,
                retry_at.is_some()
            ));
            publish(Reply {
                settings: None,
                error: message,
                winusb_required,
                announce_change: false,
                removed,
                revision,
            });
        }
    }
}
