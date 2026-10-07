use std::time;

/// Returns the current time in microseconds.
pub fn current_time_micros() -> i64 {
    let micros = time::SystemTime::now()
        .duration_since(time::UNIX_EPOCH)
        .unwrap()
        .as_micros();
    i64::try_from(micros).unwrap()
}
