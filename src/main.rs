#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use audio_radar::audio::start_capture_audio;
use audio_radar::errors::AudioRadarErrors;
use audio_radar::gui::app::IndicatorApp;
use eframe::NativeOptions;
use eframe::egui::ViewportBuilder;
use eframe::icon_data::from_png_bytes;
use std::sync::mpsc;

const ICON_BYTES: &[u8] = include_bytes!("../assets/icon.png");

fn main() -> Result<(), AudioRadarErrors> {
    env_logger::init();
    let (tx, rx) = mpsc::channel();
    let options = NativeOptions {
        viewport: ViewportBuilder::default()
            .with_app_id("sublive")
            .with_icon(from_png_bytes(ICON_BYTES).expect("Failed to load icon"))
            .with_decorations(false)
            .with_always_on_top()
            .with_transparent(true)
            .with_maximized(true),
        ..Default::default()
    };

    std::thread::spawn(|| {
        if let Err(err) = start_capture_audio(tx) {
            log::error!("error in capture audio: {}", err);
        }
    });

    log::info!("running...");
    eframe::run_native(
        "AudioRadar",
        options,
        Box::new(|_cc| Ok(Box::new(IndicatorApp::new(rx)))),
    )?;
    Ok(())
}
