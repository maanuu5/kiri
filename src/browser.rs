use std::path::Path;
use windows::Win32::Foundation::{CloseHandle, HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsZoomed,
};

/// Supported Chromium-based browser process names.
pub const SUPPORTED_BROWSERS: &[&str] = &[
    "chrome.exe",
    "msedge.exe",
    "brave.exe",
    "vivaldi.exe",
];

/// Screen-space rectangle representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WindowRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl WindowRect {
    pub fn from_win32(r: RECT) -> Self {
        Self {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.bottom,
        }
    }

    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

/// Window-relative coordinates (offset from parent browser window top-left).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RelativeRect {
    pub rel_x: i32,
    pub rel_y: i32,
    pub width: i32,
    pub height: i32,
}

impl RelativeRect {
    pub fn from_screen(screen_rect: WindowRect, window_rect: WindowRect) -> Self {
        Self {
            rel_x: screen_rect.left - window_rect.left,
            rel_y: screen_rect.top - window_rect.top,
            width: screen_rect.width(),
            height: screen_rect.height(),
        }
    }
}

/// Real-time metadata for a detected foreground browser window.
#[derive(Debug, Clone)]
pub struct BrowserWindowInfo {
    pub hwnd: HWND,
    pub process_name: String,
    pub window_rect: WindowRect,
    pub is_maximized: bool,
    pub is_fullscreen: bool,
}

/// Inspects the current foreground window.
/// If it matches a supported Chromium browser and is not minimized, returns its details.
pub fn get_foreground_browser_info() -> Option<BrowserWindowInfo> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() || IsIconic(hwnd).as_bool() {
            return None;
        }

        let mut process_id: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));
        if process_id == 0 {
            return None;
        }

        let proc_handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).ok()?;
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;

        let query_res = QueryFullProcessImageNameW(
            proc_handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(proc_handle);

        if query_res.is_err() || size == 0 {
            return None;
        }

        let full_path = String::from_utf16_lossy(&buffer[..size as usize]);
        let exe_name = Path::new(&full_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if !SUPPORTED_BROWSERS.contains(&exe_name.as_str()) {
            return None;
        }

        let mut win_rect = RECT::default();
        if GetWindowRect(hwnd, &mut win_rect).is_err() {
            return None;
        }
        let window_rect = WindowRect::from_win32(win_rect);
        let is_maximized = IsZoomed(hwnd).as_bool();

        // Check if window covers the entire monitor (true fullscreen video/mode)
        let is_fullscreen = is_fullscreen_window(hwnd, &win_rect);

        Some(BrowserWindowInfo {
            hwnd,
            process_name: exe_name,
            window_rect,
            is_maximized,
            is_fullscreen,
        })
    }
}

/// Determines if a window RECT matches or exceeds monitor bounds.
unsafe fn is_fullscreen_window(hwnd: HWND, win_rect: &RECT) -> bool {
    let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
    if monitor.0.is_null() {
        return false;
    }

    let mut mi = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };

    if GetMonitorInfoW(monitor, &mut mi).as_bool() {
        win_rect.left <= mi.rcMonitor.left
            && win_rect.top <= mi.rcMonitor.top
            && win_rect.right >= mi.rcMonitor.right
            && win_rect.bottom >= mi.rcMonitor.bottom
    } else {
        false
    }
}
