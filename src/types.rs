use crate::dsp::localize::{HIST_BINS, MAX_PEAKS};

/// Максимум одновременно показываемых источников.
pub const MAX_SOURCES: usize = MAX_PEAKS;

/// Число узлов отклика по азимуту, которые уходят в отрисовку.
pub const RESPONSE_BINS: usize = HIST_BINS;

#[derive(Clone, Copy, Debug, Default)]
pub struct Source {
    /// Азимут в передней полусфере, градусы: 0 — спереди, +90 — справа.
    ///
    /// Зеркальное направление сзади даёт ровно те же бинауральные признаки, и
    /// из стерео их не различить. Отрисовка обязана показывать обе
    /// возможности, а не выбирать одну.
    pub azimuth_deg: f32,
    /// Громкость источника, 0..1.
    pub level: f32,
    /// Надёжность направления, 0..1.
    pub confidence: f32,
}

/// Состояние радара на один кадр анализа.
#[derive(Clone, Copy, Debug)]
pub struct RadarFrame {
    pub sources: [Source; MAX_SOURCES],
    pub count: usize,
    /// Общая громкость кадра, 0..1.
    pub loudness: f32,
    /// Кадр пришёлся на атаку — шаг, выстрел, удар.
    pub transient: bool,
    /// Отклик по всей передней полусфере, 0..1. Первый узел — -90 градусов,
    /// шаг — [`RESPONSE_STEP_DEG`].
    pub response: [f32; RESPONSE_BINS],
}

/// Шаг сетки отклика, градусы.
pub const RESPONSE_STEP_DEG: f32 = crate::dsp::localize::HIST_STEP_DEG;

impl Default for RadarFrame {
    fn default() -> Self {
        Self {
            sources: [Source::default(); MAX_SOURCES],
            count: 0,
            loudness: 0.0,
            transient: false,
            response: [0.0; RESPONSE_BINS],
        }
    }
}

impl RadarFrame {
    /// Азимут узла отклика по его индексу.
    pub fn response_azimuth(i: usize) -> f32 {
        -90.0 + i as f32 * RESPONSE_STEP_DEG
    }
}
