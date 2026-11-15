//! Turns raw pointer input into sound events. Pure logic, no OS calls.
use std::time::{Duration, Instant};

/// One input event from any platform backend.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    Key { code: &'static str, down: bool },
    /// which: "left" | "right" | "middle"; tap = trackpad tap-to-click (no physical press)
    Button { which: &'static str, down: bool, tap: bool },
    Scroll { dx: f64, dy: f64 },
    Move { dx: f64, dy: f64 },
    /// "swipe" | "pinch"
    Gesture(&'static str),
}

const MIN_TICK_GAP: Duration = Duration::from_millis(25); // 40 ticks/s cap

/// Scroll distance -> wheel-notch ticks.
pub struct ScrollTicks {
    acc: f64,
    last_tick: Option<Instant>,
}

impl ScrollTicks {
    pub fn new() -> Self {
        Self { acc: 0.0, last_tick: None }
    }

    /// sensitivity 0..1: 0 = one tick per 120 points, 1 = one per 15.
    pub fn feed(&mut self, dx: f64, dy: f64, sensitivity: f32, now: Instant) -> bool {
        let threshold = 120.0 - 105.0 * sensitivity.clamp(0.0, 1.0) as f64;
        self.acc += dx.abs() + dy.abs();
        if self.acc < threshold {
            return false;
        }
        if self.last_tick.is_some_and(|t| now.duration_since(t) < MIN_TICK_GAP) {
            self.acc = threshold; // over the cap: drop the excess instead of building a backlog
            return false;
        }
        self.acc -= threshold;
        self.acc = self.acc.min(threshold);
        self.last_tick = Some(now);
        true
    }
}

/// Pointer deltas -> speed samples at most every 16 ms.
pub struct MoveSpeed {
    dist: f64,
    since: Option<Instant>,
}

impl MoveSpeed {
    pub fn new() -> Self {
        Self { dist: 0.0, since: None }
    }

    /// Returns points/second when a 16 ms window closes.
    pub fn feed(&mut self, dx: f64, dy: f64, now: Instant) -> Option<f32> {
        let start = *self.since.get_or_insert(now);
        self.dist += (dx * dx + dy * dy).sqrt();
        let dt = now.duration_since(start);
        if dt < Duration::from_millis(16) {
            return None;
        }
        let speed = (self.dist / dt.as_secs_f64().min(0.25)) as f32;
        self.dist = 0.0;
        self.since = Some(now);
        Some(speed)
    }
}

/// Collapses a burst of gesture events into one sound per gesture.
pub struct GestureDebounce {
    last: Option<(&'static str, Instant)>,
}

impl GestureDebounce {
    pub fn new() -> Self {
        Self { last: None }
    }

    pub fn feed(&mut self, kind: &'static str, now: Instant) -> bool {
        let fire = !matches!(self.last, Some((k, t)) if k == kind && now.duration_since(t) < Duration::from_millis(250));
        self.last = Some((kind, now));
        fire
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_ticks_by_distance_and_cap() {
        let t0 = Instant::now();
        let mut s = ScrollTicks::new();
        // sensitivity 0 -> 120 points per tick
        assert!(!s.feed(0.0, 100.0, 0.0, t0));
        assert!(s.feed(0.0, 30.0, 0.0, t0));
        // a flood of momentum scrolling within 25 ms gives no extra ticks
        let mut n = 0;
        for i in 0..20 {
            if s.feed(0.0, 500.0, 0.0, t0 + Duration::from_millis(i)) { n += 1; }
        }
        assert_eq!(n, 0);
        // over one second of heavy scrolling, at most 40 ticks
        let mut n = 0;
        for i in 0..1000 {
            if s.feed(0.0, 500.0, 1.0, t0 + Duration::from_millis(30 + i)) { n += 1; }
        }
        assert!((38..=40).contains(&n), "{n}");
    }

    #[test]
    fn move_speed_windows() {
        let t0 = Instant::now();
        let mut m = MoveSpeed::new();
        assert_eq!(m.feed(3.0, 4.0, t0), None);
        let v = m.feed(3.0, 4.0, t0 + Duration::from_millis(20)).unwrap();
        assert!((v - 500.0).abs() < 1.0, "{v}"); // 10 points in 20 ms
    }

    #[test]
    fn gesture_once_per_burst() {
        let t0 = Instant::now();
        let mut g = GestureDebounce::new();
        assert!(g.feed("pinch", t0));
        assert!(!g.feed("pinch", t0 + Duration::from_millis(100)));
        assert!(!g.feed("pinch", t0 + Duration::from_millis(300))); // still the same burst
        assert!(g.feed("swipe", t0 + Duration::from_millis(310)));
        assert!(g.feed("swipe", t0 + Duration::from_millis(900)));
    }
}
