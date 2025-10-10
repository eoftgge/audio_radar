use audio_radar::types::RadarMessage;
use audio_radar::audio::start_capture_audio;
use audio_radar::handler::overlay_loop;

fn main() {
    simple_logger::init().unwrap();
    let (tx_radar, rx_radar) = std::sync::mpsc::channel::<RadarMessage>();
    std::thread::spawn(move || start_capture_audio(tx_radar));
    overlay_loop(rx_radar);
}
