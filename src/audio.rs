use crate::errors::AudioRadarErrors;
use crate::types::RadarMessage;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SizedSample, Stream};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;
use wgpu::PollType;
use wgpu::util::DeviceExt;

const CHUNK_SIZE: usize = 1024;
const MAX_SHIFT: i32 = 40;
const OUT_LEN: usize = (2 * MAX_SHIFT + 1) as usize;

pub fn start_capture_audio(tx: Sender<RadarMessage>) -> Result<(), AudioRadarErrors> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| AudioRadarErrors::from("Not found default audio output device"))?;

    let config = device.default_output_config()?;
    log::info!("Format Audio: {:?}", config);

    let channels = config.channels() as usize;
    let stream_config: cpal::StreamConfig = config.clone().into();

    let (gpu_tx, gpu_rx) = mpsc::channel::<(Vec<f32>, Vec<f32>)>();
    let radar_tx = tx.clone();

    thread::spawn(move || {
        pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
                .expect("Not found GPU");
            let (gpu_device, queue) = adapter
                .request_device(&Default::default())
                .await
                .expect("Failed to connect GPU");

            let shader = gpu_device.create_shader_module(wgpu::include_wgsl!("compute.wgsl"));
            let compute_pipeline = gpu_device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Audio Cross-Correlation Pipeline"),
                layout: None,
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });

            let out_buffer = gpu_device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Output Buffer"),
                size: (OUT_LEN * 4) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let staging_buffer = gpu_device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Staging Buffer"),
                size: (OUT_LEN * 4) as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let mut prev_x = 0.0;
            let mut prev_y = 1.0;
            let mut lp_left = 0.0;
            let mut lp_right = 0.0;
            let mut prev_brightness = 0.0;
            let mut prev_intensity = 0.0;

            while let Ok((left, right)) = gpu_rx.recv() {
                let rms_l = (left.iter().map(|s| s.powi(2)).sum::<f32>() / left.len() as f32).sqrt();
                let rms_r = (right.iter().map(|s| s.powi(2)).sum::<f32>() / right.len() as f32).sqrt();
                let total_intensity = rms_l + rms_r;

                if total_intensity < 0.001 {
                    prev_x *= 0.92;
                    prev_y += 0.08 * (1.0 - prev_y);
                    prev_intensity = 0.0;
                    let _ = radar_tx.send(RadarMessage::Surround { x: prev_x, y: prev_y, intensity: 0.0 });
                    continue;
                }

                let left_buffer = gpu_device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Left Buffer"),
                    contents: bytemuck::cast_slice(&left),
                    usage: wgpu::BufferUsages::STORAGE,
                });

                let right_buffer = gpu_device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Right Buffer"),
                    contents: bytemuck::cast_slice(&right),
                    usage: wgpu::BufferUsages::STORAGE,
                });

                let bind_group = gpu_device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Compute Bind Group"),
                    layout: &compute_pipeline.get_bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: left_buffer.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 1, resource: right_buffer.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 2, resource: out_buffer.as_entire_binding() },
                    ],
                });

                let mut encoder = gpu_device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
                {
                    let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
                    cpass.set_pipeline(&compute_pipeline);
                    cpass.set_bind_group(0, &bind_group, &[]);
                    cpass.dispatch_workgroups(1, 1, 1);
                }

                encoder.copy_buffer_to_buffer(&out_buffer, 0, &staging_buffer, 0, (OUT_LEN * 4) as u64);
                queue.submit(Some(encoder.finish()));

                let buffer_slice = staging_buffer.slice(..);
                let (sender, receiver) = futures_intrusive::channel::shared::oneshot_channel();
                buffer_slice.map_async(wgpu::MapMode::Read, move |v| sender.send(v).unwrap());

                gpu_device.poll(PollType::Wait {
                    submission_index: None,
                    timeout: None,
                }).expect("GPU Polling failed");

                if receiver.receive().await.is_some() {
                    let data = buffer_slice.get_mapped_range();
                    let result: &[f32] = bytemuck::cast_slice(&data);

                    let mut max_val = -1.0;
                    let mut max_idx = 0;
                    for (i, &val) in result.iter().enumerate() {
                        if val > max_val {
                            max_val = val;
                            max_idx = i;
                        }
                    }
                    drop(data);
                    staging_buffer.unmap();

                    let shift = max_idx as i32 - MAX_SHIFT;
                    let tdoa_x = -((shift as f32) / (MAX_SHIFT as f32));

                    let volume_x = if total_intensity > 0.0 {
                        (rms_r - rms_l) / total_intensity
                    } else {
                        0.0
                    };

                    let raw_x = ((tdoa_x * 0.8) + (volume_x * 1.5)).clamp(-1.0, 1.0);
                    let abs_x = raw_x.abs();

                    let mut diff_sum = 0.0;
                    let dom_rms;

                    if rms_l > rms_r {
                        for i in 0..left.len() {
                            let high_freq = (left[i] - lp_left) * 1.5;
                            lp_left = left[i];
                            diff_sum += high_freq.powi(2);
                        }
                        dom_rms = rms_l;
                    } else {
                        for i in 0..right.len() {
                            let high_freq = (right[i] - lp_right) * 1.5;
                            lp_right = right[i];
                            diff_sum += high_freq.powi(2);
                        }
                        dom_rms = rms_r;
                    }

                    let hf_intensity = (diff_sum / left.len() as f32).sqrt();
                    let current_brightness = if dom_rms > 0.0001 {
                        hf_intensity / dom_rms
                    } else {
                        0.0
                    };

                    if prev_brightness == 0.0 { prev_brightness = current_brightness; }
                    prev_brightness += 0.15 * (current_brightness - prev_brightness);
                    if current_brightness > prev_brightness {
                        prev_brightness += 0.85 * (current_brightness - prev_brightness);
                    } else {
                        prev_brightness += 0.15 * (current_brightness - prev_brightness);
                    }
                    let brightness = prev_brightness;
                    let y_center = (brightness - 0.85) * 4.0;
                    let y_side;

                    if brightness > 1.04 {
                        y_side = 0.0;
                    } else if brightness > 0.975 {
                        let t = (brightness - 0.975) / (1.04 - 0.975);
                        y_side = (t * std::f32::consts::PI).sin() * 2.0;
                    } else {
                        y_side = (brightness - 0.975) * 20.0;
                    }

                    let blend = ((abs_x - 0.3) / 0.5).clamp(0.0, 1.0);
                    let raw_y = (y_center * (1.0 - blend) + y_side * blend).clamp(-1.0, 1.0);

                    let length = (raw_x.powi(2) + raw_y.powi(2)).sqrt().max(1.0);
                    let norm_x = raw_x / length;
                    let norm_y = raw_y / length;
                    let current_smoothing = if total_intensity > prev_intensity * 1.5 {
                        0.9
                    } else {
                        0.3
                    };
                    prev_intensity = total_intensity; // Запоминаем для следующего кадра

                    prev_x += current_smoothing * (norm_x - prev_x);
                    prev_y += current_smoothing * (norm_y - prev_y);

                    let _ = radar_tx.send(RadarMessage::Surround {
                        x: prev_x,
                        y: prev_y,
                        intensity: total_intensity
                    });
                }
            }
        });
    });

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => build_stream::<f32>(&device, &stream_config, gpu_tx, channels)?,
        cpal::SampleFormat::I16 => build_stream::<i16>(&device, &stream_config, gpu_tx, channels)?,
        cpal::SampleFormat::U16 => build_stream::<u16>(&device, &stream_config, gpu_tx, channels)?,
        sample_format => return Err(AudioRadarErrors::Internal(format!("Unsupported format {:?}", sample_format))),
    };

    stream.play()?;
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    gpu_tx: Sender<(Vec<f32>, Vec<f32>)>,
    channels: usize,
) -> Result<Stream, AudioRadarErrors>
where
    T: Sample<Float = f32> + SizedSample,
{
    let mut left_buf = Vec::with_capacity(CHUNK_SIZE);
    let mut right_buf = Vec::with_capacity(CHUNK_SIZE);

    let err_fn = |err| log::error!("Error: {}", err);
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            for frame in data.chunks_exact(channels) {
                if channels >= 2 {
                    left_buf.push(frame[0].to_float_sample());
                    right_buf.push(frame[1].to_float_sample());
                }

                if left_buf.len() >= CHUNK_SIZE {
                    let _ = gpu_tx.send((left_buf.clone(), right_buf.clone()));
                    left_buf.clear();
                    right_buf.clear();
                }
            }
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}