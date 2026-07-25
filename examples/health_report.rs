use std::{error::Error, time::Duration};

use beatwise::{HealthPolicy, Monitor, TaskConfig, Timing};

fn main() -> Result<(), Box<dyn Error>> {
    let monitor = Monitor::new();
    let heartbeat = monitor.register(TaskConfig::new(
        "api-worker",
        Timing::fixed(Duration::from_secs(1)),
    ))?;

    let starting = monitor.summary(HealthPolicy::readiness());
    println!(
        "before first beat: verdict={:?}, starting={}",
        starting.verdict, starting.counts.starting,
    );

    heartbeat.beat()?;

    let report = monitor.report(HealthPolicy::liveness());
    println!(
        "after first beat: verdict={:?}, healthy={}",
        report.summary.verdict, report.summary.counts.healthy,
    );
    for task in report.tasks.iter() {
        println!("{}: {:?}", task.name, task.health);
    }

    Ok(())
}
