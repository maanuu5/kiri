use std::sync::mpsc::channel;

mod animator;
mod browser;
mod closer;
mod config;
mod detector;
mod overlay;
mod sprite;
mod uia_address;
mod uia_tab;
mod url_matcher;
mod watcher;

use config::{ActionMode, AppConfig, StrikeLevel};
use watcher::{spawn_watcher_thread, WatcherCommand, WatcherEvent};

fn main() {
    println!("==================================================");
    println!("   Kiri (切り) — Phase 1: UIA Inspector CLI");
    println!("==================================================");
    println!("Target Rules:");
    let config = AppConfig::default();
    for rule in &config.detector.target_rules {
        println!("  - {rule}");
    }
    println!();
    println!("Escalation thresholds:");
    println!(
        "  Strike 1: {} unique swipes OR {:.0}s dwell time",
        config.detector.strike_1_swipes, config.detector.strike_1_secs
    );
    println!(
        "  Strike 2: {} unique swipes OR {:.0}s dwell time",
        config.detector.strike_2_swipes, config.detector.strike_2_secs
    );
    println!(
        "  Strike 3: {} unique swipes OR {:.0}s dwell time -> Swat Tab",
        config.detector.strike_3_swipes, config.detector.strike_3_secs
    );
    println!("Action mode: {:?}", config.action_mode);
    println!("--------------------------------------------------");
    println!("Waiting for foreground Chromium browser window...");
    println!("Press Ctrl+C to terminate.\n");

    let (cmd_tx, cmd_rx) = channel();
    let (event_tx, event_rx) = channel();

    let _watcher_handle = spawn_watcher_thread(config.detector.clone(), cmd_rx, event_tx);

    let mut last_reported_strike = StrikeLevel::None;
    let mut swat_already_requested = false;
    let mut was_active = false;

    while let Ok(event) = event_rx.recv() {
        match event {
            WatcherEvent::Idle => {
                if was_active {
                    println!("[Browser] Lost foreground focus or entered fullscreen. Idle.");
                    was_active = false;
                }
            }
            WatcherEvent::Active(telemetry) => {
                was_active = true;

                // Log whenever URL or strike status updates
                if telemetry.detector.matched_rule.is_some() || telemetry.detector.current_strike.as_u8() > 0 {
                    let rule = telemetry.matched_rule.as_deref().unwrap_or("none");
                    let norm_url = telemetry.normalized_url.as_deref().unwrap_or("<unknown>");

                    println!(
                        "[DETECTED] Browser: {} (HWND: 0x{:X}) | Maximized: {} | Fullscreen: {}",
                        telemetry.process_name,
                        telemetry.hwnd,
                        telemetry.is_maximized,
                        telemetry.is_fullscreen
                    );
                    println!("  URL: {norm_url} (Matched: {rule})");
                    println!(
                        "  Dual Trigger: {} swipes (unique slugs) | {:.1}s active dwell",
                        telemetry.detector.swipe_count, telemetry.detector.dwell_secs
                    );
                    println!(
                        "  Escalation: {} (Level {})",
                        telemetry.detector.current_strike.label(),
                        telemetry.detector.current_strike.as_u8()
                    );

                    if let Some(strip) = telemetry.tab_strip_relative {
                        println!(
                            "  Tab Strip: [x: {}, y: {}, w: {}, h: {}] | Orientation: {:?}",
                            strip.rel_x, strip.rel_y, strip.width, strip.height,
                            telemetry.tab_strip_orientation.unwrap_or(uia_tab::TabStripOrientation::Horizontal)
                        );
                    }

                    if let Some(close_btn) = telemetry.close_button_relative {
                        println!(
                            "  Close Btn: [x: {}, y: {}, w: {}, h: {}] | Pinned: {}",
                            close_btn.rel_x, close_btn.rel_y, close_btn.width, close_btn.height,
                            telemetry.is_pinned
                        );
                    } else if telemetry.is_pinned {
                        println!("  Close Btn: <Pinned tab - no close button>");
                    }

                    // Check if Strike 3 reached and swat should be triggered
                    if telemetry.detector.current_strike == StrikeLevel::Strike3 && !swat_already_requested {
                        println!("\n>>> [STRIKE 3 TRIGGERED] Executing race-safe tab swat!");
                        if config.action_mode == ActionMode::Swat {
                            let _ = cmd_tx.send(WatcherCommand::ExecuteSwat);
                            swat_already_requested = true;
                        } else {
                            println!(">>> [NUDGE MODE] Swat simulated (tab left open).");
                        }
                    }

                    if telemetry.detector.current_strike == StrikeLevel::None {
                        swat_already_requested = false;
                    }

                    if telemetry.detector.current_strike != last_reported_strike {
                        last_reported_strike = telemetry.detector.current_strike;
                    }

                    println!("--------------------------------------------------");
                } else if telemetry.detector.cooled_down {
                    println!("[Cooldown] Inactivity threshold reached. Strike reset to baseline.");
                    last_reported_strike = StrikeLevel::None;
                    swat_already_requested = false;
                }
            }
            WatcherEvent::SwatExecuted(result) => {
                println!(">>> [SWAT RESULT]: {:?}", result);
                println!(">>> Single tab swat executed cleanly. Resetting to baseline.");
                println!("--------------------------------------------------");
            }
        }
    }

    let _ = cmd_tx.send(WatcherCommand::Stop);
}
