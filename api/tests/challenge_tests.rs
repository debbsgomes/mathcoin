/// Tests for generate_challenge — pure function, no DB, no HTTP.
use mathcoin_api::challenge::generate_challenge;
use rand::SeedableRng;

/// Independent re-evaluation of a problem string.
/// Parses the expression and computes the answer manually.
fn evaluate(problem: &str) -> i64 {
    let problem = problem.trim();
    // Level 5 mod: (a × b) mod p
    if problem.contains(" mod ") {
        let rest = problem.strip_prefix('(').unwrap_or(problem);
        let rest = rest.strip_suffix(')').unwrap_or(rest);
        let parts: Vec<&str> = rest.split(") mod ").collect();
        let expr = if parts.len() == 2 {
            parts[0].to_string()
        } else {
            rest.split(" mod ").next().unwrap().to_string()
        };
        let modulo: i64 = problem.split(" mod ").last().unwrap().parse().unwrap();
        return evaluate(&expr) % modulo;
    }
    // Parenthesized: (a × b) − (c × d)
    if problem.contains('(') && problem.contains(')') && !problem.contains("mod") {
        let without_parens = problem.replace('(', "").replace(')', "");
        return evaluate_flat(&without_parens);
    }
    evaluate_flat(problem)
}

fn evaluate_flat(expr: &str) -> i64 {
    let tokens: Vec<&str> = expr.split_whitespace().collect();
    if tokens.len() == 1 {
        return tokens[0].parse().unwrap();
    }
    if tokens.len() == 3 {
        let a: i64 = tokens[0].parse().unwrap();
        let op = tokens[1];
        let b: i64 = tokens[2].parse().unwrap();
        return apply_op(a, op, b);
    }
    if tokens.len() == 5 {
        // a op1 b op2 c — respect × before +/−
        let a: i64 = tokens[0].parse().unwrap();
        let op1 = tokens[1];
        let b: i64 = tokens[2].parse().unwrap();
        let op2 = tokens[3];
        let c: i64 = tokens[4].parse().unwrap();
        if precedence_order(op1, op2) == 0 {
            // op1 has higher or equal precedence: (a op1 b) op2 c
            let left = apply_op(a, op1, b);
            return apply_op(left, op2, c);
        } else {
            // op2 has higher precedence: a op1 (b op2 c)
            let right = apply_op(b, op2, c);
            return apply_op(a, op1, right);
        }
    }
    if tokens.len() == 7 {
        // a op1 b op2 c op3 d — × before +/−, left-to-right for same precedence
        let a: i64 = tokens[0].parse().unwrap();
        let op1 = tokens[1];
        let b: i64 = tokens[2].parse().unwrap();
        let op2 = tokens[3];
        let c: i64 = tokens[4].parse().unwrap();
        let op3 = tokens[5];
        let d: i64 = tokens[6].parse().unwrap();
        // Compute: (a op1 b) op2 (c op3 d), with × precedence
        let left = apply_op(a, op1, b);
        let right = apply_op(c, op3, d);
        return apply_op(left, op2, right);
    }
    panic!("unparseable ({n} tokens): {expr}", n = tokens.len());
}

/// Returns 0 if op1 should be evaluated before op2, 1 otherwise.
fn precedence_order(op1: &str, op2: &str) -> usize {
    let p1 = if op1 == "×" || op1 == "*" { 2 } else { 1 };
    let p2 = if op2 == "×" || op2 == "*" { 2 } else { 1 };
    if p1 >= p2 { 0 } else { 1 }
}

fn apply_op(a: i64, op: &str, b: i64) -> i64 {
    match op {
        "+" => a + b,
        "−" | "-" => a - b,
        "×" | "*" => a * b,
        _ => panic!("unknown op: {op}"),
    }
}

/// Reward per level from the spec
fn reward_for_level(level: u32) -> i64 {
    match level {
        1 => 5,
        2 => 10,
        3 => 20,
        4 => 40,
        5 => 80,
        n if n >= 6 => 160 * (1i64 << (n - 6)),
        _ => 0,
    }
}

// ---- Tests ----

#[test]
fn test_level_1_solution_matches_independent_evaluation() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    for _ in 0..20 {
        let (problem, solution, reward) = generate_challenge(1, &mut rng);
        assert_eq!(evaluate(&problem), solution, "mismatch for problem: {problem}");
        assert_eq!(reward, reward_for_level(1));
    }
}

#[test]
fn test_level_2_solution_matches_independent_evaluation() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    for _ in 0..20 {
        let (problem, solution, reward) = generate_challenge(2, &mut rng);
        assert_eq!(evaluate(&problem), solution, "mismatch for problem: {problem}");
        assert_eq!(reward, reward_for_level(2));
    }
}

#[test]
fn test_level_3_solution_matches_independent_evaluation() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    for _ in 0..20 {
        let (problem, solution, reward) = generate_challenge(3, &mut rng);
        assert_eq!(evaluate(&problem), solution, "mismatch for problem: {problem}");
        assert_eq!(reward, reward_for_level(3));
    }
}

#[test]
fn test_level_4_solution_matches_independent_evaluation() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    for _ in 0..20 {
        let (problem, solution, reward) = generate_challenge(4, &mut rng);
        assert_eq!(evaluate(&problem), solution, "mismatch for problem: {problem}");
        assert_eq!(reward, reward_for_level(4));
    }
}

#[test]
fn test_level_5_solution_matches_independent_evaluation() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    for _ in 0..20 {
        let (problem, solution, reward) = generate_challenge(5, &mut rng);
        assert_eq!(evaluate(&problem), solution, "mismatch for problem: {problem}");
        assert_eq!(reward, reward_for_level(5));
    }
}

#[test]
fn test_level_6_solution_matches_independent_evaluation() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    for _ in 0..20 {
        let (problem, solution, reward) = generate_challenge(6, &mut rng);
        assert_eq!(evaluate(&problem), solution, "mismatch for problem: {problem}");
        assert_eq!(reward, reward_for_level(6));
    }
}

#[test]
fn test_rewards_match_spec() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(99);
    for level in 1..=8 {
        let (_, _, reward) = generate_challenge(level, &mut rng);
        assert_eq!(reward, reward_for_level(level), "wrong reward for level {level}");
    }
}

#[test]
fn test_deterministic_output_for_fixed_seed() {
    let mut rng1 = rand::rngs::StdRng::seed_from_u64(12345);
    let mut rng2 = rand::rngs::StdRng::seed_from_u64(12345);

    for level in 1..=4 {
        let p1 = generate_challenge(level, &mut rng1);
        let p2 = generate_challenge(level, &mut rng2);
        assert_eq!(p1, p2, "deterministic output mismatch for level {level}");
    }
}

#[test]
fn test_no_panic_across_all_levels() {
    let mut rng = rand::rngs::StdRng::seed_from_u64(777);
    for level in 1..=12 {
        for _ in 0..50 {
            let (problem, solution, reward) = generate_challenge(level, &mut rng);
            // Verify solution is correct
            assert_eq!(evaluate(&problem), solution, "eval mismatch: {problem}");
            // Verify reward is within expected range
            assert!(reward > 0, "reward must be positive at level {level}");
        }
    }
}
