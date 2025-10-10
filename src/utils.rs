use windows::Win32::Foundation::COLORREF;

pub fn colorref_from_rgb(r: u8, g: u8, b: u8) -> COLORREF {
    // Windows RGB macro: (r) | (g << 8) | (b << 16)
    COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16))
}