//! Special functions and deterministic generator.
//!
//! Nothing here uses magic tables without provenance: `erfc` comes from `libm`
//! (musl implementation, documented precision) and the Normal quantile is
//! solved numerically over that CDF, instead of using a polynomial
//! approximation whose constants would have to be copied from a source.

use std::f64::consts::{FRAC_1_SQRT_2, PI, SQRT_2};

/// Complementary error function, `erfc(x) = 1 − erf(x)`.
#[must_use]
pub fn erfc(x: f64) -> f64 {
    libm::erfc(x)
}

/// Density of the standard Normal.
#[must_use]
pub fn normal_pdf(z: f64) -> f64 {
    FRAC_1_SQRT_2 / PI.sqrt() * (-0.5 * z * z).exp()
}

/// CDF of the standard Normal, `Φ(z) = ½·erfc(−z/√2)`.
///
/// Uses `erfc` instead of `0.5 * (1 + erf(z/√2))` to preserve relative
/// precision in the lower tail.
#[must_use]
pub fn normal_cdf(z: f64) -> f64 {
    0.5 * erfc(-z / SQRT_2)
}

/// Upper tail of the standard Normal, `1 − Φ(z)`.
#[must_use]
pub fn normal_sf(z: f64) -> f64 {
    0.5 * erfc(z / SQRT_2)
}

/// Quantile of the standard Normal, `Φ⁻¹(p)`.
///
/// Solved by bisection over [`normal_cdf`] to machine precision.
/// It is slower than a rational approximation, but it always converges and
/// does not depend on unverified constants; the cost is irrelevant outside
/// loops with millions of calls (and even there, ~60 iterations per quantile).
///
/// Returns `−∞` for `p ≤ 0`, `+∞` for `p ≥ 1` and `NaN` for non-finite `p`.
#[must_use]
pub fn normal_quantile(p: f64) -> f64 {
    if p.is_nan() {
        return f64::NAN;
    }
    if p <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }

    // [-40, 40] covers Φ⁻¹ down to ~1e-350, far beyond f64 precision.
    let mut lo = -40.0f64;
    let mut hi = 40.0f64;
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if normal_cdf(mid) < p {
            lo = mid;
        } else {
            hi = mid;
        }
        if (hi - lo) <= 1e-15 * (1.0 + mid.abs()) {
            break;
        }
    }
    0.5 * (lo + hi)
}

/// Natural logarithm of the Gamma function, `ln Γ(x)`.
#[must_use]
pub fn ln_gamma(x: f64) -> f64 {
    libm::lgamma(x)
}

/// Regularized lower incomplete gamma, `P(a, x)`.
///
/// Series for `x < a + 1`, continued fraction otherwise; used by the
/// chi-square distribution and the capability intervals.
#[must_use]
pub fn regularized_gamma_lower(a: f64, x: f64) -> f64 {
    if a <= 0.0 || !a.is_finite() || x < 0.0 || x.is_nan() {
        return f64::NAN;
    }
    if x == 0.0 {
        return 0.0;
    }
    let ln_prefix = -x + a * x.ln() - ln_gamma(a);
    if x < a + 1.0 {
        let mut term = 1.0 / a;
        let mut sum = term;
        let mut ap = a;
        for _ in 0..500 {
            ap += 1.0;
            term *= x / ap;
            sum += term;
            if term.abs() < sum.abs() * 3e-16 {
                break;
            }
        }
        (ln_prefix + sum.ln()).exp()
    } else {
        1.0 - (ln_prefix + gamma_continued_fraction(a, x).ln()).exp()
    }
}

fn gamma_continued_fraction(a: f64, x: f64) -> f64 {
    const MAX_ITER: usize = 500;
    const EPS: f64 = 3e-16;
    const TINY: f64 = 1e-300;

    let mut b = x + 1.0 - a;
    let mut c = 1.0 / TINY;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..=MAX_ITER {
        let an = -(i as f64) * (i as f64 - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < TINY {
            d = TINY;
        }
        c = b + an / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        let delta = d * c;
        h *= delta;
        if (delta - 1.0).abs() < EPS {
            break;
        }
    }
    h
}

/// Regularized upper incomplete gamma, `Q(a, x) = 1 − P(a, x)`.
///
/// Mirrors [`regularized_gamma_lower`] but keeps the exponential form in the
/// region where the continued fraction is used, so the upper tail does not
/// lose precision to cancellation.
#[must_use]
pub fn regularized_gamma_upper(a: f64, x: f64) -> f64 {
    if a <= 0.0 || !a.is_finite() || x < 0.0 || x.is_nan() {
        return f64::NAN;
    }
    if x == 0.0 {
        return 1.0;
    }
    let ln_prefix = -x + a * x.ln() - ln_gamma(a);
    if x < a + 1.0 {
        1.0 - regularized_gamma_lower(a, x)
    } else {
        (ln_prefix + gamma_continued_fraction(a, x).ln()).exp()
    }
}

/// Digamma function, `ψ(x) = d/dx ln Γ(x)`.
///
/// Recurrence `ψ(x) = ψ(x + 1) − 1/x` up to `x ≥ 20`, then the asymptotic
/// series `ln x − 1/(2x) − Σ B₂ₖ/(2k x²ᵏ)` truncated at the `x⁻¹⁰` term
/// (the first omitted term is below `1e-19` there). Used by the Gamma
/// maximum-likelihood score equation.
#[must_use]
pub fn digamma(mut x: f64) -> f64 {
    if !x.is_finite() || x <= 0.0 {
        return f64::NAN;
    }
    let mut shift = 0.0;
    while x < 20.0 {
        shift -= 1.0 / x;
        x += 1.0;
    }
    let inv = 1.0 / x;
    let inv2 = inv * inv;
    let series = inv2
        * (1.0 / 12.0
            - inv2 * (1.0 / 120.0 - inv2 * (1.0 / 252.0 - inv2 * (1.0 / 240.0 - inv2 / 132.0))));
    shift + x.ln() - 0.5 * inv - series
}

/// Trigamma function, `ψ'(x) = d²/dx² ln Γ(x)`.
///
/// Recurrence `ψ'(x) = ψ'(x + 1) + 1/x²` up to `x ≥ 20`, then the asymptotic
/// series `1/x + 1/(2x²) + Σ B₂ₖ/x²ᵏ⁺¹` truncated at the `x⁻¹¹` term.
#[must_use]
pub fn trigamma(mut x: f64) -> f64 {
    if !x.is_finite() || x <= 0.0 {
        return f64::NAN;
    }
    let mut shift = 0.0;
    while x < 20.0 {
        shift += 1.0 / (x * x);
        x += 1.0;
    }
    let inv = 1.0 / x;
    let inv2 = inv * inv;
    let series = inv
        * inv2
        * (1.0 / 6.0
            - inv2 * (1.0 / 30.0 - inv2 * (1.0 / 42.0 - inv2 * (1.0 / 30.0 - 5.0 * inv2 / 66.0))));
    shift + inv + 0.5 * inv2 + series
}

/// Quantile of the Beta distribution.
///
/// Bisection over [`regularized_beta`]. Used by the exact binomial
/// (Clopper–Pearson) interval and by the F quantile.
#[must_use]
pub fn beta_quantile(p: f64, a: f64, b: f64) -> f64 {
    if p.is_nan() || a <= 0.0 || b <= 0.0 || !a.is_finite() || !b.is_finite() {
        return f64::NAN;
    }
    if p <= 0.0 {
        return 0.0;
    }
    if p >= 1.0 {
        return 1.0;
    }
    let mut lo = 0.0;
    let mut hi = 1.0;
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if regularized_beta(mid, a, b) < p {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo < 1e-14 {
            break;
        }
    }
    0.5 * (lo + hi)
}

/// CDF of the F distribution with `d1` and `d2` degrees of freedom.
///
/// `F(x) = I_{d1·x/(d1·x + d2)}(d1/2, d2/2)`.
#[must_use]
pub fn f_cdf(x: f64, d1: f64, d2: f64) -> f64 {
    if d1 <= 0.0 || d2 <= 0.0 || !d1.is_finite() || !d2.is_finite() || x.is_nan() {
        return f64::NAN;
    }
    if x <= 0.0 {
        return 0.0;
    }
    regularized_beta(d1 * x / (d1 * x + d2), 0.5 * d1, 0.5 * d2)
}

/// Quantile of the F distribution.
///
/// The Beta quantile is transformed by `x = (d2/d1)·y/(1−y)`.
#[must_use]
pub fn f_quantile(p: f64, d1: f64, d2: f64) -> f64 {
    if p.is_nan() || d1 <= 0.0 || d2 <= 0.0 {
        return f64::NAN;
    }
    if p <= 0.0 {
        return 0.0;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    let y = beta_quantile(p, 0.5 * d1, 0.5 * d2);
    if y >= 1.0 {
        return f64::INFINITY;
    }
    d2 / d1 * y / (1.0 - y)
}

/// CDF of the chi-square distribution with `df` degrees of freedom.
#[must_use]
pub fn chi_square_cdf(x: f64, df: f64) -> f64 {
    if df <= 0.0 || !df.is_finite() || x < 0.0 || x.is_nan() {
        return f64::NAN;
    }
    regularized_gamma_lower(0.5 * df, 0.5 * x)
}

/// Quantile of the chi-square distribution with `df` degrees of freedom.
///
/// Bisection over [`chi_square_cdf`], like the Student-t quantile. Used only
/// for capability intervals, where a few dozen evaluations are negligible.
#[must_use]
pub fn chi_square_quantile(p: f64, df: f64) -> f64 {
    if p.is_nan() || df <= 0.0 || !df.is_finite() {
        return f64::NAN;
    }
    if p <= 0.0 {
        return 0.0;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    let mut lo = 0.0;
    let mut hi = df.max(1.0);
    while chi_square_cdf(hi, df) < p && hi < 1e15 {
        hi *= 2.0;
    }
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if chi_square_cdf(mid, df) < p {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo < 1e-12 {
            break;
        }
    }
    0.5 * (lo + hi)
}

/// Regularized incomplete beta, `I_x(a, b)`.
///
/// Continued fraction (Lentz), the standard stable evaluation; used by the
/// Student t CDF. Returns `NaN` for invalid parameters.
#[must_use]
pub fn regularized_beta(x: f64, a: f64, b: f64) -> f64 {
    if a <= 0.0 || b <= 0.0 || !a.is_finite() || !b.is_finite() {
        return f64::NAN;
    }
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let ln_bt = ln_gamma(a + b) - ln_gamma(a) - ln_gamma(b) + a * x.ln() + b * (1.0 - x).ln();
    let bt = ln_bt.exp();
    if x < (a + 1.0) / (a + b + 2.0) {
        bt * beta_continued_fraction(a, b, x) / a
    } else {
        1.0 - bt * beta_continued_fraction(b, a, 1.0 - x) / b
    }
}

fn beta_continued_fraction(a: f64, b: f64, x: f64) -> f64 {
    const MAX_ITER: usize = 300;
    const EPS: f64 = 3e-16;
    const TINY: f64 = 1e-300;

    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < TINY {
        d = TINY;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..=MAX_ITER {
        let mf = m as f64;
        let m2 = 2.0 * mf;
        let aa = mf * (b - mf) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < TINY {
            d = TINY;
        }
        c = 1.0 + aa / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        h *= d * c;

        let aa = -(a + mf) * (qab + mf) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < TINY {
            d = TINY;
        }
        c = 1.0 + aa / c;
        if c.abs() < TINY {
            c = TINY;
        }
        d = 1.0 / d;
        let delta = d * c;
        h *= delta;
        if (delta - 1.0).abs() < EPS {
            break;
        }
    }
    h
}

/// CDF of Student's t with `df` degrees of freedom.
///
/// `P(T ≤ t) = 1 − ½ I_{ν/(ν+t²)}(ν/2, ½)` for `t ≥ 0`, by symmetry below.
#[must_use]
pub fn student_t_cdf(t: f64, df: f64) -> f64 {
    if df <= 0.0 || !df.is_finite() || t.is_nan() {
        return f64::NAN;
    }
    let x = df / (df + t * t);
    let tail = 0.5 * regularized_beta(x, 0.5 * df, 0.5);
    if t >= 0.0 { 1.0 - tail } else { tail }
}

/// Quantile of Student's t with `df` degrees of freedom.
///
/// Bisection over [`student_t_cdf`]: the CDF is cheap enough for a plot band
/// and this keeps the implementation free of hard-coded coefficient tables.
/// Returns `±∞` at the boundaries and `NaN` for invalid `df`.
#[must_use]
pub fn student_t_quantile(p: f64, df: f64) -> f64 {
    if p.is_nan() || df <= 0.0 || !df.is_finite() {
        return f64::NAN;
    }
    if p <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    if (p - 0.5).abs() < 1e-15 {
        return 0.0;
    }
    if p < 0.5 {
        return -student_t_quantile(1.0 - p, df);
    }

    let mut lo = 0.0;
    let mut hi = 1.0;
    while student_t_cdf(hi, df) < p && hi < 1e12 {
        hi *= 2.0;
    }
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if student_t_cdf(mid, df) < p {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo < 1e-13 {
            break;
        }
    }
    0.5 * (lo + hi)
}

/// Probability that the range of `groups` independent standard normals is at
/// most `r`.
///
/// `P(W ≤ r) = k ∫ φ(z)·[Φ(z) − Φ(z − r)]^{k−1} dz`, evaluated with Simpson's
/// rule on a window wide enough to hold the whole mass (the integrand decays
/// far faster than the window growth with `r`).
fn range_cdf(r: f64, groups: f64) -> f64 {
    if r <= 0.0 {
        return 0.0;
    }
    if !r.is_finite() {
        return 1.0;
    }
    let lo = -10.0 - 0.5 * r;
    let hi = 10.0 + 0.5 * r;
    let steps = 120;
    let h = (hi - lo) / steps as f64;
    let mut sum = 0.0;
    for i in 0..=steps {
        let z = lo + i as f64 * h;
        let bracket = normal_cdf(z) - normal_cdf(z - r);
        let value = normal_pdf(z) * bracket.powf(groups - 1.0);
        let weight = if i == 0 || i == steps {
            1.0
        } else if i % 2 == 1 {
            4.0
        } else {
            2.0
        };
        sum += weight * value;
    }
    (groups * sum * h / 3.0).clamp(0.0, 1.0)
}

/// Density of `S = sqrt(χ²_ν / ν)` at `s`.
fn scaled_chi_density(s: f64, df: f64) -> f64 {
    if s <= 0.0 {
        return 0.0;
    }
    let half = 0.5 * df;
    let log = half * df.ln() + (df - 1.0) * s.ln()
        - 0.5 * df * s * s
        - (half - 1.0) * std::f64::consts::LN_2
        - ln_gamma(half);
    log.exp()
}

/// CDF of the studentized range `Q = W / S` with `groups` means and `df`
/// error degrees of freedom.
///
/// `P(Q ≤ q) = ∫ f_S(s)·P(W ≤ q·s) ds`, with `S = sqrt(χ²_df/df)`; the outer
/// integral runs to the 1 − 10⁻¹⁰ quantile of the chi-square, so the tail is
/// negligible. Returns `NaN` for invalid arguments.
#[must_use]
pub fn studentized_range_cdf(q: f64, groups: f64, df: f64) -> f64 {
    if q.is_nan() || groups < 2.0 || df < 1.0 || !groups.is_finite() || !df.is_finite() {
        return f64::NAN;
    }
    if q <= 0.0 {
        return 0.0;
    }
    let upper = (chi_square_quantile(1.0 - 1e-10, df) / df).sqrt();
    let steps = 160;
    let h = upper / steps as f64;
    let mut sum = 0.0;
    for i in 0..=steps {
        let s = i as f64 * h;
        let density = scaled_chi_density(s, df);
        if density == 0.0 {
            continue;
        }
        let value = density * range_cdf(q * s, groups);
        let weight = if i == 0 || i == steps {
            1.0
        } else if i % 2 == 1 {
            4.0
        } else {
            2.0
        };
        sum += weight * value;
    }
    (sum * h / 3.0).clamp(0.0, 1.0)
}

/// Quantile of the studentized range, by bisection over
/// [`studentized_range_cdf`].
///
/// Returns `∞` at `p ≥ 1`, `0` at `p ≤ 0` and `NaN` for invalid arguments.
#[must_use]
pub fn studentized_range_quantile(p: f64, groups: f64, df: f64) -> f64 {
    if p.is_nan() || groups < 2.0 || df < 1.0 || !groups.is_finite() || !df.is_finite() {
        return f64::NAN;
    }
    if p <= 0.0 {
        return 0.0;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    let mut lo = 0.0;
    let mut hi = 6.0;
    while studentized_range_cdf(hi, groups, df) < p && hi < 1e6 {
        hi *= 2.0;
    }
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if studentized_range_cdf(mid, groups, df) < p {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo < 1e-4 {
            break;
        }
    }
    0.5 * (lo + hi)
}

/// `SplitMix64` generator (Steele, Lea & Flood, 2014).
///
/// Implemented here, and not in a dependency, for two reasons: Monte Carlo
/// must be reproducible byte-for-byte across machines and versions, and the
/// entire algorithm fits in five auditable lines.
#[derive(Debug, Clone)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Creates the generator with a fixed seed.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Next 64-bit integer.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Next value in `[0, 1)` with 53 bits of mantissa.
    pub fn next_f64(&mut self) -> f64 {
        // 2^53 exact divisions; never returns 1.0.
        ((self.next_u64() >> 11) as f64) * (1.0 / 9_007_199_254_740_992.0)
    }

    /// Next value from a standard Normal (Box–Muller).
    pub fn next_normal(&mut self) -> f64 {
        let mut u = self.next_f64();
        if u <= 0.0 {
            // next_f64() may return exactly 0; ln(0) = −inf.
            u = f64::MIN_POSITIVE;
        }
        let v = self.next_f64();
        (-2.0 * u.ln()).sqrt() * (2.0 * PI * v).cos()
    }
}
