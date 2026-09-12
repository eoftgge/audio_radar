use crate::dsp::localize::MAX_PEAKS;

pub const MAX_SOURCES: usize = MAX_PEAKS;

#[derive(Clone, Copy, Debug, Default)]
pub struct Source {
    pub azimuth_deg: f32,
    pub level: f32,
    pub confidence: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RadarFrame {
    pub sources: [Source; MAX_SOURCES],
    pub count: usize,
    pub loudness: f32,
    pub transient: bool,
}
