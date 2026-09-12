use crate::dsp::localize::{HIST_BINS, MAX_PEAKS};

pub const MAX_SOURCES: usize = MAX_PEAKS;
pub const RESPONSE_BINS: usize = HIST_BINS;

#[derive(Clone, Copy, Debug, Default)]
pub struct Source {
    pub azimuth_deg: f32,
    pub level: f32,
    pub confidence: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct RadarFrame {
    pub sources: [Source; MAX_SOURCES],
    pub count: usize,
    pub loudness: f32,
    pub transient: bool,
    pub response: [f32; RESPONSE_BINS],
}

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
    pub fn response_azimuth(i: usize) -> f32 {
        -90.0 + i as f32 * RESPONSE_STEP_DEG
    }
}
