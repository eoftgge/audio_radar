use crate::utils::colorref_from_rgb;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SYSTEM_METRICS_INDEX};

fn draw_arrow(hdc: HDC, cx: f32, cy: f32, ild_db: f32) {
    let max_ild = 10.0;
    let clamped = ild_db.clamp(-max_ild, max_ild);

    let base_width = 10.0;
    let max_length = 60.0;

    let length = (clamped.abs() / max_ild) * max_length;

    let tip_x = cx + if clamped >= 0.0 { length } else { -length };
    let tip_y = cy;

    let base_left_x = cx;
    let base_left_y = cy - base_width;
    let base_right_x = cx;
    let base_right_y = cy + base_width;

    let points = [
        POINT { x: tip_x as i32, y: tip_y as i32 },
        POINT { x: base_left_x as i32, y: base_left_y as i32 },
        POINT { x: base_right_x as i32, y: base_right_y as i32 },
    ];

    unsafe {
        let color = if clamped.abs() < 3.0 {
            colorref_from_rgb(0, 255, 0)
        } else if clamped.abs() < 7.0 {
            colorref_from_rgb(255, 255, 0)
        } else {
            colorref_from_rgb(255, 0, 0)
        };
        let brush = CreateSolidBrush(color);
        let old_brush = SelectObject(hdc, HGDIOBJ(brush.0));
        let _ = Polygon(hdc, &points);
        SelectObject(hdc, old_brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }
}

pub(crate) fn draw_indicator(hwnd: HWND, ild_db: f32) {
    unsafe {
        let hdc = GetDC(Some(hwnd));

        let screen_width = GetSystemMetrics(SYSTEM_METRICS_INDEX(0));
        let screen_height = GetSystemMetrics(SYSTEM_METRICS_INDEX(1));
        let rect = RECT { left: 0, top: 0, right: screen_width, bottom: screen_height };
        FillRect(hdc, &rect, HBRUSH(GetStockObject(BLACK_BRUSH).0));

        let screen_width = unsafe { GetSystemMetrics(SYSTEM_METRICS_INDEX(0)) } as f32;
        let screen_height = unsafe { GetSystemMetrics(SYSTEM_METRICS_INDEX(1)) } as f32;
        let cx = screen_width / 2.0;
        let cy = screen_height / 4.0;

        draw_arrow(hdc, cx, cy, ild_db);

        ReleaseDC(Some(hwnd), hdc);
    }
}