/// Logging utilities for verbose output with timestamps.
use std::io::{self, Write};
use std::time::SystemTime;

/// Format a timestamp in ISO-8601 Zulu format with fractional seconds.
pub fn timestamp() -> String {
    // Get current time
    let now = SystemTime::now();
    let duration = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("Time went backwards");

    let secs = duration.as_secs();
    let millis = duration.subsec_millis();

    // Convert to datetime components
    let days_since_epoch = secs / 86400;
    let secs_today = secs % 86400;
    let hours = secs_today / 3600;
    let minutes = (secs_today % 3600) / 60;
    let seconds = secs_today % 60;

    // Simplified year calculation (approximate, good enough for timestamps)
    let mut year = 1970;
    let mut remaining_days = days_since_epoch;
    loop {
        let year_days = if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
        {
            366
        } else {
            365
        };
        if remaining_days < year_days {
            break;
        }
        remaining_days -= year_days;
        year += 1;
    }

    // Simplified month/day calculation
    let is_leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_days = [
        31,
        if is_leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];

    let mut month = 1;
    let mut day = remaining_days + 1;
    for &days_in_month in &month_days {
        if day <= days_in_month {
            break;
        }
        day -= days_in_month;
        month += 1;
    }

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year, month, day, hours, minutes, seconds, millis
    )
}

/// Print a verbose message with timestamp prefix.
pub fn verbose_println(message: &str) {
    if !message.is_empty() {
        println!("[{}] {}", timestamp(), message);
    }
}

/// Print a polling dot on the same line (no newline).
pub fn verbose_print_dot() {
    print!(".");
    let _ = io::stdout().flush();
}

/// Print a newline after dots.
pub fn verbose_newline() {
    println!();
}
