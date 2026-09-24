use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use futures_lite::{
    StreamExt,
    future::{block_on, or, pending},
};
use std::{
    thread,
    time::{Duration, Instant},
};

pub struct Hook {
    stop: async_channel::Sender<()>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Hook {
    pub fn start(config: crate::hotkeys::Config) -> Self {
        let (stop, stopping) = async_channel::bounded(1);
        let worker = thread::spawn(move || {
            if let Err(error) = block_on(run(&stopping, config))
                && !stopping.is_closed()
            {
                super::status(format!(
                    "{}: {error}. {}",
                    crate::language::text("Клавиши Linux", "Linux hotkeys"),
                    crate::language::text("Доступна панель", "The panel is still available")
                ));
            }
        });
        Self {
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Hook {
    fn drop(&mut self) {
        self.stop.close();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

async fn bounded<T>(
    stop: &async_channel::Receiver<()>,
    seconds: u64,
    operation: impl std::future::Future<Output = Result<T, ashpd::Error>>,
) -> Result<T, String> {
    or(
        async {
            let _ = stop.recv().await;
            Err(crate::language::text("отменено", "cancelled").into())
        },
        or(
            async { operation.await.map_err(|e| e.to_string()) },
            async {
                async_io::Timer::after(Duration::from_secs(seconds)).await;
                Err(crate::language::text(
                    "истекло время ответа рабочего стола",
                    "desktop response timed out",
                )
                .into())
            },
        ),
    )
    .await
}

async fn run(
    stop: &async_channel::Receiver<()>,
    config: crate::hotkeys::Config,
) -> Result<(), String> {
    config.validate()?;
    if config.0.iter().all(Option::is_none) {
        super::status(crate::language::hotkeys_disabled());
        return Ok(());
    }
    let portal = bounded(stop, 10, GlobalShortcuts::new()).await?;
    bounded(stop, 10, async {
        let result = ashpd::register_host_app_with_connection(
            portal.connection().clone(),
            "io.github.onix-xi1-control".parse()?,
        )
        .await;
        match result {
            // Older portals have no host registry; retain their normal identity handling.
            Err(ashpd::Error::PortalNotFound(_)) => Ok(()),
            Err(ashpd::Error::Zbus(ashpd::zbus::Error::MethodError(name, _, _)))
                if matches!(
                    name.as_str(),
                    "org.freedesktop.DBus.Error.UnknownMethod"
                        | "org.freedesktop.DBus.Error.UnknownInterface"
                ) =>
            {
                Ok(())
            }
            other => other,
        }
    })
    .await?;
    let session = bounded(stop, 10, portal.create_session(Default::default())).await?;
    let result = async {
        // Subscribe before binding so immediately emitted signals cannot be lost.
        let mut activated = bounded(stop, 10, portal.receive_activated()).await?;
        let mut deactivated = bounded(stop, 10, portal.receive_deactivated()).await?;
        let mut closed = bounded(stop, 10, session.receive_closed()).await?;
        super::status(crate::language::text(
            "Linux: подтвердите сочетания в системном окне",
            "Linux: confirm shortcuts in the system dialog",
        ));
        let mut shortcuts = Vec::new();
        for ((id, label), binding) in [
            (
                "volume-up",
                crate::language::text("ONIX: громче", "ONIX: volume up"),
            ),
            (
                "volume-down",
                crate::language::text("ONIX: тише", "ONIX: volume down"),
            ),
            (
                "gain-prev",
                crate::language::text("ONIX: усиление назад", "ONIX: previous gain"),
            ),
            (
                "gain-next",
                crate::language::text("ONIX: усиление вперёд", "ONIX: next gain"),
            ),
            (
                "filter-prev",
                crate::language::text("ONIX: фильтр назад", "ONIX: previous filter"),
            ),
            (
                "filter-next",
                crate::language::text("ONIX: фильтр вперёд", "ONIX: next filter"),
            ),
            (
                "balance-prev",
                crate::language::text("ONIX: баланс назад", "ONIX: previous balance"),
            ),
            (
                "balance-next",
                crate::language::text("ONIX: баланс вперёд", "ONIX: next balance"),
            ),
            (
                "keys-prev",
                crate::language::text("ONIX: кнопки назад", "ONIX: previous button mode"),
            ),
            (
                "keys-next",
                crate::language::text("ONIX: кнопки вперёд", "ONIX: next button mode"),
            ),
            (
                "brightness-prev",
                crate::language::text("ONIX: яркость меньше", "ONIX: lower brightness"),
            ),
            (
                "brightness-next",
                crate::language::text("ONIX: яркость больше", "ONIX: higher brightness"),
            ),
            (
                "saver-prev",
                crate::language::text("ONIX: заставка назад", "ONIX: previous screensaver delay"),
            ),
            (
                "saver-next",
                crate::language::text("ONIX: заставка вперёд", "ONIX: next screensaver delay"),
            ),
            (
                "orientation-prev",
                crate::language::text("ONIX: поворот назад", "ONIX: previous rotation"),
            ),
            (
                "orientation-next",
                crate::language::text("ONIX: поворот вперёд", "ONIX: next rotation"),
            ),
            (
                "idle-prev",
                crate::language::text("ONIX: экран назад", "ONIX: previous screen timeout"),
            ),
            (
                "idle-next",
                crate::language::text("ONIX: экран вперёд", "ONIX: next screen timeout"),
            ),
            (
                "font-prev",
                crate::language::text("ONIX: шрифт назад", "ONIX: previous font"),
            ),
            (
                "font-next",
                crate::language::text("ONIX: шрифт вперёд", "ONIX: next font"),
            ),
        ]
        .into_iter()
        .zip(config.0)
        {
            if let Some(binding) = binding {
                let trigger = binding.trigger()?;
                shortcuts.push(NewShortcut::new(id, label).preferred_trigger(trigger.as_str()));
            }
        }
        let request = bounded(
            stop,
            120,
            portal.bind_shortcuts(&session, &shortcuts, None, Default::default()),
        )
        .await?;
        let response = request.response().map_err(|e| e.to_string())?;
        let labels = response
            .shortcuts()
            .iter()
            .map(|s| format!("{}: {}", s.description(), s.trigger_description()))
            .collect::<Vec<_>>()
            .join("; ");
        super::status(if labels.is_empty() {
            crate::language::text(
                "Linux: сочетания не назначены",
                "Linux: shortcuts not assigned",
            )
            .into()
        } else {
            labels
        });
        let mut held = None;
        let mut repeat_at = None;
        loop {
            enum Next {
                Activation(Option<ashpd::desktop::global_shortcuts::Activated>),
                Deactivation(Option<ashpd::desktop::global_shortcuts::Deactivated>),
                Repeat,
                Closed,
                Stop,
            }
            let next = or(
                async {
                    let _ = stop.recv().await;
                    Next::Stop
                },
                or(
                    async {
                        closed.next().await;
                        Next::Closed
                    },
                    or(
                        async { Next::Activation(activated.next().await) },
                        or(
                            async { Next::Deactivation(deactivated.next().await) },
                            async {
                                match repeat_at {
                                    Some(at) => {
                                        async_io::Timer::at(at).await;
                                    }
                                    None => pending::<()>().await,
                                }
                                Next::Repeat
                            },
                        ),
                    ),
                ),
            )
            .await;
            match next {
                Next::Activation(Some(event)) => {
                    let action = action_for_id(event.shortcut_id());
                    if let Some(action) = action
                        && held != Some(action)
                    {
                        held = Some(action);
                        repeat_at = matches!(action, crate::hotkeys::Action::Volume(_))
                            .then(|| Instant::now() + Duration::from_millis(500));
                        super::dispatch(action);
                    }
                }
                Next::Deactivation(Some(event)) => {
                    if action_for_id(event.shortcut_id()) == held {
                        held = None;
                        repeat_at = None;
                    }
                }
                Next::Repeat => {
                    if let Some(action @ crate::hotkeys::Action::Volume(_)) = held {
                        super::dispatch(action);
                        repeat_at = Some(Instant::now() + Duration::from_millis(100));
                    }
                }
                Next::Activation(None) | Next::Deactivation(None) | Next::Closed => {
                    return Err(crate::language::text(
                        "сессия закрыта; повторите запуск через меню",
                        "session closed; restart hotkeys from the menu",
                    )
                    .into());
                }
                Next::Stop => break,
            }
        }
        Ok(())
    }
    .await;
    let _ = or(
        async {
            session.close().await.ok();
        },
        async {
            async_io::Timer::after(Duration::from_millis(500)).await;
        },
    )
    .await;
    result
}

fn action_for_id(id: &str) -> Option<crate::hotkeys::Action> {
    use crate::{hotkeys::Action, protocol::Setting};
    Some(match id {
        "volume-up" => Action::Volume(1),
        "volume-down" => Action::Volume(-1),
        "gain-prev" => Action::Setting(Setting::Gain, -1),
        "gain-next" => Action::Setting(Setting::Gain, 1),
        "filter-prev" => Action::Setting(Setting::Filter, -1),
        "filter-next" => Action::Setting(Setting::Filter, 1),
        "balance-prev" => Action::Setting(Setting::Balance, -1),
        "balance-next" => Action::Setting(Setting::Balance, 1),
        "keys-prev" => Action::Setting(Setting::Keys, -1),
        "keys-next" => Action::Setting(Setting::Keys, 1),
        "brightness-prev" => Action::Setting(Setting::Brightness, -1),
        "brightness-next" => Action::Setting(Setting::Brightness, 1),
        "saver-prev" => Action::Setting(Setting::Saver, -1),
        "saver-next" => Action::Setting(Setting::Saver, 1),
        "orientation-prev" => Action::Setting(Setting::Orientation, -1),
        "orientation-next" => Action::Setting(Setting::Orientation, 1),
        "idle-prev" => Action::Setting(Setting::Idle, -1),
        "idle-next" => Action::Setting(Setting::Idle, 1),
        "font-prev" => Action::Setting(Setting::Font, -1),
        "font-next" => Action::Setting(Setting::Font, 1),
        _ => return None,
    })
}
