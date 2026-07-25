use std::{error::Error, time::Duration};

use thumper_v2::{Adaptation, LearnedTiming, LearningModel, Monitor, TaskConfig, Timing};

fn main() -> Result<(), Box<dyn Error>> {
    let monitor = Monitor::new();
    let timing = LearnedTiming::default()
        .with_minimum_samples(5)
        .with_minimum_grace(Duration::from_millis(25))
        .with_adaptation(Adaptation::Slow)
        .with_model(LearningModel::robust_window(5));
    let heartbeat = monitor.register(TaskConfig::new("learned-worker", Timing::Learned(timing)))?;

    heartbeat.beat()?;

    let status = monitor
        .status(heartbeat.id())
        .expect("registered task remains retained");
    println!("health={:?} timing={:?}", status.health, status.timing);

    Ok(())
}
