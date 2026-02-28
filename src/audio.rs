use crate::errors::AudioRadarErrors;
use crate::types::RadarMessage;
use std::sync::mpsc::Sender;
use std::time::Duration;
use cpal::{Sample, SizedSample, Stream};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub fn start_capture_audio(tx: Sender<RadarMessage>) -> Result<(), AudioRadarErrors> {
    let host = cpal::default_host();

    let device = host.default_output_device()
        .ok_or_else(|| AudioRadarErrors::from("Not found default audio output device"))?;

    let config = device.default_output_config()?;
    log::info!("Format Audio: {:?}", config);

    let channels = config.channels() as usize;
    let stream_config: cpal::StreamConfig = config.clone().into();
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => build_stream::<f32>(&device, &stream_config, tx, channels)?,
        cpal::SampleFormat::I16 => build_stream::<i16>(&device, &stream_config, tx, channels)?,
        cpal::SampleFormat::U16 => build_stream::<u16>(&device, &stream_config, tx, channels)?,
        sample_format => return Err(AudioRadarErrors::Internal(format!("Unsupported sample format {:?}", sample_format))),
    };

    stream.play()?;
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    tx: Sender<RadarMessage>,
    channels: usize,
) -> Result<Stream, AudioRadarErrors>
where
    T: Sample<Float = f32> + SizedSample,
{
    let mut prev_x = 0.0;
    let mut prev_y = 0.0;
    let mut prev_db = 0.0;
    let smoothing_factor = 0.3;

    let err_fn = |err| log::error!("Ошибка в аудио-потоке: {}", err);

    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let mut frames = 0;
            let mut sum_fl = 0.0; // Front Left
            let mut sum_fr = 0.0; // Front Right
            let mut sum_fc = 0.0; // Front Center
            let mut sum_bl = 0.0; // Back Left
            let mut sum_br = 0.0; // Back Right
            let mut sum_sl = 0.0; // Side Left
            let mut sum_sr = 0.0; // Side Right

            for frame in data.chunks_exact(channels) {
                if channels == 8 {
                    sum_fl += frame[0].to_float_sample().powi(2);
                    sum_fr += frame[1].to_float_sample().powi(2);
                    sum_fc += frame[2].to_float_sample().powi(2);
                    sum_bl += frame[4].to_float_sample().powi(2);
                    sum_br += frame[5].to_float_sample().powi(2);
                    sum_sl += frame[6].to_float_sample().powi(2);
                    sum_sr += frame[7].to_float_sample().powi(2);
                    frames += 1;
                } else if channels >= 2 {
                    sum_fl += frame[0].to_float_sample().powi(2);
                    sum_fr += frame[1].to_float_sample().powi(2);
                    frames += 1;
                }
            }

            if frames > 0 {
                if channels == 8 {
                    let f_frames = frames as f32;
                    let fl = (sum_fl / f_frames).sqrt();
                    let fr = (sum_fr / f_frames).sqrt();
                    let fc = (sum_fc / f_frames).sqrt();
                    let bl = (sum_bl / f_frames).sqrt();
                    let br = (sum_br / f_frames).sqrt();
                    let sl = (sum_sl / f_frames).sqrt();
                    let sr = (sum_sr / f_frames).sqrt();

                    let total_intensity = fl + fr + fc + bl + br + sl + sr;

                    if total_intensity > 0.002 {
                        let right = fr + br + sr;
                        let left = fl + bl + sl;
                        let raw_x = right - left;

                        let front = fl + fr + fc;
                        let back = bl + br;
                        let raw_y = front - back;

                        prev_x += smoothing_factor * (raw_x - prev_x);
                        prev_y += smoothing_factor * (raw_y - prev_y);

                        let _ = tx.send(RadarMessage::Surround {
                            x: prev_x,
                            y: prev_y,
                            intensity: total_intensity
                        });
                    } else {
                        prev_x *= 0.9;
                        prev_y *= 0.9;
                        let _ = tx.send(RadarMessage::Surround { x: prev_x, y: prev_y, intensity: 0.0 });
                    }
                } else {
                    let l_rms = (sum_fl / frames as f32).sqrt();
                    let r_rms = (sum_fr / frames as f32).sqrt();

                    if l_rms > 0.0005 || r_rms > 0.0005 {
                        let raw_db = 20.0 * ((r_rms + 1e-6) / (l_rms + 1e-6)).log10();
                        let smoothed_val = prev_db + smoothing_factor * (raw_db - prev_db);
                        prev_db = smoothed_val;
                        let _ = tx.send(RadarMessage::Direction(smoothed_val));
                    } else {
                        prev_db *= 0.9;
                        let _ = tx.send(RadarMessage::Direction(prev_db));
                    }
                }
            }
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}
