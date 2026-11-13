use core_engine::shortcut_time::{Clock, LocalTime};
use windows::Win32::{
    Foundation::{FILETIME, SYSTEMTIME},
    System::{
        SystemInformation::GetSystemTime,
        Time::{
            GetDynamicTimeZoneInformation, SystemTimeToFileTime, SystemTimeToTzSpecificLocalTimeEx,
            DYNAMIC_TIME_ZONE_INFORMATION,
        },
    },
};
pub(crate) struct WindowsClock;
impl Clock for WindowsClock {
    fn now(&self) -> Result<LocalTime, String> {
        let mut zone = DYNAMIC_TIME_ZONE_INFORMATION::default();
        // Convert one UTC instant with the captured zone, including DST for that instant.
        let (utc, local) = unsafe {
            if GetDynamicTimeZoneInformation(&mut zone) == u32::MAX {
                return Err("无法读取系统时区".into());
            }
            let utc = GetSystemTime();
            let mut local = SYSTEMTIME::default();
            SystemTimeToTzSpecificLocalTimeEx(Some(&zone), &utc, &mut local)
                .map_err(|e| e.to_string())?;
            (utc, local)
        };
        let ticks = |time: &SYSTEMTIME| -> Result<i64, String> {
            let mut file = FILETIME::default();
            unsafe { SystemTimeToFileTime(time, &mut file) }.map_err(|e| e.to_string())?;
            Ok(((file.dwHighDateTime as u64) << 32 | file.dwLowDateTime as u64) as i64)
        };
        let value = LocalTime {
            year: local.wYear,
            month: local.wMonth as u8,
            day: local.wDay as u8,
            hour: local.wHour as u8,
            minute: local.wMinute as u8,
            second: local.wSecond as u8,
            utc_offset_seconds: ((ticks(&local)? - ticks(&utc)?) / 10_000_000) as i32,
            timezone: String::from_utf16_lossy(
                &zone.TimeZoneKeyName[..zone
                    .TimeZoneKeyName
                    .iter()
                    .position(|c| *c == 0)
                    .unwrap_or(zone.TimeZoneKeyName.len())],
            ),
        };
        value.validate()?;
        Ok(value)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn system_clock_returns_a_valid_zoned_calendar_without_ui() {
        let value = WindowsClock.now().unwrap();
        assert!(value.year >= 2026);
        assert!(!value.timezone.is_empty());
        assert_eq!(value.utc_offset_seconds % 60, 0);
    }
}
