use std::time::{Duration, Instant};
use wtm_core::Service;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let samples: u32 = std::env::args().nth(1).unwrap_or_else(|| "5".into()).parse()?;
    let warmup: u64 = std::env::args().nth(2).unwrap_or_else(|| "60".into()).parse()?;
    if samples == 0 { return Err("At least one sample is required".into()); }
    let service = match std::env::var_os("WTM_BENCH_CONFIG") {
        Some(path) => Service::at(path),
        None => Service::user()?,
    };
    let mut monitor = service.monitor_from(std::env::current_dir()?)?;
    let started = Instant::now();
    std::thread::sleep(Duration::from_secs(warmup));
    for sample in 1..=samples {
        std::thread::sleep(Duration::from_secs(60));
        let update = monitor.latest().ok_or("Monitor did not publish a snapshot")?;
        if update.snapshot.has_errors() || update.launch_error.is_some() {
            return Err(format!("Measurement failed: {:?}\n{}", update.launch_error, update.snapshot.text()).into());
        }
        let worktrees: usize = update.snapshot.repos.iter().map(|repo| repo.worktrees.as_ref().map_or(0, Vec::len)).sum();
        println!("{}", serde_json::json!({
            "sample": sample,
            "elapsed_seconds": started.elapsed().as_secs_f64(),
            "polling_seconds_last_minute": update.polling_time_last_minute.as_secs_f64(),
            "repositories": update.snapshot.repos.len(),
            "worktrees": worktrees,
            "snapshot": update.snapshot.text(),
        }));
    }
    monitor.stop()?;
    Ok(())
}
