use chrono::{Utc, Timelike};

fn main() {
    // Generate build timestamp for version display (rounded to 10-minute intervals)
    let timestamp = if let Ok(epoch_str) = std::env::var("SOURCE_DATE_EPOCH") {
        // Use SOURCE_DATE_EPOCH for reproducible builds (GitHub Actions)
        if let Ok(epoch_secs) = epoch_str.parse::<i64>() {
            if let Some(dt) = chrono::DateTime::from_timestamp(epoch_secs, 0) {
                round_to_10_minutes(&dt)
            } else {
                round_to_10_minutes(&Utc::now())
            }
        } else {
            round_to_10_minutes(&Utc::now())
        }
    } else {
        // Fallback: use current UTC time
        round_to_10_minutes(&Utc::now())
    };

    println!("cargo:rustc-env=BUILD_TIMESTAMP={}", timestamp);
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
}

fn round_to_10_minutes(dt: &chrono::DateTime<chrono::Utc>) -> String {
    // Round minutes down to nearest 10-minute interval
    let rounded_minute = (dt.minute() / 10) * 10;
    let rounded_dt = dt
        .with_minute(rounded_minute).unwrap()
        .with_second(0).unwrap()
        .with_nanosecond(0).unwrap();

    rounded_dt.format("%Y%m%d.%H%M").to_string()
}
