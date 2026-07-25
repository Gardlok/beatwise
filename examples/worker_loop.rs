use std::{error::Error, thread, time::Duration};

use beatwise::{Monitor, StoppedError, TaskConfig, Timing};

fn main() -> Result<(), Box<dyn Error>> {
    let monitor = Monitor::new();
    let heartbeat = monitor.register(
        TaskConfig::new("worker-loop", Timing::fixed(Duration::from_millis(250)))
            .with_startup_grace(Duration::from_secs(1)),
    )?;
    let task_id = heartbeat.id();

    let worker = thread::spawn(move || -> Result<(), StoppedError> {
        for completed_step in 1..=3 {
            thread::sleep(Duration::from_millis(50));
            heartbeat.beat()?;
            println!("worker completed step {completed_step}");
        }

        // Dropping the final heartbeat at the end of this closure marks the
        // retained task record as stopped.
        Ok(())
    });

    while !worker.is_finished() {
        thread::sleep(Duration::from_millis(25));
        if let Some(status) = monitor.status(task_id) {
            println!("observer: {:?}", status.health);
        }
    }

    worker.join().expect("worker thread panicked")?;

    let stopped = monitor
        .status(task_id)
        .expect("stopped task remains retained until purged");
    println!("after join: {:?}", stopped.health);

    assert_eq!(monitor.purge_stopped(), 1);
    assert!(monitor.status(task_id).is_none());

    Ok(())
}
