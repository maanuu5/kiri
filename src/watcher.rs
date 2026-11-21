use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use uiautomation::core::UIAutomation;
use uiautomation::types::Handle;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

use crate::browser::{get_foreground_browser_info, RelativeRect};
use crate::closer::{execute_safe_tab_close, CloseResult};
use crate::config::DetectorConfig;
use crate::detector::{DetectorStatus, DoomscrollDetector};
use crate::uia_address::AddressBarScanner;
use crate::uia_tab::{find_active_tab_telemetry, TabStripOrientation};
use crate::url_matcher::normalize_url;

/// Commands sent to the background watcher thread.
#[derive(Debug, Clone)]
pub enum WatcherCommand {
    /// Request immediate race-safe execution of the tab close.
    ExecuteSwat,
    /// Request clean shutdown of the watcher thread.
    Stop,
}

/// Rich snapshot of browser state and tab geometry delivered to the main/UI thread.
#[derive(Debug, Clone)]
pub struct BrowserTelemetry {
    pub process_name: String,
    pub hwnd: isize,
    pub is_maximized: bool,
    pub is_fullscreen: bool,
    pub current_url: Option<String>,
    pub normalized_url: Option<String>,
    pub matched_rule: Option<String>,
    pub detector: DetectorStatus,
    pub tab_strip_relative: Option<RelativeRect>,
    pub tab_strip_orientation: Option<TabStripOrientation>,
    pub close_button_relative: Option<RelativeRect>,
    pub is_pinned: bool,
}

/// Events produced by the watcher thread.
#[derive(Debug, Clone)]
pub enum WatcherEvent {
    /// Foreground window is not a monitored Chromium browser, or is fullscreen.
    Idle,
    /// Live telemetry update from an active browser window.
    Active(BrowserTelemetry),
    /// Result of an executed swat command.
    SwatExecuted(CloseResult),
}

/// Spawns the dedicated COM MTA thread running the 400ms polling watcher.
pub fn spawn_watcher_thread(
    config: DetectorConfig,
    cmd_rx: Receiver<WatcherCommand>,
    event_tx: Sender<WatcherEvent>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        // Initialize COM as Multi-Threaded Apartment on this dedicated thread
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }

        let automation = match UIAutomation::new() {
            Ok(a) => a,
            Err(e) => {
                eprintln!("[Kiri Watcher] Failed to initialize UIAutomation: {e}");
                unsafe { CoUninitialize() };
                return;
            }
        };

        let mut address_scanner = AddressBarScanner::new();
        let mut detector = DoomscrollDetector::new(config);

        let mut last_hwnd: Option<HWND> = None;
        let mut last_close_btn = None;
        let mut last_is_pinned = false;
        let mut last_url_matching = false;

        loop {
            // Check for incoming commands
            while let Ok(cmd) = cmd_rx.try_recv() {
                match cmd {
                    WatcherCommand::Stop => {
                        unsafe { CoUninitialize() };
                        return;
                    }
                    WatcherCommand::ExecuteSwat => {
                        if let Some(target_hwnd) = last_hwnd {
                            let result = execute_safe_tab_close(
                                last_close_btn.as_ref(),
                                target_hwnd,
                                last_is_pinned,
                                last_url_matching,
                            );
                            let _ = event_tx.send(WatcherEvent::SwatExecuted(result));
                        }
                    }
                }
            }

            let start_tick = Instant::now();

            // 1. Inspect foreground window
            if let Some(browser) = get_foreground_browser_info() {
                // If browser changed, invalidate cached address bar UIA element
                if last_hwnd != Some(browser.hwnd) {
                    address_scanner.invalidate();
                    last_hwnd = Some(browser.hwnd);
                }

                if browser.is_fullscreen {
                    let _ = event_tx.send(WatcherEvent::Idle);
                } else {
                    let mut current_url = None;
                    let mut norm_url = None;

                    // 2. Discover address bar URL via language-agnostic UIA Edit search
                    let handle: Handle = unsafe { std::mem::transmute(browser.hwnd) };
                    if let Ok(root_elem) = automation.element_from_handle(handle) {
                        if let Ok(Some(raw_url)) =
                            address_scanner.read_url(&automation, &root_elem, browser.hwnd.0 as isize)
                        {
                            let normalized = normalize_url(&raw_url);
                            norm_url = Some(normalized.clone());
                            current_url = Some(raw_url);
                        }

                        // 3. Update dual-trigger detector
                        let status = detector.update(norm_url.as_deref(), Instant::now());
                        last_url_matching = status.matched_rule.is_some();

                        let mut tab_strip_relative = None;
                        let mut tab_strip_orientation = None;
                        let mut close_button_relative = None;
                        let mut is_pinned = false;

                        // 4. Query tab strip and close button geometry when active
                        if status.matched_rule.is_some() || status.current_strike.as_u8() > 0 {
                            if let Ok(Some(tab_info)) =
                                find_active_tab_telemetry(&automation, &root_elem)
                            {
                                tab_strip_relative = Some(RelativeRect::from_screen(
                                    tab_info.tab_strip_rect,
                                    browser.window_rect,
                                ));
                                tab_strip_orientation = Some(tab_info.orientation);
                                close_button_relative = tab_info
                                    .close_button_rect
                                    .map(|r| RelativeRect::from_screen(r, browser.window_rect));
                                is_pinned = tab_info.is_pinned;

                                // Cache close button element for race-safe execution
                                last_close_btn = tab_info.close_button_element;
                                last_is_pinned = is_pinned;
                            }
                        } else {
                            last_close_btn = None;
                        }

                        let telemetry = BrowserTelemetry {
                            process_name: browser.process_name,
                            hwnd: browser.hwnd.0 as isize,
                            is_maximized: browser.is_maximized,
                            is_fullscreen: browser.is_fullscreen,
                            current_url,
                            normalized_url: norm_url,
                            matched_rule: status.matched_rule.clone(),
                            detector: status,
                            tab_strip_relative,
                            tab_strip_orientation,
                            close_button_relative,
                            is_pinned,
                        };

                        let _ = event_tx.send(WatcherEvent::Active(telemetry));
                    }
                }
            } else {
                last_hwnd = None;
                last_close_btn = None;
                address_scanner.invalidate();
                // Feed None to detector to trigger cooldown decay when away from browser
                let _ = detector.update(None, Instant::now());
                let _ = event_tx.send(WatcherEvent::Idle);
            }

            // Sleep remainder of ~400ms polling cycle
            let elapsed = start_tick.elapsed();
            if elapsed < Duration::from_millis(400) {
                thread::sleep(Duration::from_millis(400) - elapsed);
            }
        }
    })
}
