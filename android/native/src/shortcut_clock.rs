use core_engine::shortcut_time::{Clock, LocalTime};
use jni::{
    objects::{JString, JValue},
    JavaVM,
};
pub(crate) struct AndroidClock(pub JavaVM);
impl Clock for AndroidClock {
    fn now(&self) -> Result<LocalTime, String> {
        let mut env = self.0.attach_current_thread().map_err(|e| e.to_string())?;
        let value: jni::errors::Result<LocalTime> = (|| {
            // Gregorian dates must not change with the device's calendar locale.
            let calendar = env.new_object("java/util/GregorianCalendar", "()V", &[])?;
            let mut field = |id| {
                env.call_method(&calendar, "get", "(I)I", &[JValue::Int(id)])?
                    .i()
            };
            let year = field(1)? as u16;
            let month = (field(2)? + 1) as u8;
            let day = field(5)? as u8;
            let hour = field(11)? as u8;
            let minute = field(12)? as u8;
            let second = field(13)? as u8;
            let utc_offset_seconds = (field(15)? + field(16)?) / 1000;
            let zone = env
                .call_method(&calendar, "getTimeZone", "()Ljava/util/TimeZone;", &[])?
                .l()?;
            let name = JString::from(
                env.call_method(&zone, "getID", "()Ljava/lang/String;", &[])?
                    .l()?,
            );
            let timezone: String = env.get_string(&name)?.into();
            Ok(LocalTime {
                year,
                month,
                day,
                hour,
                minute,
                second,
                utc_offset_seconds,
                timezone,
            })
        })();
        // Clock failure is a candidate error, not a pending exception on the IME thread.
        if value.is_err() && env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
        value.map_err(|e| format!("读取系统时间失败：{e}"))
    }
}
