use rand::Rng;

// SAFETY: All checked_arithmetic unwraps in this module are safe because operand
// ranges are bounded to prevent overflow for every level:
//   L1-L2: a,b ∈ [1,99], ± never overflows i64 (max 99+99=198, min 1-99=-98).
//   L3: a ∈ [10,99], b ∈ [2,9], max product 99×9=891.
//   L4: a ∈ [10,199], b ∈ [2,19], c ∈ [10,999], max product 199×19=3781.
//        ± with c: max 3781+999=4780, min when swapped for subtraction.
//   L5: a ∈ [10,99], b ∈ [2,19], c,d ∈ [10,99]×[2,19],
//        max product 99×19=1881, sum/sub within i64.
//        mod: divisor p ∈ [11,97], product 1881 fits, result < p.
//   L6+: scale = 10^(level-4), a ∈ [10, 99|scale×2], b ∈ [2, 19|scale/4].
//        For level 12: scale = 10^8 = 100_000_000.
//        max_a = max(99, 200_000_000), max_b = max(19, 25_000_000).
//        max product = 200_000_000 × 25_000_000 = 5e15 < i64::MAX (9.22e18).
//        Subtraction with swapped big/small keeps result non-negative.

/// Reward per difficulty level from the spec.
pub fn reward_for_level(level: u32) -> i64 {
    match level {
        1 => 5,
        2 => 10,
        3 => 20,
        4 => 40,
        5 => 80,
        n if n >= 6 => 160i64.saturating_mul(1i64 << (n - 6)),
        _ => 0,
    }
}

/// Generate a math challenge for the given difficulty level.
/// Returns (problem_string, solution, reward).
/// Uses checked arithmetic — never silently overflows.
pub fn generate_challenge(level: u32, rng: &mut impl Rng) -> (String, i64, i64) {
    let reward = reward_for_level(level);
    match level {
        1 => gen_level1(rng, reward),
        2 => gen_level2(rng, reward),
        3 => gen_level3(rng, reward),
        4 => gen_level4(rng, reward),
        5 => gen_level5(rng, reward),
        _ => gen_level6plus(level, rng, reward),
    }
}

// ---- Level 1: a ± b, single digit (1-9) ----

fn gen_level1(rng: &mut impl Rng, reward: i64) -> (String, i64, i64) {
    let a: i64 = rng.gen_range(1..=9);
    let b: i64 = rng.gen_range(1..=9);
    if rng.gen_bool(0.5) {
        let solution = a.checked_add(b).unwrap();
        (format!("{a} + {b}"), solution, reward)
    } else {
        let (big, small) = if a >= b { (a, b) } else { (b, a) };
        let solution = big.checked_sub(small).unwrap();
        (format!("{big} − {small}"), solution, reward)
    }
}

// ---- Level 2: a ± b, two digit (10-99) ----

fn gen_level2(rng: &mut impl Rng, reward: i64) -> (String, i64, i64) {
    let a: i64 = rng.gen_range(10..=99);
    let b: i64 = rng.gen_range(10..=99);
    if rng.gen_bool(0.5) {
        let solution = a.checked_add(b).unwrap();
        (format!("{a} + {b}"), solution, reward)
    } else {
        let (big, small) = if a >= b { (a, b) } else { (b, a) };
        let solution = big.checked_sub(small).unwrap();
        (format!("{big} − {small}"), solution, reward)
    }
}

// ---- Level 3: a × b, two-digit × single-digit ----

fn gen_level3(rng: &mut impl Rng, reward: i64) -> (String, i64, i64) {
    let a: i64 = rng.gen_range(10..=99);
    let b: i64 = rng.gen_range(2..=9);
    let solution = a.checked_mul(b).unwrap();
    (format!("{a} × {b}"), solution, reward)
}

// ---- Level 4: a × b ± c ----

fn gen_level4(rng: &mut impl Rng, reward: i64) -> (String, i64, i64) {
    let a: i64 = rng.gen_range(10..=199);
    let b: i64 = rng.gen_range(2..=19);
    let c: i64 = rng.gen_range(10..=999);
    let prod = a.checked_mul(b).unwrap();
    if rng.gen_bool(0.5) {
        let solution = prod.checked_add(c).unwrap();
        (format!("{a} × {b} + {c}"), solution, reward)
    } else if prod >= c {
        // SAFETY: guard ensures prod >= c
        let solution = prod.checked_sub(c).unwrap();
        (format!("{a} × {b} − {c}"), solution, reward)
    } else {
        let solution = c.checked_sub(prod).unwrap();
        (format!("{c} − {a} × {b}"), solution, reward)
    }
}

// ---- Level 5: (a × b) mod p  or  a×b − c×d ----

fn gen_level5(rng: &mut impl Rng, reward: i64) -> (String, i64, i64) {
    if rng.gen_bool(0.5) {
        gen_level5_mod(rng, reward)
    } else {
        gen_level5_mixed(rng, reward)
    }
}

fn gen_level5_mod(rng: &mut impl Rng, reward: i64) -> (String, i64, i64) {
    let a: i64 = rng.gen_range(10..=99);
    let b: i64 = rng.gen_range(2..=19);
    let p: i64 = rng.gen_range(11..=97);
    let prod = a.checked_mul(b).unwrap();
    let solution = prod.checked_rem(p).unwrap();
    (format!("({a} × {b}) mod {p}"), solution, reward)
}

fn gen_level5_mixed(rng: &mut impl Rng, reward: i64) -> (String, i64, i64) {
    let a: i64 = rng.gen_range(10..=99);
    let b: i64 = rng.gen_range(2..=19);
    let c: i64 = rng.gen_range(10..=99);
    let d: i64 = rng.gen_range(2..=19);
    let left = a.checked_mul(b).unwrap();
    let right = c.checked_mul(d).unwrap();
    if left >= right {
        // SAFETY: guard ensures left >= right
        let solution = left.checked_sub(right).unwrap();
        (format!("{a} × {b} − {c} × {d}"), solution, reward)
    } else {
        let solution = right.checked_sub(left).unwrap();
        (format!("{c} × {d} − {a} × {b}"), solution, reward)
    }
}

// ---- Level 6+: (a × b) − (c × d) with larger magnitudes ----

fn gen_level6plus(level: u32, rng: &mut impl Rng, reward: i64) -> (String, i64, i64) {
    let scale: i64 = 10i64.saturating_pow(level - 4);
    let max_a: i64 = 99i64.max(scale * 2);
    let max_b: i64 = 19i64.max(scale / 4);
    let a: i64 = rng.gen_range(10..=max_a);
    let b: i64 = rng.gen_range(2..=max_b);
    let c: i64 = rng.gen_range(10..=max_a);
    let d: i64 = rng.gen_range(2..=max_b);
    let left = a.checked_mul(b).unwrap();
    let right = c.checked_mul(d).unwrap();
    if left >= right {
        // SAFETY: guard ensures left >= right
        let solution = left.checked_sub(right).unwrap();
        (format!("({a} × {b}) − ({c} × {d})"), solution, reward)
    } else {
        let solution = right.checked_sub(left).unwrap();
        (format!("({c} × {d}) − ({a} × {b})"), solution, reward)
    }
}
