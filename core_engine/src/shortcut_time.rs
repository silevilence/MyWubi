//! Calendar values enter from a platform provider. Core never reads an OS clock.
use crate::shortcut_format::{DateField, FormatPart};
use serde::{Deserialize, Serialize};
use std::fmt::Write;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub utc_offset_seconds: i32,
    pub timezone: String,
}
pub trait Clock: Send + Sync {
    fn now(&self) -> Result<LocalTime, String>;
}
#[derive(Default)]
pub struct UnavailableClock;
impl Clock for UnavailableClock {
    fn now(&self) -> Result<LocalTime, String> {
        Err("平台未提供日期时间".into())
    }
}
impl LocalTime {
    pub fn validate(&self) -> Result<(), String> {
        let leap = self.year.is_multiple_of(4)
            && (!self.year.is_multiple_of(100) || self.year.is_multiple_of(400));
        let days = match self.month {
            2 if leap => 29,
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            _ => 0,
        };
        if self.year == 0
            || self.year > 9999
            || self.day == 0
            || self.day > days
            || self.hour > 23
            || self.minute > 59
            || self.second > 59
            || !(-86400..=86400).contains(&self.utc_offset_seconds)
        {
            return Err("平台返回了无效的本地日期时间或时区偏移".into());
        }
        Ok(())
    }
}
pub fn render(parts: &[FormatPart], time: &LocalTime) -> Result<String, String> {
    time.validate()?;
    let mut output = String::new();
    for part in parts {
        match part {
            FormatPart::Literal(text) => output.push_str(text),
            FormatPart::Field(field) => {
                let (value, width) = match field {
                    DateField::Year => (time.year, 4),
                    DateField::Month => (time.month.into(), 2),
                    DateField::Day => (time.day.into(), 2),
                    DateField::Hour => (time.hour.into(), 2),
                    DateField::Minute => (time.minute.into(), 2),
                    DateField::Second => (time.second.into(), 2),
                };
                write!(&mut output, "{value:0width$}").map_err(|e| e.to_string())?;
            }
        }
    }
    if output.is_empty() || output.chars().count() > crate::shortcuts::MAX_OUTPUT_SCALARS {
        return Err("格式化结果须为 1～512 个字符".into());
    }
    Ok(output)
}
