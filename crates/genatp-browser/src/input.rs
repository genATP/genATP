//! Human-cadence input simulation — Bezier mouse paths and gamma-distributed
//! typing delays (Phase 11).
//!
//! ## Motivation
//!
//! Bot-detection services watch mouse movement patterns and typing cadence.
//! Automated tools typically:
//!
//! - Move the mouse in straight lines between targets (or teleport instantly).
//! - Type with uniform, machine-perfect inter-key delays.
//! - Click at the exact centre of every element.
//!
//! This module replaces those behaviours with statistically realistic ones:
//!
//! - **Bezier curves** — mouse paths that arc and overshoot like a human arm.
//! - **Gamma-distributed typing delays** — inter-key intervals matching human
//!   typing rhythm (mean ~80 ms, shape k=2.0).
//! - **Click offset jitter** — normally distributed offsets from element centre,
//!   clamped to the element bounds.
//!
//! ## Architecture
//!
//! [`HumanInputSimulator`] bundles a PRNG, a `fastrand::Rng`, and methods
//! to generate movement paths and typing sequences. The orchestrator calls
//! [`generate_mouse_path`] before each click and [`generate_typing_sequence`]
//! before each fill, then dispatches the individual CDP commands through the
//! browser adapter.
//!
//! ## Integration
//!
//! The simulator is **stateless** from the browser's perspective — it
//! generates coordinate and timing data, and the caller decides how to
//! dispatch them. This keeps the simulator testable without a browser.

use serde::{Deserialize, Serialize};

/// A single mouse movement point with timing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MousePoint {
    /// X coordinate in CSS pixels (viewport-relative).
    pub x: f64,
    /// Y coordinate in CSS pixels (viewport-relative).
    pub y: f64,
    /// Timestamp in milliseconds since the start of the gesture.
    pub t_ms: f64,
}

/// A single key-down/key-up pair with timing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct KeyEvent {
    /// Character to type (single char or key name like `"Enter"`).
    pub key: char,
    /// Delay in ms before this key-down fires (relative to the previous
    /// key-up).
    pub delay_ms: f64,
}

/// Configuration for the human input simulator.
#[derive(Debug, Clone, Copy)]
pub struct InputSimConfig {
    /// Typical mouse movement speed in px/ms (default 0.5 ≈ 500 px/s).
    pub mouse_speed_px_per_ms: f64,
    /// Speed variance (stddev) as a fraction of `mouse_speed` (default 0.3).
    pub mouse_speed_variance: f64,
    /// Number of intermediate points to generate on a mouse path (min).
    pub min_path_points: u32,
    /// Number of intermediate points to generate on a mouse path (max).
    pub max_path_points: u32,
    /// Click offset jitter stddev in CSS pixels (default 2.0).
    pub click_jitter_px: f64,
    /// Mean inter-key delay in ms for typing (default 80 ms ≈ 12 WPS).
    pub typing_mean_ms: f64,
    /// Shape parameter for the gamma distribution (default 2.0).
    pub typing_shape: f64,
}

impl Default for InputSimConfig {
    fn default() -> Self {
        Self {
            mouse_speed_px_per_ms: 0.5,
            mouse_speed_variance: 0.3,
            min_path_points: 8,
            max_path_points: 25,
            click_jitter_px: 2.0,
            typing_mean_ms: 80.0,
            typing_shape: 2.0,
        }
    }
}

/// Human input simulator — generates realistic mouse paths and typing
/// sequences.
#[derive(Debug, Clone)]
pub struct HumanInputSimulator {
    rng: fastrand::Rng,
    config: InputSimConfig,
}

impl HumanInputSimulator {
    /// Create a new simulator with the given config and random seed.
    #[must_use]
    pub fn new(config: InputSimConfig) -> Self {
        Self {
            rng: fastrand::Rng::new(),
            config,
        }
    }

    /// Create a simulator with a specific seed (for reproducible tests).
    #[must_use]
    pub fn with_seed(config: InputSimConfig, seed: u64) -> Self {
        Self {
            rng: fastrand::Rng::with_seed(seed),
            config,
        }
    }

    /// Generate a cubic Bezier mouse path from `(x0, y0)` to `(x1, y1)`.
    ///
    /// Returns a sequence of [`MousePoint`]s including the start and end
    /// points. The number of intermediate points is randomised within
    /// `[min_path_points, max_path_points]`.
    ///
    /// The path is a cubic Bezier with randomised control points that create
    /// a natural-looking arc (slight overshoot on one axis, varying curvature).
    #[must_use]
    pub fn generate_mouse_path(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<MousePoint> {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let dist = (dx * dx + dy * dy).sqrt();

        // No movement, or sub-pixel.
        if dist < 1.0 {
            return vec![MousePoint {
                x: x1,
                y: y1,
                t_ms: 0.0,
            }];
        }

        // Generate two control points for the cubic Bezier.
        // We add noise perpendicular to the movement direction.
        let perp_x = -dy / dist.max(0.001);
        let perp_y = dx / dist.max(0.001);

        let mid = 0.3 + self.rng.f64() * 0.4; // control point distance fraction 0.3-0.7
        let spread = dist * 0.1 * (0.5 + self.rng.f64()); // perpendicular spread

        let cp1 = (
            x0 + dx * mid + perp_x * spread,
            y0 + dy * mid + perp_y * spread,
        );
        let cp2 = (
            x0 + dx * (1.0 - mid * 0.5) - perp_x * spread * 0.7,
            y0 + dy * (1.0 - mid * 0.5) - perp_y * spread * 0.7,
        );

        // Number of segments.
        let n_segments = self
            .rng
            .u32(self.config.min_path_points..=self.config.max_path_points)
            .max(2);

        // Speed with variance.
        let speed = self.config.mouse_speed_px_per_ms
            * (1.0 + self.rng.f64() * self.config.mouse_speed_variance * 2.0
                - self.config.mouse_speed_variance);

        let mut points = Vec::with_capacity(n_segments as usize + 1);
        let mut prev_x = x0;
        let mut prev_y = y0;
        let mut t_acc = 0.0;

        points.push(MousePoint {
            x: x0,
            y: y0,
            t_ms: 0.0,
        });

        for i in 1..=n_segments {
            let t = f64::from(i) / f64::from(n_segments);

            // Cubic Bezier: B(t) = (1-t)^3 P0 + 3(1-t)^2 t P1 + 3(1-t) t^2 P2 + t^3 P3
            let u = 1.0 - t;
            let px =
                u * u * u * x0 + 3.0 * u * u * t * cp1.0 + 3.0 * u * t * t * cp2.0 + t * t * t * x1;
            let py =
                u * u * u * y0 + 3.0 * u * u * t * cp1.1 + 3.0 * u * t * t * cp2.1 + t * t * t * y1;

            let seg_dx = px - prev_x;
            let seg_dy = py - prev_y;
            let seg_dist = (seg_dx * seg_dx + seg_dy * seg_dy).sqrt();
            let seg_time = seg_dist / speed.max(0.01);

            t_acc += seg_time;
            prev_x = px;
            prev_y = py;

            points.push(MousePoint {
                x: px,
                y: py,
                t_ms: t_acc,
            });
        }

        // Ensure the last point is exactly the target.
        if let Some(last) = points.last_mut() {
            last.x = x1;
            last.y = y1;
        }

        points
    }

    /// Generate a click offset jitter around `(cx, cy)`. Returns the
    /// offset point (normally distributed within ~±2σ of element centre).
    #[must_use]
    pub fn jitter_click_point(&mut self, cx: f64, cy: f64) -> (f64, f64) {
        let jx = gaussian_sample(&mut self.rng) * self.config.click_jitter_px;
        let jy = gaussian_sample(&mut self.rng) * self.config.click_jitter_px;
        (cx + jx, cy + jy)
    }

    /// Generate inter-key typing delays for a string of length `n`.
    ///
    /// Returns a vector of `n + 1` delays: the first is the initial pause
    /// before typing begins (thinking time), then one delay per character.
    /// Delays are drawn from a gamma distribution with the configured shape
    /// and mean.
    #[must_use]
    pub fn generate_typing_sequence(&mut self, text: &str) -> Vec<f64> {
        let n = text.chars().count();
        if n == 0 {
            return vec![0.0];
        }

        // Initial "thinking" pause: 200–600 ms log-normal.
        let initial_pause = 200.0 + self.rng.f64() * 400.0;

        let mut delays = Vec::with_capacity(n + 1);
        delays.push(initial_pause);

        for _ in 0..n {
            let delay = gamma_sample(&mut self.rng, self.config.typing_shape)
                * self.config.typing_mean_ms
                / self.config.typing_shape;
            // Clamp to reasonable range [20, 300] ms.
            let clamped = delay.clamp(20.0, 300.0);
            delays.push(clamped);
        }

        delays
    }

    /// Accelerate typing on common bigrams / trigrams (e.g. "th", "he",
    /// "ing", "ed") by reducing the delay by ~30%. This makes the typing
    /// pattern more human-like.
    pub fn apply_bigram_acceleration(&self, text: &str, delays: &mut [f64]) {
        if delays.len() < 3 {
            return;
        }
        // Characters from index 1 onward correspond to delays[1..].
        let chars: Vec<char> = text.chars().collect();
        for i in 1..chars.len().min(delays.len()) {
            // Check bigram: chars[i-1] + chars[i]
            let bigram = format!("{}{}", chars[i - 1], chars[i]);
            match bigram.as_str() {
                "th" | "he" | "in" | "er" | "an" | "re" | "ed" | "on" | "es" | "st" | "en"
                | "at" | "to" | "nt" | "ha" | "nd" | "ou" | "ea" | "ng" | "te" | "or" | "ti"
                | "as" | "ar" | "al" | "le" | "se" | "de" | "ve" | "ra" | "ri" | "ro" | "ic"
                | "co" | "me" | "si" | "li" | "di" | "la" | "lo" | "ta" | "fo" | "hi" | "ne"
                | "wa" | "wh" | "fi" | "pr" | "io" => {
                    delays[i] *= 0.7; // 30% faster on common bigrams
                }
                _ => {}
            }
            // Check trigram: chars[i-1] + chars[i] may form a common pattern
            if i >= 2 {
                let trigram = format!("{}{}{}", chars[i - 2], chars[i - 1], chars[i]);
                match trigram.as_str() {
                    "the" | "ing" | "and" | "ion" | "tio" | "thi" | "ght" | "for" | "hat"
                    | "ent" | "ere" | "ter" | "tha" | "all" | "ith" | "you" | "ple" | "are"
                    | "tin" | "not" | "ill" | "ess" | "men" => {
                        delays[i] *= 0.6; // 40% faster on common trigrams
                    }
                    _ => {}
                }
            }
            // Keep delays within bounds.
            delays[i] = delays[i].clamp(15.0, 300.0);
        }
    }

    /// Convenience: generate a full typing sequence with bigram acceleration.
    #[must_use]
    pub fn generate_typing_delays(&mut self, text: &str) -> Vec<f64> {
        let mut delays = self.generate_typing_sequence(text);
        self.apply_bigram_acceleration(text, &mut delays);
        delays
    }

    /// Return the element-relative offset for a click given the element's
    /// bounding box `(cx, cy, width, height)`. The offset is normally
    /// distributed around the centre, clamped to element bounds.
    #[must_use]
    pub fn click_offset_in_element(
        &mut self,
        cx: f64,
        cy: f64,
        width: f64,
        height: f64,
    ) -> (f64, f64) {
        let (jx, jy) = self.jitter_click_point(cx, cy);
        let half_w = width / 2.0;
        let half_h = height / 2.0;
        (
            jx.clamp(cx - half_w, cx + half_w),
            jy.clamp(cy - half_h, cy + half_h),
        )
    }
}

// ---------------------------------------------------------------------------
// Statistical sampling helpers
// ---------------------------------------------------------------------------

/// Sample from a standard normal distribution (Box-Muller transform).
fn gaussian_sample(rng: &mut fastrand::Rng) -> f64 {
    let u1: f64 = rng.f64();
    let u2: f64 = rng.f64();
    let z0 = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
    // Discard z1; we only need one sample.
    z0
}

/// Sample from a gamma distribution with shape `k` (scale = 1.0).
/// Uses Marsaglia & Tsang's method for k >= 1.
fn gamma_sample(rng: &mut fastrand::Rng, k: f64) -> f64 {
    if k < 1.0 {
        // For k < 1, use the bootstrap method: sample from Gamma(k+1, 1) * U^(1/k).
        return gamma_sample(rng, k + 1.0) * rng.f64().powf(1.0 / k);
    }

    // Marsaglia & Tsang for k >= 1.
    let d = k - 1.0 / 3.0;
    let c = 1.0 / (9.0 * d).sqrt();

    loop {
        let mut x;
        let mut v;
        loop {
            x = gaussian_sample(rng);
            v = 1.0 + c * x;
            if v > 0.0 {
                break;
            }
        }
        let v_cubed = v * v * v;
        let u: f64 = rng.f64();
        if u < 1.0 - 0.0331 * (x * x) * (x * x) {
            return d * v_cubed;
        }
        if u.ln() < 0.5 * x * x + d * (1.0 - v_cubed + v_cubed.ln()) {
            return d * v_cubed;
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_path_starts_and_ends_at_targets() {
        let mut sim = HumanInputSimulator::new(InputSimConfig::default());
        let path = sim.generate_mouse_path(100.0, 200.0, 500.0, 600.0);
        assert!(!path.is_empty(), "path must have at least start point");
        assert!((path[0].x - 100.0).abs() < 0.01, "start x should match");
        assert!((path[0].y - 200.0).abs() < 0.01, "start y should match");
        let last = path.last().unwrap();
        assert!((last.x - 500.0).abs() < 0.01, "end x should match");
        assert!((last.y - 600.0).abs() < 0.01, "end y should match");
        assert!(last.t_ms > 0.0, "end time should be positive");
    }

    #[test]
    fn mouse_path_points_increase_monotonically_in_time() {
        let mut sim = HumanInputSimulator::new(InputSimConfig::default());
        let path = sim.generate_mouse_path(0.0, 0.0, 800.0, 600.0);
        for i in 1..path.len() {
            assert!(
                path[i].t_ms >= path[i - 1].t_ms,
                "time must be monotonic: idx {i}"
            );
        }
    }

    #[test]
    fn subpixel_movement_returns_single_point() {
        let mut sim = HumanInputSimulator::new(InputSimConfig::default());
        let path = sim.generate_mouse_path(100.0, 100.0, 100.3, 100.2);
        assert_eq!(path.len(), 1);
    }

    #[test]
    fn typing_sequence_length_matches_text_length_plus_one() {
        let mut sim = HumanInputSimulator::new(InputSimConfig::default());
        let delays = sim.generate_typing_sequence("hello world");
        assert_eq!(delays.len(), 12); // "hello world" has 11 chars + 1 initial
    }

    #[test]
    fn typing_delays_are_within_reasonable_bounds() {
        let mut sim = HumanInputSimulator::new(InputSimConfig::default());
        let long_text = "The quick brown fox jumps over the lazy dog. ".repeat(5);
        let delays = sim.generate_typing_delays(&long_text);
        for (i, d) in delays.iter().enumerate() {
            assert!(
                *d >= 0.0 && *d <= 600.0,
                "delay[{i}] = {d} is out of bounds"
            );
        }
    }

    #[test]
    fn empty_text_yields_single_initial_delay() {
        let mut sim = HumanInputSimulator::new(InputSimConfig::default());
        let delays = sim.generate_typing_sequence("");
        assert_eq!(delays.len(), 1);
    }

    #[test]
    fn click_jitter_stays_near_centre() {
        let mut sim = HumanInputSimulator::new(InputSimConfig::default());
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let samples = 1000;
        for _ in 0..samples {
            let (jx, jy) = sim.jitter_click_point(500.0, 300.0);
            sum_x += jx;
            sum_y += jy;
        }
        let mean_x = sum_x / samples as f64;
        let mean_y = sum_y / samples as f64;
        // Mean should be close to centre (within ~0.3 px).
        assert!(
            (mean_x - 500.0).abs() < 0.5,
            "mean x {mean_x} should be near 500"
        );
        assert!(
            (mean_y - 300.0).abs() < 0.5,
            "mean y {mean_y} should be near 300"
        );
    }

    #[test]
    fn click_offset_in_element_clamps_to_bounds() {
        let mut sim = HumanInputSimulator::new(InputSimConfig {
            click_jitter_px: 50.0,
            ..InputSimConfig::default()
        });
        let (x, y) = sim.click_offset_in_element(100.0, 100.0, 40.0, 30.0);
        assert!(
            (80.0..=120.0).contains(&x),
            "x must be within element width: {x}"
        );
        assert!(
            (85.0..=115.0).contains(&y),
            "y must be within element height: {y}"
        );
    }

    #[test]
    fn mouse_path_number_of_points_in_config_range() {
        let config = InputSimConfig {
            min_path_points: 10,
            max_path_points: 15,
            ..InputSimConfig::default()
        };
        let mut sim = HumanInputSimulator::new(config);
        let path = sim.generate_mouse_path(10.0, 10.0, 500.0, 500.0);
        // n_segments in [10,15] yields n_segments+1 points = [11, 16]
        assert!(
            path.len() >= 11 && path.len() <= 16,
            "path length {} should be in [11, 16]",
            path.len()
        );
    }

    #[test]
    fn gaussian_sample_produces_reasonable_distribution() {
        let mut rng = fastrand::Rng::with_seed(42);
        let mut sum = 0.0;
        let n = 10_000;
        for _ in 0..n {
            sum += gaussian_sample(&mut rng);
        }
        let mean = sum / n as f64;
        assert!(
            mean.abs() < 0.1,
            "gaussian sample mean should be near 0, got {mean}"
        );
    }

    #[test]
    fn gamma_sample_produces_reasonable_distribution() {
        let mut rng = fastrand::Rng::with_seed(42);
        let mut sum = 0.0;
        let n = 10_000;
        for _ in 0..n {
            sum += gamma_sample(&mut rng, 2.0);
        }
        let mean = sum / n as f64;
        assert!(
            (mean - 2.0).abs() < 0.2,
            "gamma(k=2) mean should be near 2.0, got {mean}"
        );
    }

    #[test]
    fn bigram_acceleration_never_increases_delays() {
        let mut sim = HumanInputSimulator::new(InputSimConfig::default());
        let text = "the quick brown fox";
        let mut delays = sim.generate_typing_sequence(text);
        let original = delays.clone();
        sim.apply_bigram_acceleration(text, &mut delays);
        for (i, d) in delays.iter().enumerate() {
            assert!(
                *d <= original[i] * 1.01,
                "bigram acceleration should not increase delay[{i}]: {d} > {}",
                original[i]
            );
        }
    }

    #[test]
    fn deterministic_seed_yields_reproducible_path() {
        let mut sim1 = HumanInputSimulator::with_seed(InputSimConfig::default(), 12345);
        let mut sim2 = HumanInputSimulator::with_seed(InputSimConfig::default(), 12345);
        let path1 = sim1.generate_mouse_path(0.0, 0.0, 100.0, 100.0);
        let path2 = sim2.generate_mouse_path(0.0, 0.0, 100.0, 100.0);
        assert_eq!(path1.len(), path2.len());
        for (a, b) in path1.iter().zip(path2.iter()) {
            assert!((a.x - b.x).abs() < 0.001);
            assert!((a.y - b.y).abs() < 0.001);
        }
    }
}
