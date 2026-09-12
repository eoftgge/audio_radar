use crate::dsp::calib::Calibration;
use crate::dsp::fft::{Complex, Fft, unpack_two_real};
use crate::dsp::localize::{Localizer, PeakSet};
use crate::dsp::onset::OnsetDetector;
use crate::types::{MAX_SOURCES, RadarFrame, Source};

const SILENCE_RMS: f32 = 1e-4;
const LOUDNESS_RANGE_DB: f32 = 60.0;
const TRACK_MATCH_DEG: f32 = 15.0;
const AZIMUTH_TAU_S: f32 = 0.04;
const RELEASE_S: f32 = 0.25;
const TRACK_CUTOFF: f32 = 0.02;
const ONSET_BAND_HZ: (f32, f32) = (200.0, 6000.0);
const ONSET_HISTORY_S: f32 = 1.0;
const ONSET_REFRACTORY_S: f32 = 0.05;

#[derive(Clone, Copy, Default)]
struct Track {
    azimuth_deg: f32,
    level: f32,
    confidence: f32,
    alive: bool,
}

pub struct Analyzer {
    n: usize,
    window: Vec<f32>,
    fft: Fft,
    packed: Vec<Complex>,
    lspec: Vec<Complex>,
    rspec: Vec<Complex>,
    mag: Vec<f32>,
    onset: OnsetDetector,
    loc: Localizer,
    tracks: [Track; MAX_SOURCES],
    hop_seconds: f32,
}

impl Analyzer {
    pub fn new(n: usize, hop: usize, sample_rate: f32, cal: Calibration) -> Self {
        assert!(
            n.is_power_of_two(),
            "длина окна должна быть степенью двойки"
        );
        assert!(hop > 0 && hop <= n, "шаг окна вне допустимого диапазона");

        let window: Vec<f32> = (0..n)
            .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos())
            .collect();

        let hop_seconds = hop as f32 / sample_rate;
        let bin_of = |hz: f32| ((hz * n as f32 / sample_rate).round() as usize).clamp(1, n / 2);
        let onset = OnsetDetector::new(
            n / 2 + 1,
            bin_of(ONSET_BAND_HZ.0),
            bin_of(ONSET_BAND_HZ.1),
            (ONSET_HISTORY_S / hop_seconds).round() as usize,
            (ONSET_REFRACTORY_S / hop_seconds).round() as usize,
        );

        Self {
            n,
            window,
            fft: Fft::new(n),
            packed: vec![Complex::ZERO; n],
            lspec: vec![Complex::ZERO; n / 2 + 1],
            rspec: vec![Complex::ZERO; n / 2 + 1],
            mag: vec![0.0; n / 2 + 1],
            onset,
            loc: Localizer::new(n, sample_rate, cal),
            tracks: [Track::default(); MAX_SOURCES],
            hop_seconds,
        }
    }

    pub fn localizer(&self) -> &Localizer {
        &self.loc
    }

    pub fn process(&mut self, l: &[f32], r: &[f32]) -> RadarFrame {
        assert_eq!(l.len(), self.n);
        assert_eq!(r.len(), self.n);

        let mut sum_sq = 0.0f32;
        for i in 0..self.n {
            sum_sq += l[i] * l[i] + r[i] * r[i];
            let w = self.window[i];
            self.packed[i] = Complex::new(l[i] * w, r[i] * w);
        }
        let rms = (sum_sq / (2.0 * self.n as f32)).sqrt();

        if rms < SILENCE_RMS {
            self.release_all();
            return self.build_frame(0.0, false);
        }

        self.fft.forward(&mut self.packed);
        unpack_two_real(&self.packed, &mut self.lspec, &mut self.rspec);

        for k in 0..=self.n / 2 {
            self.mag[k] = self.lspec[k].norm() + self.rspec[k].norm();
        }
        let transient = self.onset.push(&self.mag);

        let peaks = self.loc.analyze(&self.lspec, &self.rspec);

        let loudness =
            ((20.0 * rms.log10() + LOUDNESS_RANGE_DB) / LOUDNESS_RANGE_DB).clamp(0.0, 1.0);

        self.update_tracks(&peaks, loudness, transient);
        self.build_frame(loudness, transient)
    }

    fn release_all(&mut self) {
        let decay = (-self.hop_seconds / RELEASE_S).exp();
        for t in self.tracks.iter_mut() {
            t.level *= decay;
            if t.level < TRACK_CUTOFF {
                *t = Track::default();
            }
        }
    }

    fn update_tracks(&mut self, peaks: &PeakSet, loudness: f32, transient: bool) {
        let decay = (-self.hop_seconds / RELEASE_S).exp();
        let alpha = if transient {
            1.0
        } else {
            1.0 - (-self.hop_seconds / AZIMUTH_TAU_S).exp()
        };

        let mut matched = [false; MAX_SOURCES];
        let mut updated = [false; MAX_SOURCES];

        for pi in 0..peaks.count {
            let p = peaks.peaks[pi];
            let target = p.weight * loudness;

            let mut chosen = None;
            let mut best_dist = TRACK_MATCH_DEG;
            for ti in 0..MAX_SOURCES {
                if !self.tracks[ti].alive || matched[ti] {
                    continue;
                }
                let d = (self.tracks[ti].azimuth_deg - p.azimuth_deg).abs();
                if d < best_dist {
                    best_dist = d;
                    chosen = Some(ti);
                }
            }

            let ti = match chosen {
                Some(ti) => ti,
                None => {
                    let free = (0..MAX_SOURCES).find(|&i| !self.tracks[i].alive && !matched[i]);
                    match free.or_else(|| {
                        (0..MAX_SOURCES).filter(|&i| !matched[i]).min_by(|&a, &b| {
                            self.tracks[a]
                                .level
                                .partial_cmp(&self.tracks[b].level)
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                    }) {
                        Some(i) => i,
                        None => continue,
                    }
                }
            };

            let t = &mut self.tracks[ti];
            if t.alive {
                t.azimuth_deg += alpha * (p.azimuth_deg - t.azimuth_deg);
            } else {
                t.azimuth_deg = p.azimuth_deg;
            }
            t.level = t.level.max(target);
            t.confidence = p.confidence;
            t.alive = true;
            matched[ti] = true;
            updated[ti] = true;
        }

        for ti in 0..MAX_SOURCES {
            if updated[ti] || !self.tracks[ti].alive {
                continue;
            }
            self.tracks[ti].level *= decay;
            if self.tracks[ti].level < TRACK_CUTOFF {
                self.tracks[ti] = Track::default();
            }
        }
    }

    fn build_frame(&self, loudness: f32, transient: bool) -> RadarFrame {
        let mut frame = RadarFrame {
            loudness,
            transient,
            ..Default::default()
        };

        let mut order = [0usize; MAX_SOURCES];
        let mut n = 0;
        for i in 0..MAX_SOURCES {
            if self.tracks[i].alive {
                order[n] = i;
                n += 1;
            }
        }
        order[..n].sort_by(|&a, &b| {
            self.tracks[b]
                .level
                .partial_cmp(&self.tracks[a].level)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for &i in &order[..n] {
            let t = self.tracks[i];
            frame.sources[frame.count] = Source {
                azimuth_deg: t.azimuth_deg,
                level: t.level,
                confidence: t.confidence,
            };
            frame.count += 1;
        }
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: usize = 1024;
    const HOP: usize = 256;
    const FS: f32 = 48000.0;
    fn noise(state: &mut u32) -> f32 {
        *state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        ((*state >> 8) as f32 / 8388608.0) - 1.0
    }

    fn analyzer() -> Analyzer {
        Analyzer::new(N, HOP, FS, Calibration::cs2())
    }

    #[test]
    fn silence_reports_nothing() {
        let mut an = analyzer();
        let zeros = vec![0.0f32; N];
        let frame = an.process(&zeros, &zeros);
        assert_eq!(frame.count, 0);
        assert_eq!(frame.loudness, 0.0);
        assert!(!frame.transient);
    }

    #[test]
    fn tracks_a_source_on_the_right() {
        let mut an = analyzer();
        let mut seed = 4242;
        let src: Vec<f32> = (0..N * 40).map(|_| noise(&mut seed) * 0.2).collect();

        const DELAY: usize = 25;
        const ILD_GAIN: f32 = 0.209;
        let mut frame = RadarFrame::default();
        for step in 0..30 {
            let off = step * HOP;
            let l: Vec<f32> = (0..N)
                .map(|i| {
                    if off + i >= DELAY {
                        src[off + i - DELAY] * ILD_GAIN
                    } else {
                        0.0
                    }
                })
                .collect();
            let r: Vec<f32> = (0..N).map(|i| src[off + i]).collect();
            frame = an.process(&l, &r);
        }

        assert_eq!(frame.count, 1, "один источник — одна метка");
        let s = frame.sources[0];
        assert!(
            s.azimuth_deg > 40.0,
            "источник справа вышел на {}",
            s.azimuth_deg
        );
        assert!(s.level > 0.0 && s.level <= 1.0);
        assert!(frame.loudness > 0.0 && frame.loudness <= 1.0);
        assert!(s.confidence > 0.0, "уверенность не выставлена");
    }

    #[test]
    fn sources_fade_after_the_sound_stops() {
        let mut an = analyzer();
        let mut seed = 31337;
        let src: Vec<f32> = (0..N * 20).map(|_| noise(&mut seed) * 0.2).collect();

        for step in 0..20 {
            let off = step * HOP;
            let l: Vec<f32> = (0..N).map(|i| src[off + i] * 0.3).collect();
            let r: Vec<f32> = (0..N).map(|i| src[off + i]).collect();
            an.process(&l, &r);
        }

        let zeros = vec![0.0f32; N];
        let mut last = an.process(&zeros, &zeros);
        for _ in 0..200 {
            last = an.process(&zeros, &zeros);
        }
        assert_eq!(last.count, 0, "источники должны погаснуть в тишине");
    }

    #[test]
    fn detects_bursts() {
        let mut an = analyzer();
        let mut seed = 777;
        let mut hits = 0;
        for step in 0..120 {
            let amp = if step % 40 == 20 { 0.5 } else { 0.01 };
            let buf: Vec<f32> = (0..N).map(|_| noise(&mut seed) * amp).collect();
            if an.process(&buf, &buf).transient {
                hits += 1;
            }
        }
        assert!(hits >= 2, "всплески не пойманы, срабатываний {hits}");
    }
}
