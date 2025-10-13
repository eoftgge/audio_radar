#![windows_subsystem = "windows"]

use audio_radar::audio::start_capture_audio;
use audio_radar::handler::handler;
use audio_radar::types::RadarMessage;
use simple_file_logger::{LogLevel, init_logger};
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
use windows::core::PCWSTR;

fn show_error(msg: &str) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    let wide: Vec<u16> = OsStr::new(msg).encode_wide().chain(Some(0)).collect();

    unsafe {
        MessageBoxW(
            None,
            PCWSTR(wide.as_ptr()),
            PCWSTR(wide.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn main() {
    init_logger("audio_radar", LogLevel::Debug).unwrap();
    let (tx_radar, rx_radar) = std::sync::mpsc::channel::<RadarMessage>();
    std::thread::spawn(move || start_capture_audio(tx_radar));

    log::info!("Starting the program");
    if let Err(err) = handler(rx_radar) {
        log::error!("{}", err);
        log::warn!("aborting...");
        show_error(&format!("{}", err));
    }
}
