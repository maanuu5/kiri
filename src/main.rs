use std::sync::mpsc::channel;
use std::thread::sleep;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
};

#[link(name = "user32")]
extern "system" {
    fn MessageBeep(uType: u32) -> i32;
}

mod animator;
mod browser;
mod cat_fsm;
mod closer;
mod config;
mod detector;
mod overlay;
mod sprite;
mod uia_address;
mod uia_tab;
mod url_matcher;
mod watcher;

use cat_fsm::{CatFsm, CatState, FsmAction};
use config::{ActionMode, AppConfig};
use overlay::CatOverlay;
use sprite::SpriteBank;
use watcher::{spawn_watcher_thread, WatcherCommand, WatcherEvent};

fn compute_scale(hwnd: HWND) -> u32 {
    if hwnd.0.is_null() {
        return 2;
    }
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi >= 192 {
        4
    } else if dpi >= 144 {
        3
    } else {
        2
    }
}

fn pump_windows_messages() {
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn main() {
    println!("==================================================");
    println!("   Kiri (切り) — Pixel Cat Doomscroll Patrol");
    println!("==================================================");
    let config = AppConfig::default();
    println!("Target Rules:");
    for rule in &config.detector.target_rules {
        println!("  - {rule}");
    }
    println!();
    println!("Escalation thresholds:");
    println!(
        "  Strike 1 (Peeking):    {} unique swipes OR {:.0}s dwell time",
        config.detector.strike_1_swipes, config.detector.strike_1_secs
    );
    println!(
        "  Strike 2 (Patrolling): {} unique swipes OR {:.0}s dwell time",
        config.detector.strike_2_swipes, config.detector.strike_2_secs
    );
    println!(
        "  Strike 3 (Swatting):   {} unique swipes OR {:.0}s dwell time -> Swat Tab",
        config.detector.strike_3_swipes, config.detector.strike_3_secs
    );
    println!("Action mode: {:?}", config.action_mode);
    println!("--------------------------------------------------");
    println!("Watching foreground Chromium windows at 60 FPS...");
    println!("Press Ctrl+C to terminate.\n");

    let (cmd_tx, cmd_rx) = channel();
    let (event_tx, event_rx) = channel();

    let _watcher_handle = spawn_watcher_thread(config.detector.clone(), cmd_rx, event_tx);

    let mut current_scale = 2;
    let mut sprite_bank = SpriteBank::new(current_scale);
    let mut overlay = match CatOverlay::new(sprite_bank.width(), sprite_bank.height()) {
        Ok(ov) => ov,
        Err(e) => {
            eprintln!("Failed to initialize transparent layered overlay: {e}");
            return;
        }
    };
    let mut fsm = CatFsm::new(current_scale);

    let mut last_tick = Instant::now();
    let target_frame_duration = Duration::from_micros(16_667); // 60 FPS = ~16.6ms

    loop {
        pump_windows_messages();

        if fsm.state == CatState::Hidden {
            // When hidden: stop 60 FPS loop, hide overlay, block on incoming telemetry (0.0% CPU)
            if overlay.is_visible {
                overlay.hide();
            }

            match event_rx.recv() {
                Ok(event) => match event {
                    WatcherEvent::Idle => {}
                    WatcherEvent::Active(telemetry) => {
                        // Check DPI scaling on target window
                        let target_hwnd = HWND(telemetry.hwnd as *mut _);
                        let desired_scale = compute_scale(target_hwnd);
                        if desired_scale != current_scale {
                            current_scale = desired_scale;
                            sprite_bank = SpriteBank::new(current_scale);
                            overlay.resize(sprite_bank.width(), sprite_bank.height());
                            fsm.scale = current_scale as f32;
                            fsm.animator.scale = current_scale as f32;
                            fsm.animator.max_walk_speed = 120.0 * (current_scale as f32);
                            fsm.animator.arrival_radius = 28.0 * (current_scale as f32);
                        }

                        fsm.apply_telemetry(&telemetry);
                        last_tick = Instant::now();
                    }
                    WatcherEvent::SwatExecuted(result) => {
                        println!(">>> [SWAT RESULT]: {:?}", result);
                    }
                },
                Err(_) => break, // Channel closed, exit cleanly
            }
        } else {
            // Cat is visible: run 60 FPS sub-pixel loop
            let frame_start = Instant::now();
            let raw_dt = frame_start.duration_since(last_tick).as_secs_f32();
            last_tick = frame_start;
            let dt = raw_dt.clamp(0.001, 0.05);

            // Drain any pending 400ms telemetry events without blocking
            while let Ok(event) = event_rx.try_recv() {
                match event {
                    WatcherEvent::Idle => {
                        fsm.transition_to(CatState::Hidden);
                    }
                    WatcherEvent::Active(telemetry) => {
                        fsm.apply_telemetry(&telemetry);
                    }
                    WatcherEvent::SwatExecuted(result) => {
                        println!(">>> [SWAT RESULT]: {:?}", result);
                    }
                }
            }

            // Update state machine physics and animations
            let action = fsm.update(dt);

            match action {
                FsmAction::None => {}
                FsmAction::ExecuteSwat => {
                    println!("\n==================================================");
                    println!(">>> [IMPACT FRAME 11] Paw slams down on 'X' button!");
                    println!("==================================================");

                    if config.action_mode == ActionMode::Swat {
                        let _ = cmd_tx.send(WatcherCommand::ExecuteSwat);
                    } else {
                        println!(">>> [NUDGE MODE] Swat simulated (tab left open).");
                    }

                    if config.sound_enabled {
                        unsafe {
                            let _ = MessageBeep(0);
                        }
                    }
                }
                FsmAction::HideOverlay => {
                    overlay.hide();
                }
            }

            // Render current frame to layered window
            if fsm.state != CatState::Hidden {
                if !overlay.is_visible {
                    overlay.show();
                }

                let pixels = sprite_bank.get_frame(fsm.animator.frame_index, fsm.animator.facing);
                let (screen_x, screen_y) = fsm.animator.screen_coords();
                overlay.update_frame(screen_x, screen_y, pixels);
            }

            // High precision frame pacing for smooth 60 FPS
            let elapsed = frame_start.elapsed();
            if elapsed < target_frame_duration {
                sleep(target_frame_duration - elapsed);
            }
        }
    }

    let _ = cmd_tx.send(WatcherCommand::Stop);
}
