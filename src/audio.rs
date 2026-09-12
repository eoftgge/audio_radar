//! Захват звука и запуск обработки.
//!
//! Аудио-колбэк не делает ни одной аллокации: он только снимает смещение и
//! копирует сэмплы в кольцевой буфер. Раньше на каждый чанк вызывался
//! `Vec::clone` прямо в realtime-потоке, а два буфера видеокарты создавались
//! заново 47 раз в секунду.

use crate::dsp::analyzer::Analyzer;
use crate::dsp::calib::Calibration;
use crate::dsp::ring::SpscRing;
use crate::errors::AudioRadarErrors;
use crate::filter::DcBlocker;
use crate::types::RadarFrame;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SizedSample, Stream};
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

/// Длина окна анализа. При 48 кГц это 21 мс — компромисс между разрешением по
/// частоте и задержкой.
pub const WINDOW: usize = 1024;

/// Шаг между окнами: при 48 кГц около 5.3 мс, то есть примерно 187 обновлений
/// в секунду.
pub const HOP: usize = 256;

/// Ёмкость кольцевого буфера в сэмплах на канал (около 0.68 с при 48 кГц).
const RING_CAPACITY: usize = 32768;

/// Если накопилось больше, обработка отстала — старое отбрасывается, чтобы
/// задержка не росла бесконечно.
const BACKLOG_LIMIT: usize = RING_CAPACITY / 2;

/// Частота среза фильтра постоянной составляющей.
const DC_CUTOFF_HZ: f32 = 20.0;

/// Имя файла калибровки, который ищется в рабочем каталоге.
const CALIBRATION_FILE: &str = "calibration.txt";

/// Переменная окружения с путём к калибровке.
const CALIBRATION_ENV: &str = "AUDIO_RADAR_CALIBRATION";

/// Читает калибровку с диска, откатываясь на встроенный профиль CS2.
///
/// Встроенный профиль задан аналитически (сферическая модель головы) и уже
/// пригоден для игры. Измеренная под конкретную HRTF таблица точнее, поэтому
/// её можно положить рядом файлом — формат описан в [`Calibration::parse`].
fn load_calibration() -> Calibration {
    let path = std::env::var(CALIBRATION_ENV).unwrap_or_else(|_| CALIBRATION_FILE.to_string());
    match std::fs::read_to_string(&path) {
        Ok(text) => match Calibration::parse(&text) {
            Ok(cal) => {
                log::info!("калибровка из {}: профиль {}", path, cal.name);
                return cal;
            }
            Err(err) => log::warn!(
                "калибровка {} не разобрана ({}), берётся встроенная",
                path,
                err
            ),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            log::info!("{} не найден, берётся встроенный профиль CS2", path);
        }
        Err(err) => log::warn!("не удалось прочитать {}: {}", path, err),
    }
    Calibration::cs2()
}

pub fn start_capture_audio(tx: Sender<RadarFrame>) -> Result<(), AudioRadarErrors> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| AudioRadarErrors::from("Not found default audio output device"))?;

    let config = device.default_output_config()?;
    log::info!("Format Audio: {:?}", config);

    let channels = config.channels() as usize;
    if channels < 2 {
        return Err(AudioRadarErrors::Internal(
            "нужен стереопоток: у устройства меньше двух каналов".into(),
        ));
    }

    let stream_config: cpal::StreamConfig = config.clone().into();
    // cpal хранит частоту дискретизации новотипом `SampleRate`. Если в вашей
    // версии это уже просто `u32`, здесь убирается `.0`.
    let sample_rate = stream_config.sample_rate.0 as f32;

    let left = Arc::new(SpscRing::new(RING_CAPACITY));
    let right = Arc::new(SpscRing::new(RING_CAPACITY));

    let dsp_left = Arc::clone(&left);
    let dsp_right = Arc::clone(&right);
    thread::spawn(move || {
        run_analysis(dsp_left, dsp_right, sample_rate, tx);
    });

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            build_stream::<f32>(&device, &stream_config, left, right, channels, sample_rate)?
        }
        cpal::SampleFormat::I16 => {
            build_stream::<i16>(&device, &stream_config, left, right, channels, sample_rate)?
        }
        cpal::SampleFormat::U16 => {
            build_stream::<u16>(&device, &stream_config, left, right, channels, sample_rate)?
        }
        sample_format => {
            return Err(AudioRadarErrors::Internal(format!(
                "Unsupported format {:?}",
                sample_format
            )));
        }
    };

    stream.play()?;
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

fn run_analysis(
    left: Arc<SpscRing>,
    right: Arc<SpscRing>,
    sample_rate: f32,
    tx: Sender<RadarFrame>,
) {
    let mut analyzer = Analyzer::new(WINDOW, HOP, sample_rate, load_calibration());
    let mut frame_l = vec![0.0f32; WINDOW];
    let mut frame_r = vec![0.0f32; WINDOW];

    log::info!(
        "анализ: окно {} ({:.1} мс), шаг {} ({:.1} мс), профиль {}",
        WINDOW,
        WINDOW as f32 * 1000.0 / sample_rate,
        HOP,
        HOP as f32 * 1000.0 / sample_rate,
        analyzer.localizer().calibration().name,
    );

    loop {
        let mut did_work = false;

        // Оба кольца одной ёмкости и всегда пишутся и читаются одинаково,
        // поэтому разойтись не должны. Но если это всё же случится, `peek` на
        // отставшем кольце будет вечно возвращать `false`, и обработка встанет
        // молча — дешевле проверить и пересинхронизироваться.
        if left.available() != right.available() {
            log::warn!("кольца разошлись, сброс");
            left.clear();
            right.clear();
        }

        // Если накопился завал, старое просто выбрасывается: показывать
        // направление позавчерашнего шага бессмысленно, а задержка иначе
        // растёт неограниченно.
        let backlog = left.available();
        if backlog > BACKLOG_LIMIT {
            let drop_n = backlog - WINDOW;
            left.consume(drop_n);
            right.consume(drop_n);
            log::debug!("обработка отстала, отброшено {} сэмплов", drop_n);
        }

        while left.peek(&mut frame_l) && right.peek(&mut frame_r) {
            let frame = analyzer.process(&frame_l, &frame_r);
            if tx.send(frame).is_err() {
                log::info!("окно закрыто, анализ остановлен");
                return;
            }
            left.consume(HOP);
            right.consume(HOP);
            did_work = true;
        }

        if !did_work {
            // Пауза заметно короче шага окна, поэтому на задержку почти не
            // влияет, но и не крутит процессор впустую.
            thread::sleep(Duration::from_micros(500));
        }
    }
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    left: Arc<SpscRing>,
    right: Arc<SpscRing>,
    channels: usize,
    sample_rate: f32,
) -> Result<Stream, AudioRadarErrors>
where
    T: Sample<Float = f32> + SizedSample,
{
    let mut dc_left = DcBlocker::new(DC_CUTOFF_HZ, sample_rate);
    let mut dc_right = DcBlocker::new(DC_CUTOFF_HZ, sample_rate);

    // Небольшие буферы фиксированной ёмкости: колбэк пишет в них и сразу
    // отдаёт в кольцо, ничего не выделяя.
    const BATCH: usize = 512;
    let mut batch_l = [0.0f32; BATCH];
    let mut batch_r = [0.0f32; BATCH];
    let mut filled = 0usize;
    let mut dropped_total = 0usize;

    let err_fn = |err| log::error!("Error: {}", err);
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            for frame in data.chunks_exact(channels) {
                batch_l[filled] = dc_left.process(frame[0].to_float_sample());
                batch_r[filled] = dc_right.process(frame[1].to_float_sample());
                filled += 1;

                if filled == BATCH {
                    let wrote = left.push(&batch_l[..filled]);
                    right.push(&batch_r[..filled]);
                    if wrote < filled {
                        dropped_total += filled - wrote;
                        log::debug!("кольцо переполнено, всего потеряно {}", dropped_total);
                    }
                    filled = 0;
                }
            }

            if filled > 0 {
                left.push(&batch_l[..filled]);
                right.push(&batch_r[..filled]);
                filled = 0;
            }
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}
