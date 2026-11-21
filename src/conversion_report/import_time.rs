use chrono::{DateTime, Local, Utc};

#[cfg(test)]
#[path = "import_time_test.rs"]
mod tests;

/// Uses the native locale formatter for the active desktop platform.
pub(super) fn format_local(instant: DateTime<Utc>) -> String {
    #[cfg(target_os = "macos")]
    {
        format_macos(instant)
    }
    #[cfg(target_os = "windows")]
    {
        format_windows(instant)
    }
    #[cfg(target_os = "linux")]
    {
        format_linux(instant)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        format_fallback(instant)
    }
}

/// Formats through Foundation's current locale and time zone.
#[cfg(target_os = "macos")]
fn format_macos(instant: DateTime<Utc>) -> String {
    use objc2_foundation::{NSDate, NSDateFormatter, NSDateFormatterStyle};

    let seconds = instant.timestamp() as f64 + f64::from(instant.timestamp_subsec_nanos()) / 1e9;
    let date = unsafe { NSDate::dateWithTimeIntervalSince1970(seconds) };
    let formatted = unsafe {
        NSDateFormatter::localizedStringFromDate_dateStyle_timeStyle(
            &date,
            NSDateFormatterStyle::NSDateFormatterShortStyle,
            NSDateFormatterStyle::NSDateFormatterShortStyle,
        )
    }
    .to_string();
    if formatted.is_empty() {
        format_fallback(instant)
    } else {
        formatted
    }
}

/// Formats one local instant with the current Windows regional settings.
#[cfg(target_os = "windows")]
fn format_windows(instant: DateTime<Utc>) -> String {
    use chrono::{Datelike, Timelike};
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::Globalization::{DATE_SHORTDATE, GetDateFormatEx, GetTimeFormatEx};

    let local = instant.with_timezone(&Local);
    let system_time = SYSTEMTIME {
        wYear: local.year() as u16,
        wMonth: local.month() as u16,
        wDayOfWeek: local.weekday().num_days_from_sunday() as u16,
        wDay: local.day() as u16,
        wHour: local.hour() as u16,
        wMinute: local.minute() as u16,
        wSecond: local.second() as u16,
        wMilliseconds: local.timestamp_subsec_millis() as u16,
    };
    let mut date = [0u16; 128];
    let mut time = [0u16; 128];
    let date_len = unsafe {
        GetDateFormatEx(
            std::ptr::null(),
            DATE_SHORTDATE,
            &system_time,
            std::ptr::null(),
            date.as_mut_ptr(),
            date.len() as i32,
            std::ptr::null(),
        )
    };
    let time_len = unsafe {
        GetTimeFormatEx(
            std::ptr::null(),
            0,
            &system_time,
            std::ptr::null(),
            time.as_mut_ptr(),
            time.len() as i32,
        )
    };
    if date_len <= 1 || time_len <= 1 {
        return format_fallback(instant);
    }
    format!("{} {}", wide_text(&date), wide_text(&time))
}

/// Uses `LC_TIME` through a private locale handle without changing process state.
#[cfg(target_os = "linux")]
fn format_linux(instant: DateTime<Utc>) -> String {
    use std::ffi::CStr;

    let timestamp = instant.timestamp() as libc::time_t;
    let mut local = std::mem::MaybeUninit::<libc::tm>::uninit();
    if unsafe { libc::localtime_r(&timestamp, local.as_mut_ptr()) }.is_null() {
        return format_fallback(instant);
    }
    let local = unsafe { local.assume_init() };
    let locale = unsafe { libc::newlocale(libc::LC_TIME_MASK, c"".as_ptr(), std::ptr::null_mut()) };
    if locale.is_null() {
        return format_fallback(instant);
    }

    let format = c"%x %X %Z";
    let mut output = [0 as libc::c_char; 128];
    let written = unsafe {
        libc::strftime_l(
            output.as_mut_ptr(),
            output.len(),
            format.as_ptr(),
            &local,
            locale,
        )
    };
    unsafe { libc::freelocale(locale) };
    if written == 0 {
        return format_fallback(instant);
    }
    unsafe { CStr::from_ptr(output.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

/// Produces a stable local-time fallback when the OS formatter is unavailable.
fn format_fallback(instant: DateTime<Utc>) -> String {
    instant
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M %Z")
        .to_string()
}

#[cfg(target_os = "windows")]
fn wide_text(buffer: &[u16]) -> String {
    String::from_utf16_lossy(
        &buffer[..buffer
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(buffer.len())],
    )
}
