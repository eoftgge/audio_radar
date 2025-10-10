use crate::errors::AudioRadarErrors;
use crate::types::RadarMessage;
use std::sync::mpsc::Sender;
use std::thread::sleep;
use std::time::Duration;
use wasapi::{Direction, StreamMode, get_default_device, initialize_mta};

pub fn start_capture_audio(tx_radar: Sender<RadarMessage>) -> Result<(), AudioRadarErrors> {
    initialize_mta()
        .ok()
        .map_err(|_| AudioRadarErrors::Internal("error in initialize_mta"))?;
    let device = get_default_device(&Direction::Render)?;
    let mut audio_client = device.get_iaudioclient()?;
    let format = audio_client.get_mixformat()?;
    let bytes_per_frame = format.get_blockalign() as usize;
    let channels = format.get_nchannels() as usize;
    assert_eq!(channels, 2, "need stereo");

    let mode = StreamMode::PollingShared {
        autoconvert: false,
        buffer_duration_hns: 1_000_000,
    };
    audio_client.initialize_client(&format, &Direction::Capture, &mode)?;

    let capture = audio_client.get_audiocaptureclient()?;
    let mut left_buf = Vec::new();
    let mut right_buf = Vec::new();
    let window_samples = (format.get_samplespersec() / 25) as usize;

    audio_client.start_stream()?;

    log::info!("Started audio stream!");
    loop {
        let frames = match capture.get_next_packet_size()? {
            Some(f) if f > 0 => f,
            _ => {
                sleep(Duration::from_millis(5));
                continue;
            }
        };

        let mut buffer = vec![0u8; frames as usize * bytes_per_frame];
        let _ = capture.read_from_device(&mut buffer)?;

        let samples: Vec<f32> = buffer
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();

        for chunk in samples.chunks_exact(channels) {
            left_buf.push(chunk[0]);
            right_buf.push(chunk[1]);
        }

        if left_buf.len() >= window_samples {
            let lrms = (left_buf.iter().map(|s| s * s).sum::<f32>() / left_buf.len() as f32).sqrt();
            let rrms =
                (right_buf.iter().map(|s| s * s).sum::<f32>() / right_buf.len() as f32).sqrt();

            let ild_db = 20.0 * ((lrms + 1e-6) / (rrms + 1e-6)).log10();
            let _ = tx_radar.send(RadarMessage::Direction(ild_db));

            left_buf.clear();
            right_buf.clear();
        }
    }
}
