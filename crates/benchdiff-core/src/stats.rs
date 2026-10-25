//! Median and the two-sided Mann-Whitney U test, as benchstat uses for A/B comparisons.

/// Fewest samples per side for which p < 0.05 is attainable: with 3 vs 3 the
/// smallest two-sided p is 2/C(6,3) = 0.1.
pub const MIN_SAMPLES: usize = 4;

const EXACT_MAX: usize = 50;

pub fn median(values: &[f64]) -> f64 {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    match v.len() {
        0 => f64::NAN,
        n if n % 2 == 1 => v[n / 2],
        n => (v[n / 2 - 1] + v[n / 2]) / 2.0,
    }
}

/// Two-sided p-value that `a` and `b` come from the same distribution.
/// Exact when there are no ties and both sides have at most 50 samples;
/// otherwise the normal approximation with tie and continuity corrections.
pub fn mann_whitney_p(a: &[f64], b: &[f64]) -> f64 {
    p_value(a, b, true)
}

fn p_value(a: &[f64], b: &[f64], allow_exact: bool) -> f64 {
    let (n, m) = (a.len(), b.len());
    if n == 0 || m == 0 {
        return 1.0;
    }
    let mut all: Vec<(f64, bool)> = a
        .iter()
        .map(|&v| (v, true))
        .chain(b.iter().map(|&v| (v, false)))
        .collect();
    all.sort_by(|x, y| x.0.total_cmp(&y.0));

    // Average ranks over ties; collect tie-group sizes for the variance correction.
    let mut rank_sum_a = 0.0;
    let mut tie_term = 0.0;
    let mut has_ties = false;
    let mut i = 0;
    while i < all.len() {
        let mut j = i;
        while j + 1 < all.len() && all[j + 1].0 == all[i].0 {
            j += 1;
        }
        let t = (j - i + 1) as f64;
        if t > 1.0 {
            has_ties = true;
            tie_term += t * t * t - t;
        }
        let avg_rank = (i + j) as f64 / 2.0 + 1.0;
        rank_sum_a += all[i..=j].iter().filter(|x| x.1).count() as f64 * avg_rank;
        i = j + 1;
    }
    let nf = n as f64;
    let mf = m as f64;
    let u_a = rank_sum_a - nf * (nf + 1.0) / 2.0;
    let u_min = u_a.min(nf * mf - u_a);

    if allow_exact && !has_ties && n <= EXACT_MAX && m <= EXACT_MAX {
        let counts = u_distribution(n, m);
        let total: f64 = counts.iter().sum();
        let tail: f64 = counts[..=(u_min.round() as usize)].iter().sum();
        return (2.0 * tail / total).min(1.0);
    }

    let total = nf + mf;
    let var = nf * mf / 12.0 * ((total + 1.0) - tie_term / (total * (total - 1.0)));
    if var <= 0.0 {
        return 1.0;
    }
    let z = ((nf * mf / 2.0 - u_min - 0.5).max(0.0)) / var.sqrt();
    (2.0 * (1.0 - normal_cdf(z))).clamp(0.0, 1.0)
}

/// Number of rank arrangements giving each U in `0..=n*m`: the coefficients of
/// the Gaussian binomial `[n+m choose m]_q`, built as the product of
/// `(1 - q^(n+i)) / (1 - q^i)` for `i` in `1..=m`.
fn u_distribution(n: usize, m: usize) -> Vec<f64> {
    let len = n * m + 1;
    let mut poly = vec![0.0; len];
    poly[0] = 1.0;
    for i in 1..=m {
        let shift = n + i;
        for u in (shift..len).rev() {
            poly[u] -= poly[u - shift];
        }
        for u in i..len {
            poly[u] += poly[u - i];
        }
    }
    poly
}

fn normal_cdf(z: f64) -> f64 {
    0.5 * erfc(-z / std::f64::consts::SQRT_2)
}

/// Complementary error function, Numerical Recipes 6.2 (fractional error < 1.2e-7).
fn erfc(x: f64) -> f64 {
    let z = x.abs();
    let t = 1.0 / (1.0 + 0.5 * z);
    let r = t
        * (-z * z - 1.26551223
            + t * (1.00002368
                + t * (0.37409196
                    + t * (0.09678418
                        + t * (-0.18628806
                            + t * (0.27886807
                                + t * (-1.13520398
                                    + t * (1.48851587 + t * (-0.82215223 + t * 0.17087277)))))))))
            .exp();
    if x >= 0.0 {
        r
    } else {
        2.0 - r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_should_average_the_middle_pair_for_even_counts() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), 2.5);
    }

    #[test]
    fn fully_separated_5_vs_5_has_the_textbook_exact_p() {
        // 2 / C(10,5) = 2/252 = 0.0079365 (scipy.stats.mannwhitneyu agrees).
        let p = mann_whitney_p(&[1.0, 2.0, 3.0, 4.0, 5.0], &[6.0, 7.0, 8.0, 9.0, 10.0]);
        assert!((p - 2.0 / 252.0).abs() < 1e-12, "{p}");
    }

    #[test]
    fn three_vs_three_can_never_reach_alpha_005() {
        let p = mann_whitney_p(&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]);
        assert!((p - 0.1).abs() < 1e-12, "{p}");
    }

    #[test]
    fn four_vs_four_separated_is_just_significant() {
        let p = mann_whitney_p(&[1.0, 2.0, 3.0, 4.0], &[5.0, 6.0, 7.0, 8.0]);
        assert!((p - 2.0 / 70.0).abs() < 1e-12 && p < 0.05, "{p}");
    }

    #[test]
    fn interleaved_samples_are_not_significant() {
        let p = mann_whitney_p(&[1.0, 3.0, 5.0, 7.0, 9.0], &[2.0, 4.0, 6.0, 8.0, 10.0]);
        assert!(p > 0.5, "{p}");
    }

    #[test]
    fn p_is_symmetric_in_its_arguments() {
        let a = [1.0, 2.0, 2.5, 9.0, 4.0];
        let b = [3.0, 6.0, 7.0, 8.0, 5.5, 5.0];
        assert!((mann_whitney_p(&a, &b) - mann_whitney_p(&b, &a)).abs() < 1e-12);
    }

    #[test]
    fn ties_fall_back_to_the_normal_approximation() {
        // Identical samples: no evidence of a difference.
        let p = mann_whitney_p(&[1.0; 8], &[1.0; 8]);
        assert_eq!(p, 1.0);
        // Heavily tied but clearly shifted: significant.
        let a = [1.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 2.0];
        let b = [5.0, 5.0, 5.0, 5.0, 6.0, 6.0, 6.0, 6.0];
        assert!(mann_whitney_p(&a, &b) < 0.01);
    }

    #[test]
    fn an_empty_side_gives_p_of_one() {
        assert_eq!(mann_whitney_p(&[], &[1.0, 2.0]), 1.0);
        assert_eq!(mann_whitney_p(&[1.0, 2.0], &[]), 1.0);
    }

    #[test]
    fn fifty_one_vs_fifty_one_uses_the_normal_approximation() {
        let a: Vec<f64> = (0..51).map(f64::from).collect();
        let b: Vec<f64> = (25..76).map(f64::from).collect();
        let p = mann_whitney_p(&a, &b);
        assert!(p > 0.0 && p < 0.05, "{p}");
    }

    #[test]
    fn normal_approximation_tracks_the_exact_value_for_moderate_n() {
        let a: Vec<f64> = (0..20).map(|i| i as f64 * 2.0).collect();
        let b: Vec<f64> = (0..20).map(|i| i as f64 * 2.0 + 7.5).collect();
        let (exact, approx) = (p_value(&a, &b, true), p_value(&a, &b, false));
        assert!((exact - approx).abs() < 0.01, "{exact} vs {approx}");
    }
}
