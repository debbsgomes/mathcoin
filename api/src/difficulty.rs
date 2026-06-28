use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ---- Clock trait ----

pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}

pub struct RealClock;

impl Clock for RealClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

pub struct FakeClock {
    now: Arc<Mutex<Instant>>,
}

impl FakeClock {
    pub fn new(t: Instant) -> Self {
        Self {
            now: Arc::new(Mutex::new(t)),
        }
    }

    pub fn advance(&self, d: Duration) {
        let mut now = self.now.lock().unwrap();
        *now += d;
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Instant {
        *self.now.lock().unwrap()
    }
}

// ---- Retarget config ----

#[derive(Debug, Clone)]
pub struct RetargetConfig {
    pub window: Duration,
    pub target_rate: f64,
    pub hysteresis_low: f64,
    pub hysteresis_high: f64,
    pub diff_min: u32,
    pub diff_max: u32,
    pub max_step: u32,
}

// ---- Pure retarget function ----

pub fn difficulty_retarget(current: u32, mint_rate: f64, config: &RetargetConfig) -> u32 {
    if mint_rate > config.hysteresis_high {
        let next = current.saturating_add(config.max_step);
        next.min(config.diff_max)
    } else if mint_rate < config.hysteresis_low {
        let next = current.saturating_sub(config.max_step);
        next.max(config.diff_min)
    } else {
        current
    }
}
