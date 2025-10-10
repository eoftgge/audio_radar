use audio_radar::audio::start_capture_audio;
use audio_radar::handler::handler;
use audio_radar::types::RadarMessage;
use simple_file_logger::{init_logger, LogLevel};

fn main() {
    init_logger("audio_radar", LogLevel::Info).unwrap();
    let (tx_radar, rx_radar) = std::sync::mpsc::channel::<RadarMessage>();
    std::thread::spawn(move || start_capture_audio(tx_radar));

    if let Err(err) = handler(rx_radar) {
        log::error!("{}", err);
        log::warn!("aborting...");
    }
}
