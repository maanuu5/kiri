use uiautomation::controls::ControlType;
use uiautomation::core::UIAutomation;
use uiautomation::patterns::UISelectionItemPattern;
use uiautomation::types::{TreeScope, UIProperty};
use uiautomation::UIElement;

use crate::browser::WindowRect;

/// Orientation of the browser tab strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabStripOrientation {
    Horizontal,
    Vertical,
}

/// Telemetry and UI elements for the currently active browser tab.
#[derive(Clone)]
pub struct TabTelemetry {
    /// Bounding rectangle of the overall tab strip.
    pub tab_strip_rect: WindowRect,
    /// Orientation of the tab strip (Horizontal vs Vertical).
    pub orientation: TabStripOrientation,
    /// Bounding rectangle of the selected active tab item.
    pub active_tab_rect: WindowRect,
    /// Bounding rectangle of the tab's close button, if present.
    pub close_button_rect: Option<WindowRect>,
    /// Whether this tab is pinned (has no close button).
    pub is_pinned: bool,
    /// Live UIA element reference to the close button (for safe invocation).
    pub close_button_element: Option<UIElement>,
}

/// Discovers the browser's tab strip, identifies the active tab item,
/// and locates the language-agnostic Close button.
pub fn find_active_tab_telemetry(
    automation: &UIAutomation,
    root: &UIElement,
) -> Result<Option<TabTelemetry>, uiautomation::Error> {
    // 1. Locate Tab strip container
    let tab_condition = automation
        .create_property_condition(UIProperty::ControlType, (ControlType::Tab as i32).into(), None)?;

    let tab_strip = match root.find_first(TreeScope::Descendants, &tab_condition) {
        Ok(elem) => elem,
        Err(_) => return Ok(None),
    };

    let raw_strip_rect = tab_strip.get_bounding_rectangle()?;
    let tab_strip_rect = WindowRect {
        left: raw_strip_rect.get_left(),
        top: raw_strip_rect.get_top(),
        right: raw_strip_rect.get_right(),
        bottom: raw_strip_rect.get_bottom(),
    };

    let orientation = if tab_strip_rect.width() >= tab_strip_rect.height() {
        TabStripOrientation::Horizontal
    } else {
        TabStripOrientation::Vertical
    };

    // 2. Locate active/selected TabItem
    let tab_item_condition = automation
        .create_property_condition(UIProperty::ControlType, (ControlType::TabItem as i32).into(), None)?;

    let tab_items = tab_strip.find_all(TreeScope::Children, &tab_item_condition)?;

    let mut active_tab = None;
    for item in tab_items {
        if let Ok(selection_pattern) = item.get_pattern::<UISelectionItemPattern>() {
            if let Ok(is_selected) = selection_pattern.is_selected() {
                if is_selected {
                    active_tab = Some(item);
                    break;
                }
            }
        }
    }

    let active_item = match active_tab {
        Some(item) => item,
        None => return Ok(None),
    };

    let raw_tab_rect = active_item.get_bounding_rectangle()?;
    let active_tab_rect = WindowRect {
        left: raw_tab_rect.get_left(),
        top: raw_tab_rect.get_top(),
        right: raw_tab_rect.get_right(),
        bottom: raw_tab_rect.get_bottom(),
    };

    // 3. Language-Agnostic Close Button Locator
    // In Chromium tabs, the close button is the rightmost button on the tab item,
    // positioned past any audio/volume indicator or site icons.
    let button_condition = automation
        .create_property_condition(UIProperty::ControlType, (ControlType::Button as i32).into(), None)?;

    let buttons = active_item.find_all(TreeScope::Children, &button_condition)?;

    let mut rightmost_button = None;
    let mut max_right = active_tab_rect.left + (active_tab_rect.width() / 2);

    for btn in buttons {
        if let Ok(rect) = btn.get_bounding_rectangle() {
            let b_rect = WindowRect {
                left: rect.get_left(),
                top: rect.get_top(),
                right: rect.get_right(),
                bottom: rect.get_bottom(),
            };

            // Compare right edge so we always select the X button (far right)
            // instead of the audio/volume indicator button to its left.
            if b_rect.right > max_right {
                max_right = b_rect.right;
                rightmost_button = Some((b_rect, btn));
            }
        }
    }

    let (close_button_rect, close_button_element, is_pinned) = match rightmost_button {
        Some((rect, btn)) => (Some(rect), Some(btn), false),
        None => (None, None, true),
    };

    Ok(Some(TabTelemetry {
        tab_strip_rect,
        orientation,
        active_tab_rect,
        close_button_rect,
        is_pinned,
        close_button_element,
    }))
}
