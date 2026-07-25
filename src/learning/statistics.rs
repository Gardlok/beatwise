pub(crate) fn lock_learning(state: &Mutex<LearningState>) -> MutexGuard<'_, LearningState> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn median(values: &mut [u64]) -> f64 {
    values.sort_unstable();
    let midpoint = values.len() / 2;

    if values.len() & 1 == 0 {
        (values[midpoint - 1] as f64 + values[midpoint] as f64) / 2.0
    } else {
        values[midpoint] as f64
    }
}

fn median_f64(values: &mut [f64]) -> f64 {
    values.sort_unstable_by(f64::total_cmp);
    let midpoint = values.len() / 2;

    if values.len() & 1 == 0 {
        (values[midpoint - 1] + values[midpoint]) / 2.0
    } else {
        values[midpoint]
    }
}

fn duration_from_micros(micros: f64) -> Duration {
    if !micros.is_finite() || micros <= 0.0 {
        return Duration::ZERO;
    }

    Duration::from_micros(micros.round().min(u64::MAX as f64) as u64)
}
