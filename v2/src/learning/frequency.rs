const MAX_PATTERN_SAMPLES: usize = MAX_PATTERN_PERIOD * MAX_PATTERN_CYCLES;

#[derive(Clone, Copy, Debug, Default)]
struct EwmaState {
    mean_micros: f64,
    deviation_micros: f64,
}

impl EwmaState {
    fn update(&mut self, observed: f64, alpha: f64) {
        if self.mean_micros <= 0.0 {
            self.mean_micros = observed;
            self.deviation_micros = 0.0;
            return;
        }

        let delta = observed - self.mean_micros;
        self.mean_micros += alpha * delta;
        self.deviation_micros += alpha * (delta.abs() - self.deviation_micros);
        self.deviation_micros = self.deviation_micros.max(0.0);
    }
}

#[derive(Clone, Copy, Debug)]
struct RobustWindowState {
    values: [u64; MAX_ROBUST_WINDOW],
    capacity: u8,
    len: u8,
    cursor: u8,
}

impl RobustWindowState {
    fn new(capacity: u8) -> Self {
        Self {
            values: [0; MAX_ROBUST_WINDOW],
            capacity,
            len: 0,
            cursor: 0,
        }
    }

    fn push(&mut self, observed_micros: u64) {
        let cursor = usize::from(self.cursor);
        self.values[cursor] = observed_micros;

        if self.len < self.capacity {
            self.len += 1;
        }

        self.cursor = (self.cursor + 1) % self.capacity;
    }

    fn center_micros(&self) -> Option<f64> {
        let len = usize::from(self.len);
        if len == 0 {
            return None;
        }

        let mut values = self.values;
        Some(median(&mut values[..len]))
    }

    fn deviation_micros(&self) -> Option<f64> {
        let len = usize::from(self.len);
        let center = self.center_micros()?;
        let mut deviations = [0_u64; MAX_ROBUST_WINDOW];

        for (target, value) in deviations.iter_mut().zip(self.values.iter()).take(len) {
            *target = value.abs_diff(center.round() as u64);
        }

        // 1.4826 scales MAD toward a standard-deviation-like quantity for
        // normally distributed samples while retaining robust outlier behavior.
        Some(median(&mut deviations[..len]) * 1.4826)
    }
}
