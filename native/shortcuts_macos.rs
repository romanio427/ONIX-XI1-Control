use core_foundation::{
    base::TCFType,
    mach_port::{CFMachPort, CFMachPortRef},
    runloop::{
        CFRunLoop, CFRunLoopSource, CFRunLoopSourceContext, CFRunLoopSourceCreate,
        CFRunLoopSourceInvalidate, CFRunLoopSourceSignal, CFRunLoopWakeUp, kCFRunLoopCommonModes,
    },
};
use objc2_app_kit::NSEvent;
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    sync::{Arc, Mutex, atomic::Ordering},
    thread,
};

// Use the native numeric event type: NSEvent's media-key event (14) is not
// represented in core-graphics' Rust enum and must never be transmuted into it.
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapCreate(
        location: u32,
        placement: u32,
        options: u32,
        mask: u64,
        callback: unsafe extern "C" fn(*mut c_void, u32, *mut c_void, *mut c_void) -> *mut c_void,
        user: *mut c_void,
    ) -> CFMachPortRef;
    fn CGEventTapEnable(port: CFMachPortRef, enable: bool);
    fn CGEventGetIntegerValueField(event: *mut c_void, field: u32) -> i64;
    fn CGEventGetFlags(event: *mut c_void) -> u64;
}

struct State {
    port: Cell<CFMachPortRef>,
    keys: RefCell<crate::hotkeys::Engine>,
}
pub struct Hook {
    stop: Arc<Mutex<Shutdown>>,
    worker: Option<thread::JoinHandle<()>>,
}

#[derive(Default)]
struct Shutdown {
    requested: bool,
    target: Option<WakeTarget>,
}

struct WakeTarget {
    source: CFRunLoopSource,
    runloop: CFRunLoop,
}

// This private source has no context data. The other thread only signals it;
// Core Foundation supports that operation across threads. Retained handles
// and the shutdown mutex keep it alive until the signal/cleanup completes.
unsafe impl Send for WakeTarget {}

extern "C" fn finish_runloop(_: *const c_void) {
    CFRunLoop::get_current().stop();
}

impl Hook {
    pub fn start(config: crate::hotkeys::Config) -> Self {
        let stop = Arc::new(Mutex::new(Shutdown::default()));
        let stopping = stop.clone();
        let worker = thread::spawn(move || unsafe {
            let mut state = Box::new(State {
                port: Cell::new(std::ptr::null_mut()),
                keys: RefCell::new(crate::hotkeys::Engine::new(config)),
            });
            // Session tap, head insertion, active (not listen-only), key down/up and media keys.
            let raw = CGEventTapCreate(
                1,
                0,
                0,
                (1 << 10) | (1 << 11) | (1 << 14),
                callback,
                (&mut *state as *mut State).cast(),
            );
            if raw.is_null() {
                super::status(crate::language::text(
                    "macOS: разрешите ONIX в Accessibility и повторите запуск клавиш",
                    "macOS: allow ONIX in Accessibility and restart hotkeys",
                ));
                return;
            }
            state.port.set(raw);
            let port = CFMachPort::wrap_under_create_rule(raw);
            let Ok(source) = port.create_runloop_source(0) else {
                super::status(crate::language::text(
                    "macOS: не удалось подключить обработчик клавиш",
                    "macOS: could not start the hotkey handler",
                ));
                return;
            };
            let runloop = CFRunLoop::get_current();
            let mut context = CFRunLoopSourceContext {
                version: 0,
                info: std::ptr::null_mut(),
                retain: None,
                release: None,
                copyDescription: None,
                equal: None,
                hash: None,
                schedule: None,
                cancel: None,
                perform: finish_runloop,
            };
            let raw_shutdown = CFRunLoopSourceCreate(std::ptr::null(), -1, &mut context);
            if raw_shutdown.is_null() {
                super::status(crate::language::text(
                    "macOS: не удалось создать сигнал завершения клавиш",
                    "macOS: could not create the hotkey shutdown signal",
                ));
                return;
            }
            let shutdown_source = CFRunLoopSource::wrap_under_create_rule(raw_shutdown);
            runloop.add_source(&source, kCFRunLoopCommonModes);
            runloop.add_source(&shutdown_source, kCFRunLoopCommonModes);
            CGEventTapEnable(raw, true);
            let should_run = {
                let mut shutdown = stopping.lock().unwrap();
                if shutdown.requested {
                    false
                } else {
                    shutdown.target = Some(WakeTarget {
                        source: shutdown_source.clone(),
                        runloop: runloop.clone(),
                    });
                    true
                }
            };
            if should_run {
                let assigned = config.0.iter().flatten().count();
                super::status(format!(
                    "{}: {assigned}",
                    crate::language::text("Назначено горячих клавиш", "Assigned hotkeys")
                ));
                // A source signaled before run_current stays pending: unlike
                // stopping a not-yet-running loop, this cannot lose shutdown.
                CFRunLoop::run_current();
            }
            stopping.lock().unwrap().target.take();
            CGEventTapEnable(raw, false);
            runloop.remove_source(&source, kCFRunLoopCommonModes);
            runloop.remove_source(&shutdown_source, kCFRunLoopCommonModes);
            CFRunLoopSourceInvalidate(shutdown_source.as_concrete_TypeRef());
            // Callback state outlives the disabled tap and its run-loop source.
            drop(source);
            drop(port);
            drop(state);
        });
        Self {
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Hook {
    fn drop(&mut self) {
        {
            let mut shutdown = self.stop.lock().unwrap();
            shutdown.requested = true;
            if let Some(target) = &shutdown.target {
                unsafe {
                    CFRunLoopSourceSignal(target.source.as_concrete_TypeRef());
                    CFRunLoopWakeUp(target.runloop.as_concrete_TypeRef());
                }
            }
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

unsafe extern "C" fn callback(
    _: *mut c_void,
    kind: u32,
    event: *mut c_void,
    context: *mut c_void,
) -> *mut c_void {
    // Drain Objective-C temporaries for each event, not on a periodic timer.
    objc2::rc::autoreleasepool(|_| unsafe { handle_event(kind, event, context) })
}

unsafe fn handle_event(kind: u32, event: *mut c_void, context: *mut c_void) -> *mut c_void {
    let state = unsafe { &*context.cast::<State>() };
    if kind == 0xffff_fffe || kind == 0xffff_ffff {
        let config = state.keys.borrow().config;
        *state.keys.borrow_mut() = crate::hotkeys::Engine::new(config);
        unsafe {
            CGEventTapEnable(state.port.get(), true);
        }
        return event;
    }
    if event.is_null() {
        return event;
    }
    let flags = unsafe { CGEventGetFlags(event) };
    let mut modifiers = 0;
    for (native, flag) in [
        (1 << 18, crate::hotkeys::MOD_CTRL),
        (1 << 19, crate::hotkeys::MOD_ALT),
        (1 << 17, crate::hotkeys::MOD_SHIFT),
        (1 << 20, crate::hotkeys::MOD_META),
    ] {
        if flags & native != 0 {
            modifiers |= flag;
        }
    }
    let dispatch = |key, down| {
        let (consume, action) = state.keys.borrow_mut().event(
            key,
            down,
            Some(modifiers),
            super::CONNECTED.load(Ordering::Acquire),
        );
        if let Some(action) = action {
            super::dispatch(action);
        }
        consume
    };
    if kind == 10 || kind == 11 {
        let code = unsafe { CGEventGetIntegerValueField(event, 9) } as u16;
        if let Some(key) = crate::hotkeys::mac_key(code)
            && dispatch(key, kind == 10)
        {
            return std::ptr::null_mut();
        }
    }
    if kind == 14 {
        // CGEvent is valid only during this callback. NSEvent retains its own representation.
        if let Some(native) = NSEvent::eventWithCGEvent(unsafe { &*event.cast() }) {
            if native.subtype().0 != 8 {
                return event;
            }
            let data = native.data1();
            let key = (data >> 16) & 0xffff;
            let phase = (data >> 8) & 0xff;
            let key = match key {
                0 => 175,
                1 => 174,
                7 => 173,
                16 => 179,
                _ => return event,
            };
            if matches!(phase, 0x0a | 0x0b) && dispatch(key, phase == 0x0a) {
                return std::ptr::null_mut();
            }
        }
    }
    event
}
