use std::collections::HashSet;
use std::time::Instant;

use crate::config::{DetectorConfig, StrikeLevel};
use crate::url_matcher::{extract_slug, find_matching_rule};

/// Current snapshot status of the doomscroll detector.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectorStatus {
    pub current_strike: StrikeLevel,
    pub swipe_count: u32,
    pub dwell_secs: f32,
    pub matched_rule: Option<String>,
    pub strike_changed: bool,
    pub cooled_down: bool,
}

/// Dual-trigger doomscroll state machine tracking unique URL slugs
/// and continuous active dwell time.
pub struct DoomscrollDetector {
    config: DetectorConfig,
    visited_slugs: HashSet<String>,
    swipe_count: u32,
    dwell_started: Option<Instant>,
    active_dwell_secs: f32,
    last_doomscroll_time: Option<Instant>,
    current_strike: StrikeLevel,
}

impl DoomscrollDetector {
    pub fn new(config: DetectorConfig) -> Self {
        Self {
            config,
            visited_slugs: HashSet::new(),
            swipe_count: 0,
            dwell_started: None,
            active_dwell_secs: 0.0,
            last_doomscroll_time: None,
            current_strike: StrikeLevel::None,
        }
    }

    /// Feeds the current foreground browser URL into the detector.
    /// Returns the updated detector status and whether strike escalation occurred.
    pub fn update(&mut self, current_url: Option<&str>, now: Instant) -> DetectorStatus {
        let mut cooled_down = false;

        // Check if URL matches any doomscroll rule
        let matched = current_url.and_then(|url| {
            find_matching_rule(url, &self.config.target_rules).map(|r| (url, r.to_string()))
        });

        if let Some((url, rule)) = matched {
            self.last_doomscroll_time = Some(now);

            // Trigger A: Unique Slug Swipe Counter
            let slug = extract_slug(url, &rule);
            if self.visited_slugs.insert(slug) {
                self.swipe_count += 1;
            }

            // Trigger B: Continuous Dwell Timer
            match self.dwell_started {
                Some(start) => {
                    let elapsed = now.saturating_duration_since(start).as_secs_f32();
                    self.active_dwell_secs = elapsed;
                }
                None => {
                    self.dwell_started = Some(now);
                    self.active_dwell_secs = 0.0;
                }
            }

            // Evaluate escalation based on whichever threshold is reached first
            let prev_strike = self.current_strike;
            let evaluated_strike = self.evaluate_thresholds();
            if evaluated_strike > self.current_strike {
                self.current_strike = evaluated_strike;
            }

            DetectorStatus {
                current_strike: self.current_strike,
                swipe_count: self.swipe_count,
                dwell_secs: self.active_dwell_secs,
                matched_rule: Some(rule),
                strike_changed: self.current_strike != prev_strike,
                cooled_down: false,
            }
        } else {
            // Not currently doomscrolling: pause the continuous dwell timer
            self.dwell_started = None;

            // Check if cooldown threshold has expired
            if let Some(last_time) = self.last_doomscroll_time {
                let inactive_secs = now.saturating_duration_since(last_time).as_secs_f32();
                if inactive_secs >= self.config.cooldown_secs {
                    self.reset();
                    cooled_down = true;
                }
            }

            DetectorStatus {
                current_strike: self.current_strike,
                swipe_count: self.swipe_count,
                dwell_secs: self.active_dwell_secs,
                matched_rule: None,
                strike_changed: false,
                cooled_down,
            }
        }
    }

    /// Evaluates strike level based on configured swipe count or dwell time limits.
    fn evaluate_thresholds(&self) -> StrikeLevel {
        if self.swipe_count >= self.config.strike_3_swipes
            || self.active_dwell_secs >= self.config.strike_3_secs
        {
            StrikeLevel::Strike3
        } else if self.swipe_count >= self.config.strike_2_swipes
            || self.active_dwell_secs >= self.config.strike_2_secs
        {
            StrikeLevel::Strike2
        } else if self.swipe_count >= self.config.strike_1_swipes
            || self.active_dwell_secs >= self.config.strike_1_secs
        {
            StrikeLevel::Strike1
        } else {
            StrikeLevel::None
        }
    }

    /// Resets all counters and strike levels back to baseline.
    pub fn reset(&mut self) {
        self.visited_slugs.clear();
        self.swipe_count = 0;
        self.dwell_started = None;
        self.active_dwell_secs = 0.0;
        self.last_doomscroll_time = None;
        self.current_strike = StrikeLevel::None;
    }

    pub fn current_strike(&self) -> StrikeLevel {
        self.current_strike
    }

    pub fn swipe_count(&self) -> u32 {
        self.swipe_count
    }

    pub fn active_dwell_secs(&self) -> f32 {
        self.active_dwell_secs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_swipe_trigger_escalation() {
        let config = DetectorConfig {
            target_rules: vec!["youtube.com/shorts/".to_string()],
            strike_1_swipes: 2,
            strike_1_secs: 100.0,
            strike_2_swipes: 4,
            strike_2_secs: 200.0,
            strike_3_swipes: 5,
            strike_3_secs: 300.0,
            cooldown_secs: 60.0,
            allow_close_pinned: false,
        };

        let mut detector = DoomscrollDetector::new(config);
        let start = Instant::now();

        // 1st swipe
        let status = detector.update(Some("youtube.com/shorts/video1"), start);
        assert_eq!(status.current_strike, StrikeLevel::None);
        assert_eq!(status.swipe_count, 1);

        // 2nd swipe -> Strike 1
        let status = detector.update(
            Some("youtube.com/shorts/video2"),
            start + Duration::from_millis(500),
        );
        assert_eq!(status.current_strike, StrikeLevel::Strike1);
        assert_eq!(status.swipe_count, 2);
        assert!(status.strike_changed);

        // Same slug should not increment swipe count
        let status = detector.update(
            Some("youtube.com/shorts/video2"),
            start + Duration::from_millis(1000),
        );
        assert_eq!(status.swipe_count, 2);
    }

    #[test]
    fn test_dwell_trigger_escalation() {
        let config = DetectorConfig {
            target_rules: vec!["instagram.com/reels".to_string()],
            strike_1_swipes: 10,
            strike_1_secs: 10.0,
            strike_2_swipes: 20,
            strike_2_secs: 20.0,
            strike_3_swipes: 30,
            strike_3_secs: 30.0,
            cooldown_secs: 60.0,
            allow_close_pinned: false,
        };

        let mut detector = DoomscrollDetector::new(config);
        let t0 = Instant::now();

        detector.update(Some("instagram.com/reels/reel1"), t0);

        // 12 seconds dwell on same reel -> Strike 1
        let t1 = t0 + Duration::from_secs(12);
        let status = detector.update(Some("instagram.com/reels/reel1"), t1);
        assert_eq!(status.current_strike, StrikeLevel::Strike1);
        assert!(status.dwell_secs >= 12.0);

        // 32 seconds dwell -> Strike 3
        let t2 = t0 + Duration::from_secs(32);
        let status = detector.update(Some("instagram.com/reels/reel1"), t2);
        assert_eq!(status.current_strike, StrikeLevel::Strike3);
    }

    #[test]
    fn test_cooldown_reset() {
        let config = DetectorConfig {
            target_rules: vec!["youtube.com/shorts/".to_string()],
            strike_1_swipes: 1,
            strike_1_secs: 5.0,
            cooldown_secs: 10.0,
            ..Default::default()
        };

        let mut detector = DoomscrollDetector::new(config);
        let t0 = Instant::now();

        detector.update(Some("youtube.com/shorts/short1"), t0);
        assert_eq!(detector.current_strike(), StrikeLevel::Strike1);

        // User navigates away to safe page
        detector.update(Some("github.com"), t0 + Duration::from_secs(2));
        assert_eq!(detector.current_strike(), StrikeLevel::Strike1);

        // After cooldown expires
        let status = detector.update(Some("github.com"), t0 + Duration::from_secs(12));
        assert!(status.cooled_down);
        assert_eq!(detector.current_strike(), StrikeLevel::None);
        assert_eq!(detector.swipe_count(), 0);
    }
}
