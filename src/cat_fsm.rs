use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetSystemMetrics, GetWindowRect, IsIconic, IsWindow, SM_CYSCREEN,
};

use crate::animator::{AnimationClip, Animator};
use crate::browser::RelativeRect;
use crate::config::StrikeLevel;
use crate::sprite::FacingDirection;
use crate::watcher::BrowserTelemetry;

/// Paw tip offset in unscaled 32x32 sprite coordinates (facing right).
pub const PAW_OFFSET_X: f32 = 26.0;
pub const PAW_OFFSET_Y: f32 = 22.0;
pub const SPRITE_CELL_SIZE: f32 = 32.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatState {
    /// Hidden from desktop, 0.0% idle CPU.
    Hidden,
    /// Strike 1: Perched near tab bar edge, flicking tail (Idle).
    Peeking,
    /// Strike 2: Trotting back and forth along tab strip ledge (Walk).
    Patrolling,
    /// Strike 3: Alerted with pixel '!' (Alert 400ms).
    Alert,
    /// Trotting smoothly to align paw over the active tab's X button.
    WalkingToTarget,
    /// Swatting sequence: WindUp -> Anticipate -> Impact -> FollowThrough.
    SwatSequence,
    /// Smug paw-lick then trotting off-screen.
    SmugExit,
    /// User escaped doomscrolling tab: '?' overhead, trotting off-screen.
    ConfusedAbort,
    /// Browser minimized/closed while visible: falling with gravity.
    Falling,
}

/// Outcome of an FSM update tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsmAction {
    /// Continue rendering normal frame.
    None,
    /// Swat impact frame reached! Execute tab close and optional sound.
    ExecuteSwat,
    /// Cat has completed its exit and should hide (returning to 0.0% CPU).
    HideOverlay,
}

/// The high-level state machine coordinating live window tracking,
/// strike escalation, sub-pixel steering, and animation sequences.
pub struct CatFsm {
    pub state: CatState,
    pub animator: Animator,
    pub scale: f32,

    // Window tracking state
    pub target_hwnd: HWND,
    pub tab_strip_relative: Option<RelativeRect>,
    pub close_button_relative: Option<RelativeRect>,
    pub is_maximized: bool,
    pub is_fullscreen: bool,

    // Movement & patrol bounds
    patrol_left: f32,
    patrol_right: f32,
    patrol_direction: FacingDirection,
    patrol_timer: f32,

    // Swatting phase tracking
    swat_fired: bool,
    exit_target_x: f32,

    // Falling physics
    pub vel_y: f32,
    gravity: f32,

    // Last known browser screen bounds
    last_window_rect: RECT,
}

impl CatFsm {
    pub fn new(scale: u32) -> Self {
        let scale_f = scale.max(1) as f32;
        Self {
            state: CatState::Hidden,
            animator: Animator::new(scale_f),
            scale: scale_f,
            target_hwnd: HWND::default(),
            tab_strip_relative: None,
            close_button_relative: None,
            is_maximized: false,
            is_fullscreen: false,
            patrol_left: 0.0,
            patrol_right: 0.0,
            patrol_direction: FacingDirection::Right,
            patrol_timer: 0.0,
            swat_fired: false,
            exit_target_x: 0.0,
            vel_y: 0.0,
            gravity: 1200.0 * scale_f,
            last_window_rect: RECT::default(),
        }
    }

    /// Receives fresh 400ms telemetry from the UIA watcher.
    pub fn apply_telemetry(&mut self, telemetry: &BrowserTelemetry) {
        self.target_hwnd = HWND(telemetry.hwnd as *mut _);
        self.tab_strip_relative = telemetry.tab_strip_relative;
        self.close_button_relative = telemetry.close_button_relative;
        self.is_maximized = telemetry.is_maximized;
        self.is_fullscreen = telemetry.is_fullscreen;

        let strike = telemetry.detector.current_strike;

        // If fullscreen (e.g. F11 or full-screen video), suppress overlay
        if self.is_fullscreen {
            if self.state != CatState::Hidden {
                self.transition_to(CatState::Hidden);
            }
            return;
        }

        // State escalation handling based on strike level
        match self.state {
            CatState::Hidden => match strike {
                StrikeLevel::Strike1 => self.start_peeking(),
                StrikeLevel::Strike2 => self.start_patrolling(),
                StrikeLevel::Strike3 => self.start_swat_escalation(),
                StrikeLevel::None => {}
            },
            CatState::Peeking => match strike {
                StrikeLevel::Strike2 => self.start_patrolling(),
                StrikeLevel::Strike3 => self.start_swat_escalation(),
                StrikeLevel::None => self.start_confused_abort(),
                _ => {}
            },
            CatState::Patrolling => match strike {
                StrikeLevel::Strike3 => self.start_swat_escalation(),
                StrikeLevel::None => self.start_confused_abort(),
                _ => {}
            },
            CatState::Alert | CatState::WalkingToTarget => {
                // If the user navigates away or closes the doomscroll tab before impact:
                if strike == StrikeLevel::None || telemetry.detector.matched_rule.is_none() {
                    self.start_confused_abort();
                }
            }
            CatState::SwatSequence => {
                // In swat sequence, if user escaped during Anticipate (before impact):
                if !self.swat_fired
                    && (strike == StrikeLevel::None || telemetry.detector.matched_rule.is_none())
                {
                    self.start_confused_abort();
                }
            }
            CatState::SmugExit | CatState::ConfusedAbort | CatState::Falling => {
                // Already completing exit animations
            }
        }
    }

    /// 60 FPS sub-pixel tick: queries live window position, steps physics & animation.
    pub fn update(&mut self, dt: f32) -> FsmAction {
        if self.state == CatState::Hidden {
            return FsmAction::None;
        }

        // 1. Live window & focus verification
        if self.target_hwnd.0.is_null() || unsafe { !IsWindow(Some(self.target_hwnd)).as_bool() } {
            // Browser window closed
            if self.state != CatState::Falling {
                self.start_falling();
            }
        } else if unsafe { IsIconic(self.target_hwnd).as_bool() } {
            // Browser minimized
            if self.state != CatState::Falling {
                self.start_falling();
            }
        } else {
            // Live window bounds tracking at 60 FPS
            let mut rect = RECT::default();
            let ok = unsafe { GetWindowRect(self.target_hwnd, &mut rect) };
            if ok.is_ok() {
                self.last_window_rect = rect;
            }

            // Alt+Tab guard: if user switched to another application
            let fg = unsafe { GetForegroundWindow() };
            if fg != self.target_hwnd && self.state != CatState::Falling && self.state != CatState::ConfusedAbort {
                self.transition_to(CatState::Hidden);
                return FsmAction::HideOverlay;
            }
        }

        // 2. State-specific motion and animation update
        let mut action = FsmAction::None;

        match self.state {
            CatState::Hidden => FsmAction::None,

            CatState::Peeking => {
                self.align_to_tab_bar_ledge();
                self.animator.update(dt);
                FsmAction::None
            }

            CatState::Patrolling => {
                self.align_to_tab_bar_ledge();
                self.update_patrol(dt);
                self.animator.update(dt);
                FsmAction::None
            }

            CatState::Alert => {
                self.align_to_tab_bar_ledge();
                self.animator.update(dt);
                if self.animator.is_finished {
                    self.transition_to(CatState::WalkingToTarget);
                }
                FsmAction::None
            }

            CatState::WalkingToTarget => {
                self.align_to_tab_bar_ledge();
                let target_x = self.calculate_swat_window_x();
                let arrived = self.animator.steer_horizontal(target_x, dt);
                self.animator.update(dt);

                if arrived {
                    self.transition_to(CatState::SwatSequence);
                }
                FsmAction::None
            }

            CatState::SwatSequence => {
                self.align_to_tab_bar_ledge();
                self.animator.update(dt);

                // Step through swat clip phases: WindUp -> Anticipate -> Impact -> FollowThrough
                match self.animator.current_clip {
                    AnimationClip::WindUp => {
                        if self.animator.is_finished {
                            self.animator.play(AnimationClip::Anticipate);
                        }
                    }
                    AnimationClip::Anticipate => {
                        if self.animator.is_finished {
                            self.animator.play(AnimationClip::Impact);
                            if !self.swat_fired {
                                self.swat_fired = true;
                                action = FsmAction::ExecuteSwat;
                            }
                        }
                    }
                    AnimationClip::Impact => {
                        if self.animator.is_finished {
                            self.animator.play(AnimationClip::FollowThrough);
                        }
                    }
                    AnimationClip::FollowThrough => {
                        if self.animator.is_finished {
                            self.transition_to(CatState::SmugExit);
                        }
                    }
                    _ => {}
                }
                action
            }

            CatState::SmugExit => {
                self.align_to_tab_bar_ledge();
                self.animator.update(dt);

                if self.animator.current_clip == AnimationClip::SmugLick {
                    if self.animator.is_finished {
                        self.animator.play(AnimationClip::Walk);
                        // Steer toward nearest window edge to exit
                        let left_dist = (self.animator.pos_x - self.last_window_rect.left as f32).abs();
                        let right_dist = (self.last_window_rect.right as f32 - self.animator.pos_x).abs();
                        if left_dist < right_dist {
                            self.exit_target_x = (self.last_window_rect.left - (SPRITE_CELL_SIZE * self.scale) as i32) as f32;
                        } else {
                            self.exit_target_x = (self.last_window_rect.right + 20) as f32;
                        }
                    }
                } else if self.animator.current_clip == AnimationClip::Walk {
                    let arrived = self.animator.steer_horizontal(self.exit_target_x, dt);
                    if arrived {
                        self.transition_to(CatState::Hidden);
                        action = FsmAction::HideOverlay;
                    }
                }
                action
            }

            CatState::ConfusedAbort => {
                self.align_to_tab_bar_ledge();
                self.animator.update(dt);

                if self.animator.current_clip == AnimationClip::Confused {
                    if self.animator.is_finished {
                        self.animator.play(AnimationClip::Walk);
                        // Walk toward right edge of browser
                        self.exit_target_x = (self.last_window_rect.right + 20) as f32;
                    }
                } else if self.animator.current_clip == AnimationClip::Walk {
                    let arrived = self.animator.steer_horizontal(self.exit_target_x, dt);
                    if arrived {
                        self.transition_to(CatState::Hidden);
                        action = FsmAction::HideOverlay;
                    }
                }
                action
            }

            CatState::Falling => {
                self.vel_y += self.gravity * dt;
                self.animator.pos_y += self.vel_y * dt;
                self.animator.update(dt);

                // Stop at monitor work area / taskbar
                let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
                let floor_y = (screen_h - (SPRITE_CELL_SIZE * self.scale) as i32 - 40) as f32;

                if self.animator.pos_y >= floor_y {
                    self.animator.pos_y = floor_y;
                    self.transition_to(CatState::Hidden);
                    action = FsmAction::HideOverlay;
                }
                action
            }
        }
    }

    /// Computes the exact screen X coordinate where the cat window must be placed
    /// so its paw tip strikes precisely at the active tab's X button center.
    pub fn calculate_swat_window_x(&self) -> f32 {
        if let Some(btn) = self.close_button_relative {
            let target_center_x = (self.last_window_rect.left + btn.rel_x + (btn.width / 2)) as f32;
            match self.animator.facing {
                FacingDirection::Right => target_center_x - (PAW_OFFSET_X * self.scale),
                FacingDirection::Left => {
                    target_center_x - ((SPRITE_CELL_SIZE - PAW_OFFSET_X) * self.scale)
                }
            }
        } else {
            // Default to right edge of tab strip if close button rect is not available
            (self.last_window_rect.right - 100) as f32
        }
    }

    /// Aligns Y coordinate and perching ledge with the browser tab bar.
    pub fn align_to_tab_bar_ledge(&mut self) {
        let scaled_h = SPRITE_CELL_SIZE * self.scale;

        if let Some(strip) = self.tab_strip_relative {
            let screen_strip_top = (self.last_window_rect.top + strip.rel_y) as f32;
            let screen_strip_bottom = screen_strip_top + strip.height as f32;

            if self.is_maximized || screen_strip_top < scaled_h {
                // Maximized window: perches along the tab-bar/address-bar divider
                // Clamped to >= 0 so cat is 100% visible on top of screen
                let base_y = screen_strip_bottom - (28.0 * self.scale);
                self.animator.pos_y = base_y.max(self.last_window_rect.top.max(0) as f32);
            } else {
                // Windowed mode: perches comfortably on the tab bar top
                self.animator.pos_y = screen_strip_top - scaled_h + (6.0 * self.scale);
            }
        } else {
            // Fallback: top of browser window
            self.animator.pos_y = self.last_window_rect.top.max(0) as f32;
        }
    }

    fn update_patrol(&mut self, dt: f32) {
        if let Some(strip) = self.tab_strip_relative {
            self.patrol_left = (self.last_window_rect.left + strip.rel_x + 20) as f32;
            self.patrol_right =
                ((self.last_window_rect.left + strip.rel_x + strip.width - 60) as f32).max(self.patrol_left + 100.0);
        } else {
            self.patrol_left = (self.last_window_rect.left + 50) as f32;
            self.patrol_right = (self.last_window_rect.right - 100) as f32;
        }

        self.patrol_timer += dt;

        // Alternate walking and brief pauses every 2.5 seconds
        if (self.patrol_timer as u32) % 4 == 0 {
            self.animator.play(AnimationClip::Idle);
        } else {
            self.animator.play(AnimationClip::Walk);
            let target = match self.patrol_direction {
                FacingDirection::Right => self.patrol_right,
                FacingDirection::Left => self.patrol_left,
            };

            let arrived = self.animator.steer_horizontal(target, dt);
            if arrived {
                self.patrol_direction = match self.patrol_direction {
                    FacingDirection::Right => FacingDirection::Left,
                    FacingDirection::Left => FacingDirection::Right,
                };
            }
        }
    }

    fn start_peeking(&mut self) {
        self.align_to_tab_bar_ledge();
        // Spawn near the left side of the tab strip
        if let Some(strip) = self.tab_strip_relative {
            self.animator.pos_x = (self.last_window_rect.left + strip.rel_x + 40) as f32;
        } else {
            self.animator.pos_x = (self.last_window_rect.left + 100) as f32;
        }
        self.animator.facing = FacingDirection::Right;
        self.transition_to(CatState::Peeking);
        self.animator.play(AnimationClip::Idle);
    }

    fn start_patrolling(&mut self) {
        self.align_to_tab_bar_ledge();
        self.transition_to(CatState::Patrolling);
        self.animator.play(AnimationClip::Walk);
    }

    fn start_swat_escalation(&mut self) {
        self.align_to_tab_bar_ledge();
        self.transition_to(CatState::Alert);
        self.animator.play(AnimationClip::Alert);
        self.swat_fired = false;
    }

    fn start_confused_abort(&mut self) {
        self.transition_to(CatState::ConfusedAbort);
        self.animator.play(AnimationClip::Confused);
    }

    fn start_falling(&mut self) {
        self.transition_to(CatState::Falling);
        self.animator.play(AnimationClip::Falling);
        self.vel_y = 50.0;
    }

    pub fn transition_to(&mut self, new_state: CatState) {
        self.state = new_state;
        if new_state == CatState::Hidden {
            self.swat_fired = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swat_paw_offset_math() {
        let mut fsm = CatFsm::new(2);
        fsm.last_window_rect = RECT {
            left: 100,
            top: 50,
            right: 1000,
            bottom: 800,
        };
        fsm.close_button_relative = Some(RelativeRect {
            rel_x: 500,
            rel_y: 20,
            width: 16,
            height: 16,
        });

        // Facing right: target_x should be center_x (100 + 500 + 8 = 608) - (26 * 2 = 52) = 556
        fsm.animator.facing = FacingDirection::Right;
        let swat_x = fsm.calculate_swat_window_x();
        assert_eq!(swat_x, 556.0);

        // Facing left: target_x should be 608 - ((32 - 26) * 2 = 12) = 596
        fsm.animator.facing = FacingDirection::Left;
        let swat_x_left = fsm.calculate_swat_window_x();
        assert_eq!(swat_x_left, 596.0);
    }

    #[test]
    fn test_strike_escalation_transitions() {
        let mut fsm = CatFsm::new(2);
        assert_eq!(fsm.state, CatState::Hidden);

        let mut telemetry = BrowserTelemetry {
            hwnd: 0x1234,
            process_name: "chrome.exe".to_string(),
            current_url: None,
            normalized_url: Some("youtube.com/shorts/test".to_string()),
            matched_rule: Some("youtube.com/shorts/".to_string()),
            is_maximized: true,
            is_fullscreen: false,
            tab_strip_relative: Some(RelativeRect {
                rel_x: 30,
                rel_y: 7,
                width: 800,
                height: 40,
            }),
            tab_strip_orientation: None,
            close_button_relative: Some(RelativeRect {
                rel_x: 400,
                rel_y: 18,
                width: 16,
                height: 16,
            }),
            is_pinned: false,
            detector: crate::detector::DetectorStatus {
                current_strike: StrikeLevel::Strike1,
                swipe_count: 2,
                dwell_secs: 15.0,
                matched_rule: Some("youtube.com/shorts/".to_string()),
                strike_changed: true,
                cooled_down: false,
            },
        };

        // Strike 1 -> Peeking
        fsm.apply_telemetry(&telemetry);
        assert_eq!(fsm.state, CatState::Peeking);

        // Strike 2 -> Patrolling
        telemetry.detector.current_strike = StrikeLevel::Strike2;
        fsm.apply_telemetry(&telemetry);
        assert_eq!(fsm.state, CatState::Patrolling);

        // Strike 3 -> Alert
        telemetry.detector.current_strike = StrikeLevel::Strike3;
        fsm.apply_telemetry(&telemetry);
        assert_eq!(fsm.state, CatState::Alert);
    }
}
