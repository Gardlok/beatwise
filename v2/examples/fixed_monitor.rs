use std::{error::Error, time::Duration};

use thumper_v2::{Monitor, TaskConfig, Timing};

fn main() -> Result<(), Box<dyn Error>> {
    let monitor = Monitor::new();
    let heartbeat = monitor.register(TaskConfig::new(
        "fixed-worker",
        Timing::fixed(Duration::from_secs(1)),
    ))?;

    heartbeat.beat()?;

    for status in monitor.snapshot() {
        println!(
            "id={} name={} heartbeats={} health={:?}",
            status.id.get(),
            status.name,
            status.heartbeat_count,
            status.health,
        );
    }

    Ok(())
}
