use std::sync::OnceLock;

use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconFromResourceEx, DrawIconEx, DI_NORMAL, HICON, LR_DEFAULTCOLOR,
};

pub const SIZE: i32 = 16;
pub const RIGHT_MARGIN: i32 = 5;

pub fn draw(hdc: HDC, x: i32, y: i32, size: i32) {
    draw_icon(hdc, x, y, size, original_icon());
}

fn original_icon() -> HICON {
    static CODEX: OnceLock<isize> = OnceLock::new();
    let png = include_bytes!("icons/providers/openai.png");
    let handle = *CODEX.get_or_init(|| unsafe {
        CreateIconFromResourceEx(png, true, 0x00030000, 64, 64, LR_DEFAULTCOLOR)
            .map(|icon| icon.0 as isize)
            .unwrap_or(0)
    });
    HICON(handle as *mut _)
}

fn draw_icon(hdc: HDC, x: i32, y: i32, size: i32, icon: HICON) {
    if !icon.is_invalid() {
        unsafe {
            let _ = DrawIconEx(hdc, x, y, icon, size, size, 0, HBRUSH::default(), DI_NORMAL);
        }
    }
}
