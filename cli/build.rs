//! Build script — injects compile-time metadata (timestamp, Fleet sync info).

use chrono::{Timelike, Utc};

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

    // Fleet sync info — set via env vars when building, or defaults to "unknown".
    // Update after running sync recipes: FLEET_SYNC_COMMIT=abc123 FLEET_SYNC_DATE=2026-04-03 cargo build
    let sync_commit = std::env::var("FLEET_SYNC_COMMIT").unwrap_or_else(|_| "unknown".to_string());
    let sync_date = std::env::var("FLEET_SYNC_DATE").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=FLEET_SYNC_COMMIT={}", sync_commit);
    println!("cargo:rustc-env=FLEET_SYNC_DATE={}", sync_date);
    println!("cargo:rerun-if-env-changed=FLEET_SYNC_COMMIT");
    println!("cargo:rerun-if-env-changed=FLEET_SYNC_DATE");

    // Target triple — cargo sets $TARGET for build scripts. Re-emit as a
    // runtime env so `flint version` can show what arch the binary was
    // compiled for (useful when triaging Intel vs. Apple Silicon issues).
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=TARGET_TRIPLE={}", target);
}

fn round_to_10_minutes(dt: &chrono::DateTime<chrono::Utc>) -> String {
    // Round minutes down to nearest 10-minute interval
    let rounded_minute = (dt.minute() / 10) * 10;
    let rounded_dt = dt
        .with_minute(rounded_minute)
        .unwrap()
        .with_second(0)
        .unwrap()
        .with_nanosecond(0)
        .unwrap();

    rounded_dt.format("%Y%m%d.%H%M").to_string()
}
