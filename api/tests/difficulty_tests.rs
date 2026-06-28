/// Tests for Clock trait + difficulty_retarget pure function.
/// Uses FakeClock for deterministic time control.
use mathcoin_api::difficulty::{Clock, FakeClock, RealClock, difficulty_retarget, RetargetConfig};
use std::time::{Duration, Instant};

fn default_config() -> RetargetConfig {
    RetargetConfig {
        window: Duration::from_secs(60),
        target_rate: 20.0,
        hysteresis_low: 15.0,
        hysteresis_high: 25.0,
        diff_min: 1,
        diff_max: 12,
        max_step: 1,
    }
}

// ---- Clock trait ----

#[test]
fn fake_clock_starts_at_given_time() {
    let t = Instant::now();
    let clock = FakeClock::new(t);
    assert_eq!(clock.now(), t);
}

#[test]
fn fake_clock_advances() {
    let t = Instant::now();
    let clock = FakeClock::new(t);
    clock.advance(Duration::from_secs(10));
    assert_eq!(clock.now(), t + Duration::from_secs(10));
    clock.advance(Duration::from_secs(30));
    assert_eq!(clock.now(), t + Duration::from_secs(40));
}

#[test]
fn real_clock_returns_something_close_to_now() {
    let clock = RealClock;
    let before = Instant::now();
    let now = clock.now();
    let after = Instant::now();
    assert!(now >= before);
    assert!(now <= after);
}

// ---- difficulty_retarget ----

#[test]
fn rate_above_high_hysteresis_increases_difficulty() {
    let cfg = default_config();
    // Rate = 30 (above HIGH=25)
    let new_diff = difficulty_retarget(4, 30.0, &cfg);
    assert_eq!(new_diff, 5, "should increase by 1 (clamp)");
}

#[test]
fn rate_below_low_hysteresis_decreases_difficulty() {
    let cfg = default_config();
    // Rate = 10 (below LOW=15)
    let new_diff = difficulty_retarget(4, 10.0, &cfg);
    assert_eq!(new_diff, 3, "should decrease by 1 (clamp)");
}

#[test]
fn rate_inside_band_no_change() {
    let cfg = default_config();
    // Rate = 20 (inside [15,25])
    assert_eq!(difficulty_retarget(4, 20.0, &cfg), 4);
    // Rate = 15 (low boundary)
    assert_eq!(difficulty_retarget(4, 15.0, &cfg), 4);
    // Rate = 25 (high boundary)
    assert_eq!(difficulty_retarget(4, 25.0, &cfg), 4);
    // Rate = 18 (inside)
    assert_eq!(difficulty_retarget(7, 18.0, &cfg), 7);
}

#[test]
fn huge_overshoot_still_only_one_step() {
    let cfg = default_config();
    // Rate = 100 (massively above HIGH=25)
    // Should still only increase by 1 (clamp at max_step)
    assert_eq!(difficulty_retarget(4, 100.0, &cfg), 5);
    assert_eq!(difficulty_retarget(4, 1000.0, &cfg), 5);
}

#[test]
fn zero_rate_still_only_one_step_down() {
    let cfg = default_config();
    // Rate = 0 (massively below LOW=15)
    assert_eq!(difficulty_retarget(4, 0.0, &cfg), 3);
    assert_eq!(difficulty_retarget(2, 0.0, &cfg), 1); // clamped at MIN
}

#[test]
fn clamps_at_max() {
    let cfg = default_config();
    // At MAX(12), above-band rate stays at MAX
    assert_eq!(difficulty_retarget(12, 30.0, &cfg), 12);
    // At MAX(12), in-band stays
    assert_eq!(difficulty_retarget(12, 20.0, &cfg), 12);
}

#[test]
fn clamps_at_min() {
    let cfg = default_config();
    // At MIN(1), below-band rate stays at MIN
    assert_eq!(difficulty_retarget(1, 10.0, &cfg), 1);
    // At MIN(1), in-band stays
    assert_eq!(difficulty_retarget(1, 20.0, &cfg), 1);
}

#[test]
fn clamps_at_max_does_not_overflow() {
    let cfg = default_config();
    // At 11, above-band goes to 12 (MAX)
    assert_eq!(difficulty_retarget(11, 30.0, &cfg), 12);
    // At 12, above-band stays 12
    assert_eq!(difficulty_retarget(12, 30.0, &cfg), 12);
}

#[test]
fn clamps_at_min_does_not_underflow() {
    let cfg = default_config();
    // At 2, below-band goes to 1 (MIN)
    assert_eq!(difficulty_retarget(2, 10.0, &cfg), 1);
    // At 1, below-band stays 1
    assert_eq!(difficulty_retarget(1, 10.0, &cfg), 1);
}

// ---- Property tests ----

#[test]
fn property_output_always_within_bounds() {
    let cfg = default_config();
    for diff in 1..=12 {
        for rate in [0.0, 5.0, 10.0, 14.0, 15.0, 18.0, 20.0, 22.0, 25.0, 26.0, 30.0, 50.0, 100.0] {
            let new_diff = difficulty_retarget(diff, rate, &cfg);
            assert!(
                new_diff >= cfg.diff_min && new_diff <= cfg.diff_max,
                "diff={diff} rate={rate} → new_diff={new_diff} not in [{min},{max}]",
                min = cfg.diff_min,
                max = cfg.diff_max
            );
        }
    }
}

#[test]
fn property_step_never_exceeds_max_step() {
    let cfg = default_config();
    for diff in 1..=12 {
        for rate in [0.0, 5.0, 10.0, 14.0, 15.0, 18.0, 20.0, 22.0, 25.0, 26.0, 30.0, 50.0, 100.0] {
            let new_diff = difficulty_retarget(diff, rate, &cfg);
            let delta = (new_diff as i32 - diff as i32).unsigned_abs();
            assert!(
                delta <= cfg.max_step as u32,
                "diff={diff} rate={rate} → delta={delta} exceeds max_step={step}",
                step = cfg.max_step
            );
        }
    }
}
