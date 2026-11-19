use uiautomation::core::UIAutomation;
use uiautomation::patterns::ValuePattern;
use uiautomation::types::ControlType;
use uiautomation::UIElement;

/// Scanner and cache for a browser window's address bar edit control.
#[derive(Default)]
pub struct AddressBarScanner {
    cached_hwnd: Option<isize>,
    cached_element: Option<UIElement>,
}

impl AddressBarScanner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears any cached UIA references.
    pub fn invalidate(&mut self) {
        self.cached_hwnd = None;
        self.cached_element = None;
    }

    /// Retrieves the current text/URL from the browser's address bar.
    ///
    /// Uses a language-agnostic search for `ControlType::Edit` controls that
    /// support `ValuePattern`. Caches the handle per HWND to avoid traversing
    /// the UIA tree on every tick.
    pub fn read_url(
        &mut self,
        automation: &UIAutomation,
        root: &UIElement,
        hwnd: isize,
    ) -> Result<Option<String>, uiautomation::Error> {
        // Fast path: attempt to read from cached element
        if self.cached_hwnd == Some(hwnd) {
            if let Some(ref elem) = self.cached_element {
                if let Ok(value_pattern) = elem.get_pattern::<ValuePattern>() {
                    if let Ok(val) = value_pattern.get_value() {
                        if !val.is_empty() {
                            return Ok(Some(val));
                        }
                    }
                }
            }
            // If cache was present but reading failed or was empty, invalidate and rescan
            self.invalidate();
        }

        // Slow path: Locate address bar Edit control supporting ValuePattern
        let edit_condition = automation
            .create_property_condition(
                uiautomation::types::UIProperty::ControlType,
                ControlType::Edit.into(),
            )?;

        // Search descendant controls for the address bar
        if let Ok(edits) = root.find_all(uiautomation::types::TreeScope::Descendants, &edit_condition) {
            for edit in edits {
                if let Ok(pattern) = edit.get_pattern::<ValuePattern>() {
                    if let Ok(val) = pattern.get_value() {
                        // In Chromium browsers, the address bar often contains host names or query paths.
                        // We store the first valid edit control that supports ValuePattern.
                        self.cached_hwnd = Some(hwnd);
                        self.cached_element = Some(edit);
                        return Ok(Some(val));
                    }
                }
            }
        }

        Ok(None)
    }
}
