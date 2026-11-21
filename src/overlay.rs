use std::ptr::null_mut;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, AC_SRC_ALPHA,
    AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP,
    HDC, HGDIOBJ, RGBQUAD,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassExW, ShowWindow,
    UpdateLayeredWindow, HCURSOR, HICON, SW_HIDE, SW_SHOWNA, ULW_ALPHA, WNDCLASSEXW,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

const WINDOW_CLASS_NAME: &windows::core::PCWSTR = &w!("KiriCatOverlayWindowClass");

/// Win32 layered, transparent, click-through overlay window for rendering the pixel cat.
pub struct CatOverlay {
    pub hwnd: HWND,
    mem_dc: HDC,
    dib_bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
    pixel_buffer: *mut u8,
    pub width: i32,
    pub height: i32,
    pub is_visible: bool,
}

impl CatOverlay {
    /// Creates a tiny borderless layered window sized to the scaled sprite.
    pub fn new(width: i32, height: i32) -> Result<Self, windows::core::Error> {
        unsafe {
            let instance = GetModuleHandleW(None)?;

            let wnd_class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: windows::Win32::UI::WindowsAndMessaging::WNDCLASS_STYLES(0),
                lpfnWndProc: Some(overlay_wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: instance.into(),
                hIcon: HICON(null_mut()),
                hCursor: HCURSOR(null_mut()),
                hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH(null_mut()),
                lpszMenuName: windows::core::PCWSTR::null(),
                lpszClassName: *WINDOW_CLASS_NAME,
                hIconSm: HICON(null_mut()),
            };

            // Register window class (ignore error if already registered)
            let _ = RegisterClassExW(&wnd_class);

            // Extended styles: Layered, transparent (click-through), topmost, toolwindow, no-activate
            let ex_style = WS_EX_LAYERED
                | WS_EX_TRANSPARENT
                | WS_EX_TOPMOST
                | WS_EX_TOOLWINDOW
                | WS_EX_NOACTIVATE;

            let hwnd = CreateWindowExW(
                ex_style,
                *WINDOW_CLASS_NAME,
                w!("Kiri Cat"),
                WS_POPUP,
                -1000,
                -1000,
                width,
                height,
                None,
                None,
                Some(instance.into()),
                None,
            )?;

            let mem_dc = CreateCompatibleDC(None);
            let (dib_bitmap, pixel_buffer) = create_dib_section(mem_dc, width, height);
            let old_bitmap = SelectObject(mem_dc, dib_bitmap.into());

            Ok(Self {
                hwnd,
                mem_dc,
                dib_bitmap,
                old_bitmap,
                pixel_buffer,
                width,
                height,
                is_visible: false,
            })
        }
    }

    /// Blits a premultiplied BGRA frame to the layered window at signed screen coordinates.
    pub fn update_frame(&mut self, screen_x: i32, screen_y: i32, bgra_pixels: &[u8]) {
        unsafe {
            let buffer_len = (self.width * self.height * 4) as usize;
            if bgra_pixels.len() >= buffer_len && !self.pixel_buffer.is_null() {
                // Copy frame pixels into top-down DIB memory buffer
                std::ptr::copy_nonoverlapping(bgra_pixels.as_ptr(), self.pixel_buffer, buffer_len);

                let pt_dst = POINT {
                    x: screen_x,
                    y: screen_y,
                };
                let size_dst = SIZE {
                    cx: self.width,
                    cy: self.height,
                };
                let pt_src = POINT { x: 0, y: 0 };

                let blend = BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: 255,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                };

                let _ = UpdateLayeredWindow(
                    self.hwnd,
                    None,
                    Some(&pt_dst),
                    Some(&size_dst),
                    Some(self.mem_dc),
                    Some(&pt_src),
                    COLORREF(0),
                    Some(&blend),
                    ULW_ALPHA,
                );
            }
        }
    }

    /// Shows the overlay window without stealing focus.
    pub fn show(&mut self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWNA);
            self.is_visible = true;
        }
    }

    /// Hides the overlay window (returns to 0.0% idle CPU).
    pub fn hide(&mut self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
            self.is_visible = false;
        }
    }

    /// Re-allocates the underlying DIB section when DPI scale changes.
    pub fn resize(&mut self, new_width: i32, new_height: i32) {
        if self.width == new_width && self.height == new_height {
            return;
        }

        unsafe {
            SelectObject(self.mem_dc, self.old_bitmap);
            let _ = DeleteObject(self.dib_bitmap.into());

            self.width = new_width;
            self.height = new_height;

            let (dib, ptr) = create_dib_section(self.mem_dc, new_width, new_height);
            self.dib_bitmap = dib;
            self.pixel_buffer = ptr;
            self.old_bitmap = SelectObject(self.mem_dc, dib.into());
        }
    }
}

impl Drop for CatOverlay {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.mem_dc, self.old_bitmap);
            let _ = DeleteObject(self.dib_bitmap.into());
            let _ = DeleteDC(self.mem_dc);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

/// Helper to create a 32-bit top-down DIB section for UpdateLayeredWindow.
unsafe fn create_dib_section(hdc: HDC, width: i32, height: i32) -> (HBITMAP, *mut u8) {
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height, // Negative height indicates top-down scanlines
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [RGBQUAD::default()],
    };

    let mut bits: *mut std::ffi::c_void = null_mut();
    let dib = CreateDIBSection(
        Some(hdc),
        &mut bmi,
        DIB_RGB_COLORS,
        &mut bits,
        None,
        0,
    )
    .expect("Failed to create 32-bit DIB section");

    (dib, bits as *mut u8)
}

/// Minimal DefWindowProc message handler for borderless popup window.
unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}
