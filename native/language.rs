use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Russian,
    English,
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

impl Language {
    pub fn code(self) -> &'static str {
        match self {
            Self::Russian => "ru",
            Self::English => "en",
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Self::Russian => Self::English,
            Self::English => Self::Russian,
        }
    }

    pub fn apply(self) -> Result<(), String> {
        slint::select_bundled_translation(self.code()).map_err(|error| error.to_string())?;
        CURRENT.store(self as u8, Ordering::Release);
        Ok(())
    }

    pub fn save(self) -> Result<(), String> {
        let dir = crate::instance::data_dir().map_err(|error| error.to_string())?;
        let temporary = dir.join("language.preference.tmp");
        std::fs::write(&temporary, self.code()).map_err(|error| error.to_string())?;
        std::fs::rename(temporary, dir.join("language.preference"))
            .map_err(|error| error.to_string())
    }
}

pub fn load() -> Language {
    crate::instance::data_dir()
        .ok()
        .and_then(|dir| std::fs::read_to_string(dir.join("language.preference")).ok())
        .and_then(|value| match value.trim() {
            "ru" => Some(Language::Russian),
            "en" => Some(Language::English),
            _ => None,
        })
        .unwrap_or_else(system_default)
}

fn system_default() -> Language {
    let russian = sys_locale::get_locale().is_some_and(|locale| {
        locale
            .split('-')
            .next()
            .is_some_and(|language| language.eq_ignore_ascii_case("ru"))
    });
    if russian {
        Language::Russian
    } else {
        Language::English
    }
}

pub fn initialize_from_installer(value: &str) -> Result<(), String> {
    let language = match value {
        "russian" => Language::Russian,
        "english" => Language::English,
        _ => return Err("Unsupported installer language".into()),
    };
    let path = crate::instance::data_dir()
        .map_err(|error| error.to_string())?
        .join("language.preference");
    if !path.exists() {
        language.save()?;
    }
    Ok(())
}

pub fn current() -> Language {
    match CURRENT.load(Ordering::Acquire) {
        1 => Language::English,
        _ => Language::Russian,
    }
}

pub fn prepare(language: Language) {
    CURRENT.store(language as u8, Ordering::Release);
}

pub fn text(russian: &'static str, english: &'static str) -> &'static str {
    match current() {
        Language::Russian => russian,
        Language::English => english,
    }
}

pub fn xi1_connected() -> &'static str {
    text("ONIX XI1 подключён", "ONIX XI1 connected")
}

pub fn xi1_disconnected() -> &'static str {
    text("ONIX XI1 отключён", "ONIX XI1 disconnected")
}

pub fn dac_connected() -> &'static str {
    text("ЦАП подключён", "DAC connected")
}

pub fn dac_disconnected() -> &'static str {
    text("ЦАП отключён", "DAC disconnected")
}

pub fn hotkeys_disabled() -> &'static str {
    text("Горячие клавиши отключены", "Hotkeys disabled")
}
