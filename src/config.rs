use serde::{Deserialize, Serialize};

/// Action to execute when Strike 3 is triggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionMode {
    /// Actively close the target doomscroll tab via UI Automation.
    Swat,
    /// Play full alert and swat animation without closing the tab.
    NudgeOnly,
}

impl Default for ActionMode {
    fn default() -> Self {
        Self::Swat
    }
}

/// The three-strike escalation level of doomscrolling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum StrikeLevel {
    None = 0,
    Strike1 = 1,
    Strike2 = 2,
    Strike3 = 3,
}

impl StrikeLevel {
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "Clear",
            Self::Strike1 => "Strike 1 (Peeking)",
            Self::Strike2 => "Strike 2 (Patrolling)",
            Self::Strike3 => "Strike 3 (Swat Triggered)",
        }
    }
}

/// Thresholds and target URL patterns for doomscroll detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectorConfig {
    /// Target URL substrings or prefixes to match (e.g., "youtube.com/shorts/", "instagram.com/reels").
    pub target_rules: Vec<String>,
    /// Unique URL slug count needed to trigger Strike 1.
    pub strike_1_swipes: u32,
    /// Continuous dwell time (seconds) needed to trigger Strike 1.
    pub strike_1_secs: f32,
    /// Unique URL slug count needed to trigger Strike 2.
    pub strike_2_swipes: u32,
    /// Continuous dwell time (seconds) needed to trigger Strike 2.
    pub strike_2_secs: f32,
    /// Unique URL slug count needed to trigger Strike 3.
    pub strike_3_swipes: u32,
    /// Continuous dwell time (seconds) needed to trigger Strike 3.
    pub strike_3_secs: f32,
    /// Inactivity grace period in seconds before resetting counters.
    pub cooldown_secs: f32,
    /// Whether to allow closing pinned browser tabs.
    pub allow_close_pinned: bool,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        Self {
            target_rules: vec![
                "youtube.com/shorts/".to_string(),
                "instagram.com/reels".to_string(),
            ],
            strike_1_swipes: 2,
            strike_1_secs: 20.0,
            strike_2_swipes: 4,
            strike_2_secs: 45.0,
            strike_3_swipes: 5,
            strike_3_secs: 75.0,
            cooldown_secs: 120.0,
            allow_close_pinned: false,
        }
    }
}

/// Root application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub detector: DetectorConfig,
    pub action_mode: ActionMode,
    pub sound_enabled: bool,
    pub start_with_windows: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            detector: DetectorConfig::default(),
            action_mode: ActionMode::default(),
            sound_enabled: true,
            start_with_windows: false,
        }
    }
}
