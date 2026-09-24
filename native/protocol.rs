pub const VID: u16 = 0x26b6;
pub const PID: u16 = 0x60c0;
#[cfg(windows)]
pub const INTERFACE: u8 = 2;
pub const CONTROL_INDEX: u16 = 0x09a0;
pub const REPLY_LEN: usize = 12;

pub const COMMAND_VOLUME: u8 = 1;
pub const COMMAND_SNAPSHOT: u8 = 0xf0;
pub const VOLUME_WRITE_MAX: u8 = 99;
pub const VOLUME_WIRE_MAX: u8 = 100;

const COMMAND_FILTER: u8 = 2;
const COMMAND_GAIN: u8 = 3;
const COMMAND_IDLE: u8 = 6;
const COMMAND_KEYS: u8 = 7;
const COMMAND_FONT: u8 = 8;
const COMMAND_BRIGHTNESS: u8 = 9;
const COMMAND_ORIENTATION: u8 = 10;
const COMMAND_SAVER: u8 = 11;
const COMMAND_BALANCE_LEFT: u8 = 12;
const COMMAND_BALANCE_RIGHT: u8 = 13;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setting {
    Gain,
    Filter,
    Balance,
    Keys,
    Brightness,
    Saver,
    Orientation,
    Idle,
    Font,
}

impl Setting {
    pub const ALL: [Self; 9] = [
        Self::Gain,
        Self::Filter,
        Self::Balance,
        Self::Keys,
        Self::Brightness,
        Self::Saver,
        Self::Orientation,
        Self::Idle,
        Self::Font,
    ];

    pub fn from_index(index: i32) -> Option<Self> {
        Self::ALL.get(usize::try_from(index).ok()?).copied()
    }

    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|setting| *setting == self)
            .unwrap()
    }

    pub fn notice_name(self) -> &'static str {
        match self {
            Self::Gain => "GAIN LEVEL",
            Self::Filter => "PCM FILTER",
            Self::Balance => "BALANCE",
            Self::Keys => "KEY MODE",
            Self::Brightness => "BRIGHTNESS",
            Self::Saver => "SCREENSAVER",
            Self::Orientation => "ORIENTATION",
            Self::Idle => "IDLE TIME",
            Self::Font => "SET FONT",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub volume: u8,
    pub gain: u8,
    pub filter: u8,
    pub balance: i16,
    pub brightness: u8,
    pub saver: u8,
    pub orientation: u8,
    pub keys: u8,
    pub font: u8,
    pub idle: u8,
}

impl Settings {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != REPLY_LEN {
            return Err("Invalid XI1 settings response".into());
        }
        if bytes[2] > 1
            || bytes[3] > 4
            || bytes[6] > 10
            || bytes[7] > 60
            || !bytes[7].is_multiple_of(5)
            || bytes[8] > 1
            || bytes[9] > 2
            || bytes[10] > 7
            || bytes[11] > 4
        {
            return Err("XI1 returned unsupported setting values".into());
        }
        Ok(Self {
            volume: bytes[1].min(VOLUME_WIRE_MAX),
            gain: bytes[2],
            filter: bytes[3],
            balance: i16::from_be_bytes([bytes[4], bytes[5]]).clamp(-12, 12),
            // Response offsets are NOT ordered by the setting command IDs.
            brightness: bytes[6],
            saver: bytes[7],
            orientation: bytes[8],
            keys: bytes[9],
            font: bytes[10],
            idle: bytes[11],
        })
    }

    // Logical Setting index, not wire command. Slint field-order remaps the grid.
    pub fn adjustment(&self, setting: Setting, direction: i32) -> (u8, u8) {
        let direction = direction.signum();
        let wrap = |value: u8, count: i32| (i32::from(value) + direction).rem_euclid(count) as u8;
        match setting {
            Setting::Gain => (COMMAND_GAIN, u8::from(self.gain == 0)),
            Setting::Filter => (COMMAND_FILTER, wrap(self.filter, 5)),
            Setting::Balance => {
                let next = (self.balance + direction as i16).clamp(-12, 12);
                (
                    if next < 0 {
                        COMMAND_BALANCE_LEFT
                    } else {
                        COMMAND_BALANCE_RIGHT
                    },
                    next.unsigned_abs() as u8,
                )
            }
            Setting::Keys => (COMMAND_KEYS, wrap(self.keys, 3)),
            Setting::Brightness => (
                COMMAND_BRIGHTNESS,
                (i32::from(self.brightness) + direction).clamp(0, 10) as u8,
            ),
            Setting::Saver => {
                let index = if self.saver <= 60 && self.saver.is_multiple_of(5) {
                    self.saver / 5
                } else {
                    0
                };
                (COMMAND_SAVER, wrap(index, 13) * 5)
            }
            Setting::Orientation => (COMMAND_ORIENTATION, u8::from(self.orientation == 0)),
            Setting::Idle => (COMMAND_IDLE, wrap(self.idle, 5)),
            Setting::Font => (COMMAND_FONT, wrap(self.font, 8)),
        }
    }

    pub fn cycle(&self, setting: Setting, direction: i32) -> (u8, u8) {
        match setting {
            Setting::Balance if direction > 0 && self.balance >= 12 => (COMMAND_BALANCE_LEFT, 12),
            Setting::Balance if direction < 0 && self.balance <= -12 => (COMMAND_BALANCE_RIGHT, 12),
            Setting::Brightness if direction > 0 && self.brightness >= 10 => {
                (COMMAND_BRIGHTNESS, 0)
            }
            Setting::Brightness if direction < 0 && self.brightness == 0 => {
                (COMMAND_BRIGHTNESS, 10)
            }
            _ => self.adjustment(setting, direction),
        }
    }

    pub fn labels(&self) -> Vec<String> {
        fn at(values: &[&str], index: u8) -> String {
            values.get(index as usize).unwrap_or(&"—").to_string()
        }
        vec![
            if self.gain == 0 { "LOW" } else { "HIGH" }.into(),
            at(&["FAST", "SLOW", "LL FAST", "LL SLOW", "NOS"], self.filter),
            if self.balance < 0 {
                format!("L{}", -self.balance)
            } else if self.balance > 0 {
                format!("R{}", self.balance)
            } else {
                "CENTER".into()
            },
            at(&["DAC", "SYSTEM", "TRACK"], self.keys),
            self.brightness.to_string(),
            if self.saver == 0 {
                "OFF".into()
            } else {
                format!("{}S", self.saver)
            },
            if self.orientation == 0 {
                "0°"
            } else {
                "180°"
            }
            .into(),
            at(&["10S", "30S", "1M", "5M", "OFF"], self.idle),
            format!("FONT {}", (self.font + 1).clamp(1, 8)),
        ]
    }
}

pub fn packet(command: u8, value: u8) -> [u8; 7] {
    [0xc7, 0xa3, command, value, 0, 0, 0]
}

// XI1 emits report 1 / System App Menu on physical DAC changes. Consumer
// buttons use report 2. Ignore release/idle reports; never infer volume from
// the key bits, always read the authoritative settings snapshot afterwards.
pub fn input_changes_settings(report: &[u8]) -> bool {
    report.len() >= 2 && matches!(report[0], 1 | 2) && report[1] != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Settings {
        Settings::parse(&[0, 25, 1, 1, 0, 0, 5, 20, 0, 0, 0, 2]).unwrap()
    }

    #[test]
    fn parse_rejects_short_and_clamps_loud_volume() {
        assert!(Settings::parse(&[0; 11]).is_err());
        let loud = Settings::parse(&[0, 101, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap();
        assert_eq!(loud.volume, VOLUME_WIRE_MAX);
        assert_eq!(sample().volume, 25);
    }

    #[test]
    fn setting_index_round_trips() {
        for (index, setting) in Setting::ALL.iter().copied().enumerate() {
            assert_eq!(Setting::from_index(index as i32), Some(setting));
            assert_eq!(setting.index(), index);
        }
        assert_eq!(Setting::from_index(9), None);
        assert_eq!(Setting::from_index(-1), None);
    }

    #[test]
    fn adjustment_uses_wire_commands() {
        let settings = sample();
        assert_eq!(settings.adjustment(Setting::Gain, 1), (COMMAND_GAIN, 0));
        assert_eq!(settings.adjustment(Setting::Filter, 1), (COMMAND_FILTER, 2));
        assert_eq!(
            settings.adjustment(Setting::Balance, -1),
            (COMMAND_BALANCE_LEFT, 1)
        );
    }

    #[test]
    fn volume_write_max_is_below_wire_max() {
        const { assert!(VOLUME_WRITE_MAX < VOLUME_WIRE_MAX) };
        assert_eq!(packet(COMMAND_VOLUME, VOLUME_WRITE_MAX)[2], COMMAND_VOLUME);
    }

    #[test]
    fn hid_reports_ignore_idle() {
        assert!(!input_changes_settings(&[1, 0]));
        assert!(input_changes_settings(&[1, 1]));
        assert!(input_changes_settings(&[2, 4]));
        assert!(!input_changes_settings(&[3, 1]));
    }
}
