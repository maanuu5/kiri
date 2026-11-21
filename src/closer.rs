use uiautomation::patterns::UIInvokePattern;
use uiautomation::UIElement;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_MENU, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

/// Result of an attempted tab close operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseResult {
    /// Succeeded using UIA InvokePattern on the exact close button.
    InvokedDirectly,
    /// Direct invoke failed or was unavailable, but safe Ctrl+W fallback succeeded.
    SentCtrlWFallback,
    /// Tab close was aborted because foreground window changed or modifiers were held.
    AbortedSafetyCheck,
    /// Tab was pinned or had no close target.
    SkippedPinned,
    /// COM invocation failed and fallback was bypassed.
    Failed(String),
}

/// Executes a race-safe tab closure.
///
/// Tries direct UIA InvokePattern on the specific tab's close button first.
/// If that is not possible, strictly verifies safety conditions before falling back
/// to synthesizing a Ctrl+W keystroke.
pub fn execute_safe_tab_close(
    close_button: Option<&UIElement>,
    target_hwnd: HWND,
    is_pinned: bool,
    url_still_matching: bool,
) -> CloseResult {
    if is_pinned {
        return CloseResult::SkippedPinned;
    }

    // 1. Primary Path: Direct UIA InvokePattern on the exact tab close button.
    // This is 100% race-safe: even if the user switched tabs 5ms earlier,
    // invoking that exact element either closes that specific tab or harmlessly fails.
    if let Some(btn) = close_button {
        if let Ok(pattern) = btn.get_pattern::<UIInvokePattern>() {
            if pattern.invoke().is_ok() {
                return CloseResult::InvokedDirectly;
            }
        }
    }

    // 2. Strict Fallback: Synthesize Ctrl+W ONLY if all strict safety conditions pass.
    unsafe {
        // Verification 1: Foreground HWND must still be the target browser
        let foreground = GetForegroundWindow();
        if foreground != target_hwnd {
            return CloseResult::AbortedSafetyCheck;
        }

        // Verification 2: Active URL must still match the doomscroll pattern
        if !url_still_matching {
            return CloseResult::AbortedSafetyCheck;
        }

        // Verification 3: User must not currently be pressing Ctrl, Alt, or Shift
        let ctrl_down = (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0;
        let alt_down = (GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0;
        let shift_down = (GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0;

        if ctrl_down || alt_down || shift_down {
            return CloseResult::AbortedSafetyCheck;
        }

        // Synthesize Ctrl + W
        let vk_w = VIRTUAL_KEY(0x57); // 'W' key

        let inputs = [
            make_key_input(VK_CONTROL, KEYBD_EVENT_FLAGS(0)),
            make_key_input(vk_w, KEYBD_EVENT_FLAGS(0)),
            make_key_input(vk_w, KEYEVENTF_KEYUP),
            make_key_input(VK_CONTROL, KEYEVENTF_KEYUP),
        ];

        let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        if sent == inputs.len() as u32 {
            CloseResult::SentCtrlWFallback
        } else {
            CloseResult::Failed("SendInput failed to send all keystrokes".to_string())
        }
    }
}

/// Helper to construct a Win32 KEYBDINPUT structure.
fn make_key_input(vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
