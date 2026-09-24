#[cfg(not(windows))]
use interprocess::local_socket::GenericFilePath;
#[cfg(windows)]
use interprocess::local_socket::GenericNamespaced;
use interprocess::local_socket::{
    ListenerOptions, Name,
    tokio::{Listener, Stream, prelude::*},
};
use std::{io, thread, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    runtime::{Builder, Runtime},
};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Show,
    Removed,
    Ping,
    Hotkeys,
    About,
}

impl Command {
    fn wire(self) -> u8 {
        match self {
            Self::Show => 1,
            Self::Removed => 2,
            Self::Ping => 3,
            Self::Hotkeys => 5,
            Self::About => 6,
        }
    }

    fn from_wire(byte: u8) -> Option<Self> {
        Some(match byte {
            1 => Self::Show,
            2 => Self::Removed,
            3 => Self::Ping,
            5 => Self::Hotkeys,
            6 => Self::About,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presence {
    DacUp,
    DacDown,
}

impl Presence {
    fn wire(self) -> u8 {
        match self {
            Self::DacUp => 1,
            Self::DacDown => 2,
        }
    }

    fn from_wire(byte: u8) -> Self {
        if byte == Self::DacUp.wire() {
            Self::DacUp
        } else {
            Self::DacDown
        }
    }

    fn current() -> Self {
        if crate::session::CONNECTED.load(std::sync::atomic::Ordering::Acquire) {
            Self::DacUp
        } else {
            Self::DacDown
        }
    }
}

fn runtime() -> io::Result<Runtime> {
    Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
}

async fn connect(file: &str) -> io::Result<Stream> {
    #[cfg(windows)]
    {
        use interprocess::os::windows::named_pipe::{pipe_mode::Bytes, tokio::DuplexPipeStream};
        let path = format!("\\\\.\\pipe\\{}", pipe_id(file)?);
        let pipe = DuplexPipeStream::<Bytes>::connect_by_path_with_wait_mode(
            path,
            interprocess::ConnectWaitMode::Timeout(Duration::ZERO),
        )
        .await?;
        Ok(Stream::NamedPipe(pipe.into()))
    }
    #[cfg(not(windows))]
    Stream::connect(socket_name(file)?).await
}

async fn bounded<T>(future: impl std::future::Future<Output = io::Result<T>>) -> io::Result<T> {
    tokio::time::timeout(HANDSHAKE_TIMEOUT, future)
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "ONIX IPC timed out"))?
}

fn socket_name(file: &str) -> io::Result<Name<'static>> {
    #[cfg(windows)]
    return pipe_id(file)?.to_ns_name::<GenericNamespaced>();
    #[cfg(not(windows))]
    crate::instance::data_dir()?
        .join(file)
        .to_fs_name::<GenericFilePath>()
}

#[cfg(windows)]
fn pipe_id(file: &str) -> io::Result<String> {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    crate::instance::data_dir()?.join(file).hash(&mut hash);
    Ok(format!("ONIX_XI1_Rust_{:016x}", hash.finish()))
}

fn listener(file: &str) -> io::Result<(Runtime, Listener)> {
    #[cfg(unix)]
    {
        let path = crate::instance::data_dir()?.join(file);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }
    let runtime = runtime()?;
    let listener = {
        let _entered = runtime.enter();
        ListenerOptions::new()
            .name(socket_name(file)?)
            .create_tokio()?
    };
    Ok((runtime, listener))
}

pub fn notify(command: Command) -> io::Result<Presence> {
    runtime()?.block_on(bounded(async {
        let mut stream = connect("control.sock").await?;
        stream.write_all(&[command.wire()]).await?;
        Ok(Presence::from_wire(stream.read_u8().await?))
    }))
}

pub fn ping() -> io::Result<Presence> {
    notify(Command::Ping)
}

pub struct Server {
    stop: async_channel::Sender<()>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Server {
    pub fn start() -> io::Result<Self> {
        let (runtime, listener) = listener("control.sock")?;
        let (stop, stopping) = async_channel::bounded::<()>(1);
        let worker = thread::spawn(move || {
            runtime.block_on(async move {
                loop {
                    let stream = tokio::select! {
                        biased;
                        _ = stopping.recv() => break,
                        stream = listener.accept() => stream,
                    };
                    let Ok(mut stream) = stream else { break };
                    tokio::spawn(async move {
                        let command = bounded(async {
                            let command = stream.read_u8().await?;
                            stream.write_all(&[Presence::current().wire()]).await?;
                            Ok(command)
                        })
                        .await;
                        match command.ok().and_then(Command::from_wire) {
                            Some(Command::Hotkeys) => {
                                let _ = slint::invoke_from_event_loop(|| {
                                    crate::with_app(|app| app.open_hotkeys())
                                });
                            }
                            Some(Command::About) => {
                                let _ = slint::invoke_from_event_loop(|| {
                                    crate::with_app(|app| app.open_about())
                                });
                            }
                            Some(Command::Show) => {
                                let _ = slint::invoke_from_event_loop(|| {
                                    crate::with_app(|app| app.show(true))
                                });
                            }
                            Some(Command::Removed) => {
                                let _ = slint::invoke_from_event_loop(|| {
                                    crate::with_app(|app| app.watcher_removed())
                                });
                            }
                            Some(Command::Ping) | None => {}
                        }
                    });
                }
            })
        });
        Ok(Self {
            stop,
            worker: Some(worker),
        })
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.close();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
