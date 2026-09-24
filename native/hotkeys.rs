use slint::winit_030::winit::{
    event::ElementState,
    keyboard::{KeyCode, PhysicalKey},
};

pub const KEY_SHIFT: u16 = 16;
pub const KEY_CTRL: u16 = 17;
pub const KEY_ALT: u16 = 18;
pub const KEY_META: u16 = 91;

pub const MOD_CTRL: u8 = 1;
pub const MOD_ALT: u8 = 2;
pub const MOD_SHIFT: u8 = 4;
pub const MOD_META: u8 = 8;
const MOD_ALL: u8 = MOD_CTRL | MOD_ALT | MOD_SHIFT | MOD_META;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    modifiers: u8,
    key: u16,
}

impl Binding {
    pub fn two(first: u16, second: u16) -> Self {
        Self::from_keys(&[first, second]).expect("a shortcut needs one main key")
    }

    pub fn one(key: u16) -> Self {
        Self { modifiers: 0, key }
    }

    fn from_keys(keys: &[u16]) -> Option<Self> {
        let mut modifiers = 0;
        let mut main = None;
        for &key in keys {
            if let Some(modifier) = modifier_for_key(key) {
                modifiers |= modifier;
            } else if main.replace(key).is_some() {
                return None;
            }
        }
        Some(Self {
            modifiers,
            key: main?,
        })
    }

    #[cfg(windows)]
    pub fn modifiers(self) -> u8 {
        self.modifiers
    }

    #[cfg(windows)]
    pub fn key(self) -> u16 {
        self.key
    }

    pub fn is_single(self) -> bool {
        self.modifiers == 0
    }

    pub fn label(self) -> String {
        let mut parts = Vec::with_capacity(5);
        for (modifier, key) in [
            (MOD_CTRL, KEY_CTRL),
            (MOD_ALT, KEY_ALT),
            (MOD_SHIFT, KEY_SHIFT),
            (MOD_META, KEY_META),
        ] {
            if self.modifiers & modifier != 0 {
                parts.push(key_name(key));
            }
        }
        parts.push(key_name(self.key));
        parts.join(" + ")
    }

    #[cfg(target_os = "linux")]
    pub fn trigger(self) -> Result<String, String> {
        let key = KEYS
            .iter()
            .find(|candidate| candidate.code == self.key)
            .ok_or_else(|| {
                crate::language::text(
                    "Эта клавиша не поддерживается в Linux",
                    "This key is not supported on Linux",
                )
            })?;
        let mut trigger = String::new();
        for (modifier, name) in [
            (MOD_CTRL, "CTRL+"),
            (MOD_ALT, "ALT+"),
            (MOD_SHIFT, "SHIFT+"),
            (MOD_META, "LOGO+"),
        ] {
            if self.modifiers & modifier != 0 {
                trigger.push_str(name);
            }
        }
        trigger.push_str(key.xkb);
        Ok(trigger)
    }
}

pub const ACTION_COUNT: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Volume(i32),
    Setting(crate::protocol::Setting, i32),
}

#[cfg(any(windows, target_os = "macos"))]
pub const ACTIONS: [Action; ACTION_COUNT] = [
    Action::Volume(1),
    Action::Volume(-1),
    Action::Setting(crate::protocol::Setting::Gain, -1),
    Action::Setting(crate::protocol::Setting::Gain, 1),
    Action::Setting(crate::protocol::Setting::Filter, -1),
    Action::Setting(crate::protocol::Setting::Filter, 1),
    Action::Setting(crate::protocol::Setting::Balance, -1),
    Action::Setting(crate::protocol::Setting::Balance, 1),
    Action::Setting(crate::protocol::Setting::Keys, -1),
    Action::Setting(crate::protocol::Setting::Keys, 1),
    Action::Setting(crate::protocol::Setting::Brightness, -1),
    Action::Setting(crate::protocol::Setting::Brightness, 1),
    Action::Setting(crate::protocol::Setting::Saver, -1),
    Action::Setting(crate::protocol::Setting::Saver, 1),
    Action::Setting(crate::protocol::Setting::Orientation, -1),
    Action::Setting(crate::protocol::Setting::Orientation, 1),
    Action::Setting(crate::protocol::Setting::Idle, -1),
    Action::Setting(crate::protocol::Setting::Idle, 1),
    Action::Setting(crate::protocol::Setting::Font, -1),
    Action::Setting(crate::protocol::Setting::Font, 1),
];

#[derive(Clone, Copy, Debug)]
pub struct Config(pub [Option<Binding>; ACTION_COUNT]);

impl Default for Config {
    fn default() -> Self {
        let mut bindings = [None; ACTION_COUNT];
        bindings[0] = Some(Binding::two(KEY_CTRL, 0xaf));
        bindings[1] = Some(Binding::two(KEY_CTRL, 0xae));
        Self(bindings)
    }
}

struct Key {
    physical: KeyCode,
    code: u16,
    #[cfg(target_os = "macos")]
    mac: u16,
    name: &'static str,
    #[cfg(target_os = "linux")]
    xkb: &'static str,
}

macro_rules! keys {
    ($($physical:ident, $code:expr, $mac:expr, $name:literal, $xkb:literal);* $(;)?) => {
        const KEYS: &[Key] = &[$(Key { physical: KeyCode::$physical, code: $code,
            #[cfg(target_os="macos")] mac: $mac, name: $name,
            #[cfg(target_os="linux")] xkb: $xkb }),*];
    }
}

keys! {
    KeyA,65,0,"A","a"; KeyB,66,11,"B","b"; KeyC,67,8,"C","c";
    KeyD,68,2,"D","d"; KeyE,69,14,"E","e"; KeyF,70,3,"F","f";
    KeyG,71,5,"G","g"; KeyH,72,4,"H","h"; KeyI,73,34,"I","i";
    KeyJ,74,38,"J","j"; KeyK,75,40,"K","k"; KeyL,76,37,"L","l";
    KeyM,77,46,"M","m"; KeyN,78,45,"N","n"; KeyO,79,31,"O","o";
    KeyP,80,35,"P","p"; KeyQ,81,12,"Q","q"; KeyR,82,15,"R","r";
    KeyS,83,1,"S","s"; KeyT,84,17,"T","t"; KeyU,85,32,"U","u";
    KeyV,86,9,"V","v"; KeyW,87,13,"W","w"; KeyX,88,7,"X","x";
    KeyY,89,16,"Y","y"; KeyZ,90,6,"Z","z";
    Digit0,48,29,"0","0"; Digit1,49,18,"1","1"; Digit2,50,19,"2","2";
    Digit3,51,20,"3","3"; Digit4,52,21,"4","4"; Digit5,53,23,"5","5";
    Digit6,54,22,"6","6"; Digit7,55,26,"7","7"; Digit8,56,28,"8","8"; Digit9,57,25,"9","9";
    Escape,27,53,"Esc","Escape"; Space,32,49,"Space","space";
    Enter,13,36,"Enter","Return"; Tab,9,48,"Tab","Tab"; Backspace,8,51,"Backspace","BackSpace";
    Delete,46,117,"Delete","Delete"; Insert,45,114,"Insert","Insert";
    Home,36,115,"Home","Home"; End,35,119,"End","End";
    PageUp,33,116,"Page Up","Prior"; PageDown,34,121,"Page Down","Next";
    ArrowLeft,37,123,"←","Left"; ArrowRight,39,124,"→","Right";
    ArrowUp,38,126,"↑","Up"; ArrowDown,40,125,"↓","Down";
    F1,112,122,"F1","F1"; F2,113,120,"F2","F2"; F3,114,99,"F3","F3";
    F4,115,118,"F4","F4"; F5,116,96,"F5","F5"; F6,117,97,"F6","F6";
    F7,118,98,"F7","F7"; F8,119,100,"F8","F8"; F9,120,101,"F9","F9";
    F10,121,109,"F10","F10"; F11,122,103,"F11","F11"; F12,123,111,"F12","F12";
    F13,124,105,"F13","F13"; F14,125,107,"F14","F14"; F15,126,113,"F15","F15";
    F16,127,106,"F16","F16"; F17,128,64,"F17","F17"; F18,129,79,"F18","F18";
    F19,130,80,"F19","F19"; F20,131,90,"F20","F20";
    AudioVolumeUp,175,0xffff,"Volume +","XF86AudioRaiseVolume";
    AudioVolumeDown,174,0xffff,"Volume −","XF86AudioLowerVolume";
    AudioVolumeMute,173,0xffff,"Mute","XF86AudioMute";
    MediaPlayPause,179,0xffff,"Play/Pause","XF86AudioPlay";
    CapsLock,20,57,"Caps Lock","Caps_Lock";
    Minus,189,27,"−","minus"; Equal,187,24,"=","equal";
    BracketLeft,219,33,"[","bracketleft"; BracketRight,221,30,"]","bracketright";
    Backslash,220,42,"\\","backslash"; Semicolon,186,41,";","semicolon";
    Quote,222,39,"'","apostrophe"; Backquote,192,50,"`","grave";
    Comma,188,43,",","comma"; Period,190,47,".","period"; Slash,191,44,"/","slash";
    NumpadAdd,107,69,"Num +","KP_Add"; NumpadSubtract,109,78,"Num −","KP_Subtract";
    NumpadMultiply,106,67,"Num ×","KP_Multiply"; NumpadDivide,111,75,"Num /","KP_Divide";
    NumpadDecimal,110,65,"Num .","KP_Decimal"; NumpadEnter,14,76,"Num Enter","KP_Enter";
    Numpad0,96,82,"Num 0","KP_0"; Numpad1,97,83,"Num 1","KP_1";
    Numpad2,98,84,"Num 2","KP_2"; Numpad3,99,85,"Num 3","KP_3";
    Numpad4,100,86,"Num 4","KP_4"; Numpad5,101,87,"Num 5","KP_5";
    Numpad6,102,88,"Num 6","KP_6"; Numpad7,103,89,"Num 7","KP_7";
    Numpad8,104,91,"Num 8","KP_8"; Numpad9,105,92,"Num 9","KP_9";
}

pub fn physical(code: KeyCode) -> Option<u16> {
    match code {
        KeyCode::ControlLeft | KeyCode::ControlRight => Some(KEY_CTRL),
        KeyCode::AltLeft | KeyCode::AltRight => Some(KEY_ALT),
        KeyCode::ShiftLeft | KeyCode::ShiftRight => Some(KEY_SHIFT),
        KeyCode::SuperLeft | KeyCode::SuperRight => Some(KEY_META),
        _ => KEYS
            .iter()
            .find(|key| key.physical == code)
            .map(|key| key.code),
    }
}

#[cfg(target_os = "macos")]
pub fn mac_key(code: u16) -> Option<u16> {
    KEYS.iter()
        .find(|key| key.mac == code && code != 0xffff)
        .map(|key| key.code)
}

fn key_name(code: u16) -> &'static str {
    match code {
        KEY_CTRL => "Ctrl",
        KEY_ALT => "Alt",
        KEY_SHIFT => "Shift",
        KEY_META if cfg!(target_os = "macos") => "Cmd",
        KEY_META => "Win/Super",
        _ => KEYS
            .iter()
            .find(|key| key.code == code)
            .map_or("?", |key| key.name),
    }
}

fn modifier_for_key(code: u16) -> Option<u8> {
    Some(match code {
        KEY_CTRL => MOD_CTRL,
        KEY_ALT => MOD_ALT,
        KEY_SHIFT => MOD_SHIFT,
        KEY_META => MOD_META,
        _ => return None,
    })
}

fn supported(code: u16) -> bool {
    KEYS.iter().any(|key| key.code == code)
}

impl Config {
    pub fn labels(self) -> Vec<String> {
        self.0
            .into_iter()
            .map(|binding| {
                binding.map_or_else(
                    || crate::language::text("Не назначено", "Not assigned").into(),
                    Binding::label,
                )
            })
            .collect()
    }

    pub fn validate(self) -> Result<(), String> {
        for (index, binding) in self.0.iter().enumerate() {
            if binding.is_some() && self.0[..index].contains(binding) {
                return Err(crate::language::text(
                    "Одно сочетание нельзя назначить двум действиям",
                    "One shortcut cannot be assigned to two actions",
                )
                .into());
            }
        }
        for binding in self.0.into_iter().flatten() {
            if binding.modifiers & !MOD_ALL != 0 || !supported(binding.key) {
                return Err(crate::language::text(
                    "Выберите основную клавишу и, при желании, Ctrl, Alt, Shift или Win",
                    "Choose a main key and optionally Ctrl, Alt, Shift, or Win",
                )
                .into());
            }
            #[cfg(target_os = "macos")]
            if binding.key == 20 {
                return Err(crate::language::text(
                    "Caps Lock на macOS не поддерживается; выберите другую клавишу",
                    "Caps Lock is not supported on macOS; choose another key",
                )
                .into());
            }
            #[cfg(target_os = "linux")]
            binding.trigger()?;
            if binding.key == 46 && binding.modifiers & (MOD_CTRL | MOD_ALT) == MOD_CTRL | MOD_ALT {
                return Err(crate::language::text(
                    "Это сочетание зарезервировано системой",
                    "This shortcut is reserved by the system",
                )
                .into());
            }
            if binding.key == 76 && binding.modifiers & MOD_META != 0 {
                return Err(crate::language::text(
                    "Это сочетание зарезервировано системой",
                    "This shortcut is reserved by the system",
                )
                .into());
            }
        }
        Ok(())
    }

    pub fn load() -> Result<Self, String> {
        let path = crate::instance::data_dir()
            .map_err(|error| error.to_string())?
            .join("hotkeys.conf");
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(error.to_string()),
        };
        let mut lines = text.lines();
        let version = lines.next().ok_or_else(|| {
            crate::language::text("Пустой файл горячих клавиш", "The hotkey file is empty")
        })?;
        let slots = match version {
            "ONIX-HOTKEYS-6" | "ONIX-HOTKEYS-5" => ACTION_COUNT,
            "ONIX-HOTKEYS-4" => 11,
            _ => 2,
        };
        let mut loaded = Vec::with_capacity(slots);
        for _ in 0..slots {
            let line = lines.next().ok_or_else(|| {
                crate::language::text(
                    "Неполный файл горячих клавиш",
                    "The hotkey file is incomplete",
                )
            })?;
            if line == "off" {
                loaded.push(None);
                continue;
            }
            loaded.push(match version {
                "ONIX-HOTKEYS-6" => {
                    let (modifiers, key) = line.split_once(':').ok_or_else(|| {
                        crate::language::text("Повреждено сочетание", "The shortcut is malformed")
                    })?;
                    Some(Binding {
                        modifiers: modifiers.parse().map_err(|_| {
                            crate::language::text("Некорректные модификаторы", "Invalid modifiers")
                        })?,
                        key: key.parse().map_err(|_| {
                            crate::language::text("Некорректная клавиша", "Invalid key")
                        })?,
                    })
                }
                "ONIX-HOTKEYS-5" | "ONIX-HOTKEYS-4" | "ONIX-HOTKEYS-3" => {
                    if let Some((first, second)) = line.split_once('+') {
                        Some(
                            Binding::from_keys(&[
                                first.parse().map_err(|_| {
                                    crate::language::text("Некорректная клавиша", "Invalid key")
                                })?,
                                second.parse().map_err(|_| {
                                    crate::language::text("Некорректная клавиша", "Invalid key")
                                })?,
                            ])
                            .ok_or_else(|| {
                                crate::language::text(
                                    "В старом бинде две основные клавиши",
                                    "A legacy shortcut contains two main keys",
                                )
                            })?,
                        )
                    } else {
                        Some(Binding::one(line.parse().map_err(|_| {
                            crate::language::text("Некорректная клавиша", "Invalid key")
                        })?))
                    }
                }
                "ONIX-HOTKEYS-2" => {
                    let (first, second) = line.split_once('+').ok_or_else(|| {
                        crate::language::text("Повреждено сочетание", "The shortcut is malformed")
                    })?;
                    Some(
                        Binding::from_keys(&[
                            first.parse().map_err(|_| {
                                crate::language::text("Некорректная клавиша", "Invalid key")
                            })?,
                            second.parse().map_err(|_| {
                                crate::language::text("Некорректная клавиша", "Invalid key")
                            })?,
                        ])
                        .ok_or_else(|| {
                            crate::language::text(
                                "В старом бинде две основные клавиши",
                                "A legacy shortcut contains two main keys",
                            )
                        })?,
                    )
                }
                "ONIX-HOTKEYS-1" => match migrate_v1(line) {
                    Some(binding) => Some(binding),
                    None => return Ok(Self::default()),
                },
                _ => {
                    return Err(crate::language::text(
                        "Неизвестный формат горячих клавиш",
                        "Unknown hotkey file format",
                    )
                    .into());
                }
            });
        }
        if lines.next().is_some() {
            return Err(crate::language::text(
                "Лишние данные в файле горячих клавиш",
                "Unexpected data in the hotkey file",
            )
            .into());
        }
        let mut config = Self([None; ACTION_COUNT]);
        config.0[0] = loaded[0];
        config.0[1] = loaded[1];
        if matches!(version, "ONIX-HOTKEYS-6" | "ONIX-HOTKEYS-5") {
            config.0.copy_from_slice(&loaded);
        } else if version == "ONIX-HOTKEYS-4" {
            for setting in 0..9 {
                config.0[3 + setting * 2] = loaded[2 + setting];
            }
        }
        config.validate()?;
        Ok(config)
    }

    pub fn save(self) -> Result<(), String> {
        self.validate()?;
        let dir = crate::instance::data_dir().map_err(|error| error.to_string())?;
        let mut text = String::from("ONIX-HOTKEYS-6\n");
        for binding in self.0 {
            text.push_str(&binding.map_or_else(
                || "off".into(),
                |binding| format!("{}:{}", binding.modifiers, binding.key),
            ));
            text.push('\n');
        }
        let temporary = dir.join("hotkeys.conf.tmp");
        use std::io::Write;
        let mut file = std::fs::File::create(&temporary).map_err(|error| error.to_string())?;
        file.write_all(text.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|error| error.to_string())?;
        drop(file);
        std::fs::rename(temporary, dir.join("hotkeys.conf")).map_err(|error| error.to_string())
    }
}

fn migrate_v1(line: &str) -> Option<Binding> {
    let (modifiers, key) = line.split_once(':')?;
    let modifiers: u8 = modifiers.parse().ok()?;
    let key: u16 = key.parse().ok()?;
    (modifiers & !MOD_ALL == 0).then_some(Binding { modifiers, key })
}

/// Editor capture: zero or more modifiers followed by one main key.
#[derive(Default)]
pub struct Capture {
    held: Vec<(PhysicalKey, u16)>,
    captured: Vec<PhysicalKey>,
}

pub enum CaptureAction {
    Ignore,
    Swallow,
    Message(&'static str),
    Bound(Binding),
}

impl Capture {
    pub fn focus_lost(&mut self) {
        self.held.clear();
        self.captured.clear();
    }

    pub fn key(
        &mut self,
        physical_key: PhysicalKey,
        state: ElementState,
        repeat: bool,
        recording: bool,
    ) -> CaptureAction {
        if self.captured.contains(&physical_key) {
            if state == ElementState::Released {
                self.captured.retain(|key| *key != physical_key);
            }
            return CaptureAction::Swallow;
        }
        if !recording {
            return CaptureAction::Ignore;
        }
        let PhysicalKey::Code(code) = physical_key else {
            return CaptureAction::Swallow;
        };
        let Some(key) = physical(code) else {
            return CaptureAction::Message(crate::language::text(
                "Эта клавиша не поддерживается. Выберите букву, цифру, F-клавишу, стрелку или медиаклавишу.",
                "This key is not supported. Choose a letter, number, function key, arrow, or media key.",
            ));
        };
        match state {
            ElementState::Pressed if !repeat => {
                if !self.held.iter().any(|(_, held)| *held == key) {
                    self.held.push((physical_key, key));
                }
                if modifier_for_key(key).is_some() {
                    return CaptureAction::Message(crate::language::text(
                        "Нажмите ещё одну клавишу, чтобы завершить сочетание",
                        "Press one more key to complete the shortcut",
                    ));
                }
                let modifiers = self
                    .held
                    .iter()
                    .filter_map(|(_, key)| modifier_for_key(*key))
                    .fold(0, |all, modifier| all | modifier);
                CaptureAction::Bound(Binding { modifiers, key })
            }
            ElementState::Released => {
                self.held.retain(|(_, held)| *held != key);
                CaptureAction::Swallow
            }
            _ => CaptureAction::Swallow,
        }
    }

    pub fn commit(&mut self) {
        self.captured
            .extend(self.held.iter().map(|(physical, _)| *physical));
        self.held.clear();
    }

    pub fn abort_combo(&mut self) {
        self.held.clear();
    }
}

#[cfg(target_os = "macos")]
pub struct Engine {
    pub config: Config,
    held: [bool; 256],
    consumed: [bool; 256],
}

#[cfg(target_os = "macos")]
impl Engine {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            held: [false; 256],
            consumed: [false; 256],
        }
    }

    pub fn event(
        &mut self,
        key: u16,
        down: bool,
        modifiers: Option<u8>,
        enabled: bool,
    ) -> (bool, Option<Action>) {
        let index = usize::from(key);
        if index >= self.held.len() {
            return (false, None);
        }
        let repeated = down && self.held[index];
        self.held[index] = down;
        if !down {
            return (std::mem::take(&mut self.consumed[index]), None);
        }
        let mut active_modifiers = modifiers.unwrap_or(0) & MOD_ALL;
        for (flag, modifier) in [
            (MOD_CTRL, KEY_CTRL),
            (MOD_ALT, KEY_ALT),
            (MOD_SHIFT, KEY_SHIFT),
            (MOD_META, KEY_META),
        ] {
            if self.held[usize::from(modifier)] {
                active_modifiers |= flag;
            }
        }
        for (action, binding) in self.config.0.into_iter().enumerate() {
            if let Some(binding) = binding
                && binding.key == key
                && binding.modifiers == active_modifiers
            {
                // Ownership is decided by the first key-down and kept until
                // key-up, so the system never receives half of a media-key press.
                if repeated && !self.consumed[index] {
                    return (false, None);
                }
                self.consumed[index] = true;
                // Swallow the combo even if the DAC is briefly marked disconnected.
                let action = ACTIONS[action];
                let trigger = enabled && !(repeated && matches!(action, Action::Setting(_, _)));
                return (true, trigger.then_some(action));
            }
        }
        (self.consumed[index], None)
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    fn engine() -> Engine {
        Engine::new(Config::default())
    }

    #[test]
    fn volume_without_ctrl_is_not_consumed() {
        let mut engine = engine();
        let (consume, action) = engine.event(0xaf, true, None, true);
        assert!(!consume);
        assert_eq!(action, None);
        let (consume, action) = engine.event(0xaf, false, None, true);
        assert!(!consume);
        assert_eq!(action, None);
    }

    #[test]
    fn ctrl_then_volume_swallows_down_and_up() {
        let mut engine = engine();
        assert_eq!(engine.event(KEY_CTRL, true, None, true), (false, None));
        let (consume, action) = engine.event(0xaf, true, None, true);
        assert!(consume);
        assert_eq!(action, Some(Action::Volume(1)));
        let (consume, action) = engine.event(0xaf, false, None, true);
        assert!(consume);
        assert_eq!(action, None);
        let (consume, action) = engine.event(0xae, true, None, true);
        assert!(consume);
        assert_eq!(action, Some(Action::Volume(-1)));
    }

    #[test]
    fn disconnected_still_swallows_so_windows_does_not_see_the_key() {
        let mut engine = engine();
        engine.event(KEY_CTRL, true, None, false);
        let (consume, action) = engine.event(0xaf, true, None, false);
        assert!(consume);
        assert_eq!(action, None);
    }

    #[test]
    fn volume_before_ctrl_is_not_a_combo() {
        let mut engine = engine();
        let (consume, action) = engine.event(0xaf, true, None, true);
        assert!(!consume);
        assert_eq!(action, None);
        engine.event(KEY_CTRL, true, None, true);
        let (consume, action) = engine.event(0xaf, false, None, true);
        assert!(!consume);
        assert_eq!(action, None);
    }
}
