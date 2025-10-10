use std::sync::mpsc::Receiver;
use std::time::Duration;
use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use crate::graphics::draw_indicator;
use crate::types::RadarMessage;
use crate::utils::colorref_from_rgb;

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn create_overlay_window() -> HWND {
    unsafe {
        let hinstance = GetModuleHandleW(None).unwrap();
        let class_name = w!("AudioRadar");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let width = GetSystemMetrics(SYSTEM_METRICS_INDEX(0));
        let height = GetSystemMetrics(SYSTEM_METRICS_INDEX(1));

        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name,
            w!("SoundOverlay"),
            WS_POPUP,
            0,
            0,
            width,
            height,
            None,
            None,
            Some(hinstance.into()),
            None,
        ).unwrap();
        SetLayeredWindowAttributes(hwnd, colorref_from_rgb(0, 0, 0), 0, LWA_COLORKEY).unwrap();
        let _ = ShowWindow(hwnd, SW_SHOW);
        hwnd
    }
}

pub fn overlay_loop(rx: Receiver<RadarMessage>) {
    let mut current_dir = 0.0f32;
    let hwnd = create_overlay_window();

    log::info!("Starting overlay loop");
    loop {
        if let Ok(msg) = rx.try_recv() {
            if let RadarMessage::Direction(ild_db) = msg {
                current_dir = ild_db;
            }
        }

        draw_indicator(hwnd, current_dir);
        std::thread::sleep(Duration::from_millis(30));
    }
}