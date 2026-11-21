use crate::sprite::FacingDirection;

/// Animation clips corresponding to the 16-frame cat sprite sheet layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationClip {
    /// Frames 0..=1: Sitting on tab ledge, slow tail flick (450ms/frame, loop).
    Idle,
    /// Frames 2..=7: Smooth 6-frame trot cycle (100ms/frame, loop).
    Walk,
    /// Frame 8: Ears perked, pixel ! overhead (400ms).
    Alert,
    /// Frame 9: Halts and raises right paw / drawing scissors (120ms).
    WindUp,
    /// Frame 10: Paw poised menacingly over the X button (180ms).
    Anticipate,
    /// Frame 11: Downward bap/snip with 1-frame slash flash (80ms, impact hook).
    Impact,
    /// Frame 12: Paw rested down, satisfied squint (220ms).
    FollowThrough,
    /// Frame 13: Sits back and licks paw smugly (450ms).
    SmugLick,
    /// Frame 14: Head tilt with pixel ? overhead (450ms).
    Confused,
    /// Frame 15: Splayed paws during gravity fall (loop).
    Falling,
}

struct ClipSpec {
    start_frame: usize,
    frame_count: usize,
    frame_duration: f32,
    loops: bool,
}

impl AnimationClip {
    fn spec(self) -> ClipSpec {
        match self {
            Self::Idle => ClipSpec {
                start_frame: 0,
                frame_count: 2,
                frame_duration: 0.450,
                loops: true,
            },
            Self::Walk => ClipSpec {
                start_frame: 2,
                frame_count: 6,
                frame_duration: 0.100,
                loops: true,
            },
            Self::Alert => ClipSpec {
                start_frame: 8,
                frame_count: 1,
                frame_duration: 0.400,
                loops: false,
            },
            Self::WindUp => ClipSpec {
                start_frame: 9,
                frame_count: 1,
                frame_duration: 0.120,
                loops: false,
            },
            Self::Anticipate => ClipSpec {
                start_frame: 10,
                frame_count: 1,
                frame_duration: 0.180,
                loops: false,
            },
            Self::Impact => ClipSpec {
                start_frame: 11,
                frame_count: 1,
                frame_duration: 0.080,
                loops: false,
            },
            Self::FollowThrough => ClipSpec {
                start_frame: 12,
                frame_count: 1,
                frame_duration: 0.220,
                loops: false,
            },
            Self::SmugLick => ClipSpec {
                start_frame: 13,
                frame_count: 1,
                frame_duration: 0.450,
                loops: false,
            },
            Self::Confused => ClipSpec {
                start_frame: 14,
                frame_count: 1,
                frame_duration: 0.450,
                loops: false,
            },
            Self::Falling => ClipSpec {
                start_frame: 15,
                frame_count: 1,
                frame_duration: 0.100,
                loops: true,
            },
        }
    }
}

/// Decoupled animation and sub-pixel motion controller.
#[derive(Debug, Clone)]
pub struct Animator {
    pub current_clip: AnimationClip,
    pub frame_index: usize,
    frame_timer: f32,
    local_frame: usize,
    pub is_finished: bool,

    // Sub-pixel position and steering physics
    pub pos_x: f32,
    pub pos_y: f32,
    pub facing: FacingDirection,
    pub scale: f32,
    pub max_walk_speed: f32,
    pub arrival_radius: f32,
}

impl Animator {
    pub fn new(scale: f32) -> Self {
        let scale = scale.max(1.0);
        let spec = AnimationClip::Idle.spec();
        Self {
            current_clip: AnimationClip::Idle,
            frame_index: spec.start_frame,
            frame_timer: 0.0,
            local_frame: 0,
            is_finished: false,
            pos_x: 0.0,
            pos_y: 0.0,
            facing: FacingDirection::Right,
            scale,
            // Stride-to-speed matching: 120 px/sec per 1x scale
            max_walk_speed: 120.0 * scale,
            // Arrival dampening radius
            arrival_radius: 28.0 * scale,
        }
    }

    /// Transitions to a new animation clip.
    pub fn play(&mut self, clip: AnimationClip) {
        if self.current_clip == clip && !self.is_finished {
            return;
        }
        let spec = clip.spec();
        self.current_clip = clip;
        self.local_frame = 0;
        self.frame_index = spec.start_frame;
        self.frame_timer = 0.0;
        self.is_finished = false;
    }

    /// Advances the animation timer by delta time (clamped to prevent wake spikes).
    /// Returns `Some(frame_index)` if the rendered frame changed on this tick.
    pub fn update(&mut self, raw_dt: f32) -> Option<usize> {
        let dt = raw_dt.min(0.50); // Clamp large delta-times (e.g. system sleep/wake spikes)
        let spec = self.current_clip.spec();

        if self.is_finished && !spec.loops {
            return None;
        }

        self.frame_timer += dt;
        let mut frame_changed = false;

        while self.frame_timer >= spec.frame_duration {
            self.frame_timer -= spec.frame_duration;
            self.local_frame += 1;

            if self.local_frame >= spec.frame_count {
                if spec.loops {
                    self.local_frame = 0;
                } else {
                    self.local_frame = spec.frame_count - 1;
                    self.is_finished = true;
                    break;
                }
            }

            let new_frame = spec.start_frame + self.local_frame;
            if new_frame != self.frame_index {
                self.frame_index = new_frame;
                frame_changed = true;
            }
        }

        if frame_changed {
            Some(self.frame_index)
        } else {
            None
        }
    }

    /// Smooth ease-out steering towards a target X coordinate on the tab bar.
    ///
    /// Uses the mathematical arrival damping equation:
    /// v = v_max * max(0.25, |x_target - x| / arrival_radius)
    /// Returns true when arrived within 1.0 pixel of target.
    pub fn steer_horizontal(&mut self, target_x: f32, dt: f32) -> bool {
        let diff = target_x - self.pos_x;
        let dist = diff.abs();

        if dist < 1.0 {
            self.pos_x = target_x;
            return true;
        }

        // Update facing direction
        if diff > 0.0 {
            self.facing = FacingDirection::Right;
        } else if diff < 0.0 {
            self.facing = FacingDirection::Left;
        }

        // Ease-out arrival velocity
        let speed_factor = (dist / self.arrival_radius).clamp(0.25, 1.0);
        let velocity = self.max_walk_speed * speed_factor;
        let step = velocity * dt.min(0.05);

        if step >= dist {
            self.pos_x = target_x;
            true
        } else {
            self.pos_x += diff.signum() * step;
            false
        }
    }

    /// Integer rounded screen coordinates for UpdateLayeredWindow blitting.
    pub fn screen_coords(&self) -> (i32, i32) {
        (self.pos_x.round() as i32, self.pos_y.round() as i32)
    }

    /// Frame 11 is the exact Impact (paw-slap / snip) frame.
    pub fn is_impact_frame(&self) -> bool {
        self.frame_index == 11
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clip_progression() {
        let mut anim = Animator::new(2.0);
        anim.play(AnimationClip::Walk);
        assert_eq!(anim.frame_index, 2);

        // Step by 105ms (> 100ms frame duration)
        let changed = anim.update(0.105);
        assert_eq!(changed, Some(3));
        assert_eq!(anim.frame_index, 3);
    }

    #[test]
    fn test_ease_out_steering() {
        let mut anim = Animator::new(2.0);
        anim.pos_x = 0.0;
        let target = 100.0;

        // Several ticks of horizontal movement
        for _ in 0..10 {
            anim.steer_horizontal(target, 0.016);
        }

        assert!(anim.pos_x > 0.0);
        assert_eq!(anim.facing, FacingDirection::Right);
    }

    #[test]
    fn test_impact_frame_detection() {
        let mut anim = Animator::new(2.0);
        anim.play(AnimationClip::Impact);
        assert!(anim.is_impact_frame());
    }
}
