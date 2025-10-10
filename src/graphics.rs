use crate::utils::colorref_from_rgb;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SYSTEM_METRICS_INDEX};

pub(crate) fn draw_indicator(hwnd: HWND, ild_db: f32) {
    unsafe {
        let hdc = GetDC(Some(hwnd));
        let rect = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        FillRect(hdc, &rect, HBRUSH(GetStockObject(BLACK_BRUSH).0));
        let clamped = ild_db.clamp(-10.0, 10.0);
        let screen_width = GetSystemMetrics(SYSTEM_METRICS_INDEX(0));
        let screen_height = GetSystemMetrics(SYSTEM_METRICS_INDEX(1));
        let cx = screen_width as f32 / 2.0;
        let cy = screen_height as f32 / 4.0;
        let length = 60.0;
        let x2 = cx + clamped / 10.0 * length;
        let y2 = cy;

        let color = if clamped.abs() < 3.0 {
            colorref_from_rgb(0, 255, 0)
        } else {
            colorref_from_rgb(255, 0, 0)
        };

        let pen = CreatePen(PS_SOLID, 6, color);
        let old_pen = SelectObject(hdc, HGDIOBJ(pen.0));

        MoveToEx(hdc, cx as i32, cy as i32, None).unwrap();
        LineTo(hdc, x2 as i32, y2 as i32).unwrap();

        SelectObject(hdc, old_pen);
        DeleteObject(HGDIOBJ(pen.0)).unwrap();
        ReleaseDC(Some(hwnd), hdc);
    }
}
