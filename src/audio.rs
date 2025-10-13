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

    log::info!("device {}", device.get_friendlyname()?);
    let mut audio_client = device.get_iaudioclient()?;
    let format = audio_client.get_mixformat()?;
    let bytes_per_frame = format.get_blockalign() as usize;
    let channels = format.get_nchannels() as usize;
    if channels != 2 {
        return Err(AudioRadarErrors::Internal("need stereo"));
    }

    let mode = StreamMode::PollingShared {
        autoconvert: true,
        buffer_duration_hns: 5_000_000,
    };
    audio_client.initialize_client(&format, &Direction::Capture, &mode)?;

    let capture = audio_client.get_audiocaptureclient()?;
    let mut left_buf = Vec::new();
    let mut right_buf = Vec::new();
    let window_samples = (format.get_samplespersec() / 5) as usize;
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
        let samples: &[f32] = unsafe {
            std::slice::from_raw_parts(buffer.as_ptr() as *const f32, buffer.len() / 4)
        };

        for chunk in samples.chunks_exact(channels) {
            left_buf.push(chunk[0]);
            right_buf.push(chunk[1]);
        }

        if left_buf.len() >= window_samples {
            let lrms = (left_buf.iter().map(|s| s * s).sum::<f32>() / left_buf.len() as f32).sqrt();
            let rrms =
                (right_buf.iter().map(|s| s * s).sum::<f32>() / right_buf.len() as f32).sqrt();
            let ild_db = 20.0 * ((rrms + 1e-6) / (lrms + 1e-6)).log10();
            let _ = tx_radar.send(RadarMessage::Direction(ild_db));
            left_buf.clear();
            right_buf.clear();
        }
    }
}
