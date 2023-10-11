use log::LevelFilter;
use std::io::Write;
use std::time::Instant;

pub(super) fn init_logging() {
    let mut builder = env_logger::builder();
    let level = std::env::var("RUST_LOG")
        .map(|s| s.parse().expect("Failed to parse RUST_LOG"))
        .unwrap_or(LevelFilter::Info);
    let started_at = Instant::now();
    builder
        .filter_level(level)
        .format_level(false)
        .format_target(false)
        .format_timestamp_secs()
        .format(move |buf, record| {
            let duration_sec = Instant::now().duration_since(started_at).as_secs();
            let seconds = duration_sec % 60;
            let minutes = duration_sec / 60 % 60;
            let hours = duration_sec / 3600;
            writeln!(
                buf,
                "# {:#02}:{:#02}:{:#02} - {:?}",
                hours,
                minutes,
                seconds,
                record.args()
            )
        });
    builder.init();
}
