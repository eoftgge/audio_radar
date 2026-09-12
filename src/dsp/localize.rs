use crate::dsp::calib::{Calibration, sample_lut};
use crate::dsp::fft::{Complex, Fft};

pub const MAX_PEAKS: usize = 4;
pub const HIST_STEP_DEG: f32 = 2.5;
pub const HIST_BINS: usize = 73;
const HIST_SIGMA_BINS: f32 = 1.6;
const MIN_PEAK_SEPARATION_DEG: f32 = 20.0;
const PEAK_REL_THRESHOLD: f32 = 0.28;
const BIN_FLOOR_REL: f32 = 0.02;
const ILD_TRUST: f32 = 0.7;
const ILD_SIGMA_DB: f32 = 3.0;
const MIN_COHERENCE: f32 = 0.22;
const ASSIGN_THRESHOLD: f32 = 0.5;
const MIN_LIVE_BINS: usize = 6;

#[derive(Clone, Copy, Debug, Default)]
pub struct Peak {
    pub azimuth_deg: f32,
    pub weight: f32,
    pub confidence: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PeakSet {
    pub peaks: [Peak; MAX_PEAKS],
    pub count: usize,
}

pub struct Localizer {
    fft: Fft,
    n: usize,
    sample_rate: f32,
    cal: Calibration,
    max_lag: usize,

    itd_bins: (usize, usize),
    ild_bins: (usize, usize),

    ild_curves: Vec<f32>,

    steer_cos: Vec<f32>,
    steer_sin: Vec<f32>,

    cross: Vec<Complex>,
    psi: Vec<Complex>,
    bin_w: Vec<f32>,
    bin_live: Vec<bool>,

    srp: Vec<f32>,
    hist_itd: Vec<f32>,
    hist_ild: Vec<f32>,
    hist: Vec<f32>,
    hist_smooth: Vec<f32>,
    kernel: Vec<f32>,
}

impl Localizer {
    pub fn new(n: usize, sample_rate: f32, cal: Calibration) -> Self {
        let fft = Fft::new(n);
        let bin_of = |hz: f32| ((hz * n as f32 / sample_rate).round() as usize).clamp(1, n / 2);

        let itd_bins = (bin_of(cal.itd_band.0), bin_of(cal.itd_band.1));
        let ild_lo = cal.ild_bands.first().map(|b| b.f_lo).unwrap_or(2000.0);
        let ild_hi = cal.ild_bands.last().map(|b| b.f_hi).unwrap_or(9000.0);
        let ild_bins = (bin_of(ild_lo), bin_of(ild_hi));

        let max_lag = ((cal.max_itd() * sample_rate * 1.25).ceil() as usize)
            .max(2)
            .min(n / 4);

        let n_itd = itd_bins.1.saturating_sub(itd_bins.0);
        let mut steer_cos = vec![0.0f32; HIST_BINS * n_itd];
        let mut steer_sin = vec![0.0f32; HIST_BINS * n_itd];
        for i in 0..HIST_BINS {
            let tau = cal.azimuth_to_itd(Self::response_azimuth(i)) * sample_rate;
            for (j, k) in (itd_bins.0..itd_bins.1).enumerate() {
                let phi = 2.0 * std::f32::consts::PI * k as f32 * tau / n as f32;
                steer_cos[i * n_itd + j] = phi.cos();
                steer_sin[i * n_itd + j] = phi.sin();
            }
        }

        let mut ild_curves = Vec::with_capacity(cal.ild_bands.len() * HIST_BINS);
        for band in &cal.ild_bands {
            for i in 0..HIST_BINS {
                ild_curves.push(sample_lut(&band.lut, Self::response_azimuth(i)));
            }
        }

        let radius = (HIST_SIGMA_BINS * 3.0).ceil() as usize;
        let kernel: Vec<f32> = (0..=2 * radius)
            .map(|i| {
                let d = i as f32 - radius as f32;
                (-0.5 * (d / HIST_SIGMA_BINS).powi(2)).exp()
            })
            .collect();
        let ksum: f32 = kernel.iter().sum();
        let kernel = kernel.into_iter().map(|v| v / ksum).collect();

        Self {
            fft,
            n,
            sample_rate,
            cal,
            max_lag,
            itd_bins,
            ild_bins,
            ild_curves,
            steer_cos,
            steer_sin,
            cross: vec![Complex::ZERO; n],
            psi: vec![Complex::ZERO; n_itd],
            bin_w: vec![0.0; n_itd],
            bin_live: vec![false; n_itd],
            srp: vec![0.0; HIST_BINS],
            hist_itd: vec![0.0; HIST_BINS],
            hist_ild: vec![0.0; HIST_BINS],
            hist: vec![0.0; HIST_BINS],
            hist_smooth: vec![0.0; HIST_BINS],
            kernel,
        }
    }

    pub fn calibration(&self) -> &Calibration {
        &self.cal
    }

    pub fn response(&self) -> &[f32] {
        &self.hist_smooth
    }

    pub fn response_azimuth(i: usize) -> f32 {
        -90.0 + i as f32 * HIST_STEP_DEG
    }

    pub fn gcc_phat(&mut self, lspec: &[Complex], rspec: &[Complex]) -> Option<(f32, f32)> {
        let n = self.n;
        let half = n / 2;
        for c in self.cross.iter_mut() {
            *c = Complex::ZERO;
        }

        let (lo, hi) = self.itd_bins;
        let mut active = 0usize;
        for k in lo..hi.min(half) {
            let x = lspec[k].mul(rspec[k].conj());
            let mag = x.norm();
            if mag > 1e-12 {
                self.cross[k] = x.scale(1.0 / mag);
                active += 1;
            }
        }
        if active == 0 {
            return None;
        }

        for k in 1..half {
            self.cross[n - k] = self.cross[k].conj();
        }
        self.cross[0] = Complex::new(self.cross[0].re, 0.0);
        self.cross[half] = Complex::new(self.cross[half].re, 0.0);

        self.fft.inverse(&mut self.cross);

        let at = |tau: i32| -> f32 {
            let idx = if tau >= 0 {
                tau as usize
            } else {
                (n as i32 + tau) as usize
            };
            self.cross[idx].re
        };

        let max_lag = self.max_lag as i32;
        let mut best_tau = 0i32;
        let mut best_val = f32::NEG_INFINITY;
        for tau in -max_lag..=max_lag {
            let v = at(tau);
            if v > best_val {
                best_val = v;
                best_tau = tau;
            }
        }

        let refined = if best_tau > -max_lag && best_tau < max_lag {
            let y0 = at(best_tau - 1);
            let y1 = best_val;
            let y2 = at(best_tau + 1);
            let denom = y0 - 2.0 * y1 + y2;
            if denom.abs() > 1e-12 {
                let delta = (0.5 * (y0 - y2) / denom).clamp(-0.5, 0.5);
                best_tau as f32 + delta
            } else {
                best_tau as f32
            }
        } else {
            best_tau as f32
        };

        let ideal = active as f32 / half as f32;
        let quality = if ideal > 0.0 {
            (best_val / ideal).clamp(0.0, 1.0)
        } else {
            0.0
        };

        Some((refined, quality))
    }

    pub fn analyze(&mut self, lspec: &[Complex], rspec: &[Complex]) -> PeakSet {
        let half = self.n / 2;

        let mut frame_peak = 0.0f32;
        for k in 1..half {
            frame_peak = frame_peak.max(lspec[k].norm().max(rspec[k].norm()));
        }
        if frame_peak <= 0.0 {
            self.hist_smooth.fill(0.0);
            return PeakSet::default();
        }
        let bin_floor = frame_peak * BIN_FLOOR_REL;

        let (_, n_itd) = self.whiten(lspec, rspec, bin_floor, half);
        let (energy_ild, n_ild) = self.ild_histogram(lspec, rspec, bin_floor, half);

        let energy_itd = self.deflate();

        let trust_itd = if n_itd > 0 {
            energy_itd / n_itd as f32
        } else {
            0.0
        };
        let trust_ild = if n_ild > 0 {
            ILD_TRUST * energy_ild / n_ild as f32
        } else {
            0.0
        };

        let mass_itd: f32 = self.hist_itd.iter().sum();
        let mass_ild: f32 = self.hist_ild.iter().sum();
        for i in 0..HIST_BINS {
            let a = if mass_itd > 0.0 {
                self.hist_itd[i] / mass_itd * trust_itd
            } else {
                0.0
            };
            let b = if mass_ild > 0.0 {
                self.hist_ild[i] / mass_ild * trust_ild
            } else {
                0.0
            };
            self.hist[i] = a + b;
        }
        self.smooth();
        self.pick_peaks()
    }

    fn whiten(
        &mut self,
        lspec: &[Complex],
        rspec: &[Complex],
        bin_floor: f32,
        half: usize,
    ) -> (f32, usize) {
        let (lo, hi) = self.itd_bins;
        let mut energy = 0.0f32;
        for (j, k) in (lo..hi.min(half)).enumerate() {
            let (ml, mr) = (lspec[k].norm(), rspec[k].norm());
            if ml < bin_floor || mr < bin_floor {
                self.psi[j] = Complex::ZERO;
                self.bin_w[j] = 0.0;
                self.bin_live[j] = false;
                continue;
            }
            let x = lspec[k].mul(rspec[k].conj());
            let mag = x.norm();
            if mag <= 1e-12 {
                self.psi[j] = Complex::ZERO;
                self.bin_w[j] = 0.0;
                self.bin_live[j] = false;
                continue;
            }
            self.psi[j] = x.scale(1.0 / mag);
            self.bin_w[j] = ml * mr;
            self.bin_live[j] = true;
            energy += ml * mr;
        }
        (energy, hi.saturating_sub(lo))
    }

    fn deflate(&mut self) -> f32 {
        for h in self.hist_itd.iter_mut() {
            *h = 0.0;
        }
        let n_itd = self.psi.len();
        let mut found: Vec<f32> = Vec::new();
        let mut total = 0.0f32;

        while found.len() < MAX_PEAKS {
            let live: usize = self.bin_live.iter().filter(|&&v| v).count();
            if live < MIN_LIVE_BINS {
                break;
            }

            let inv = 1.0 / live as f32;
            let mut best_i = 0usize;
            let mut best_v = f32::NEG_INFINITY;
            for i in 0..HIST_BINS {
                let base = i * n_itd;
                let mut acc = 0.0f32;
                for j in 0..n_itd {
                    if !self.bin_live[j] {
                        continue;
                    }
                    let p = self.psi[j];
                    acc += p.re * self.steer_cos[base + j] - p.im * self.steer_sin[base + j];
                }
                let v = acc * inv;
                self.srp[i] = v.max(0.0);
                if v > best_v {
                    best_v = v;
                    best_i = i;
                }
            }

            if best_v < MIN_COHERENCE {
                break;
            }

            let base = best_i * n_itd;
            let mut assigned = 0usize;
            let mut energy = 0.0f32;
            for j in 0..n_itd {
                if !self.bin_live[j] {
                    continue;
                }
                let p = self.psi[j];
                let c = p.re * self.steer_cos[base + j] - p.im * self.steer_sin[base + j];
                if c > ASSIGN_THRESHOLD {
                    self.bin_live[j] = false;
                    energy += self.bin_w[j];
                    assigned += 1;
                }
            }
            if assigned == 0 {
                break;
            }

            let az = refine_peak(&self.srp, best_i);
            if found
                .iter()
                .any(|&prev| (prev - az).abs() < MIN_PEAK_SEPARATION_DEG)
            {
                continue;
            }
            found.push(az);
            total += energy;
            vote(&mut self.hist_itd, az, energy * best_v.clamp(0.0, 1.0));
        }

        total
    }

    fn ild_histogram(
        &mut self,
        lspec: &[Complex],
        rspec: &[Complex],
        bin_floor: f32,
        half: usize,
    ) -> (f32, usize) {
        for h in self.hist_ild.iter_mut() {
            *h = 0.0;
        }

        let (lo, hi) = self.ild_bins;
        let n_bins = hi.saturating_sub(lo);
        let floor_sq = bin_floor * bin_floor;
        let mut total = 0.0f32;

        for (bi, band) in self.cal.ild_bands.iter().enumerate() {
            let k_of = |hz: f32| ((hz * self.n as f32 / self.sample_rate).round() as usize).max(1);
            let k_lo = k_of(band.f_lo);
            let k_hi = k_of(band.f_hi).min(half);
            if k_hi <= k_lo {
                continue;
            }

            let mut el = 0.0f32;
            let mut er = 0.0f32;
            for k in k_lo..k_hi {
                let (ml, mr) = (lspec[k].norm(), rspec[k].norm());
                if ml.max(mr) < bin_floor {
                    continue;
                }
                el += ml * ml;
                er += mr * mr;
            }

            let energy = 0.5 * (el + er);
            if energy <= floor_sq {
                continue;
            }

            let eps = floor_sq * 1e-2 + 1e-18;
            let ild_db = 10.0 * ((er + eps) / (el + eps)).log10();
            total += energy;

            let curve = &self.ild_curves[bi * HIST_BINS..(bi + 1) * HIST_BINS];
            for i in 0..HIST_BINS {
                let d = (ild_db - curve[i]) / ILD_SIGMA_DB;
                self.hist_ild[i] += energy * (-0.5 * d * d).exp();
            }
        }

        (total, n_bins)
    }

    fn smooth(&mut self) {
        let radius = (self.kernel.len() / 2) as isize;
        for i in 0..HIST_BINS {
            let mut acc = 0.0;
            let mut wsum = 0.0;
            for (ki, &kv) in self.kernel.iter().enumerate() {
                let src = i as isize + ki as isize - radius;
                if src < 0 || src >= HIST_BINS as isize {
                    continue;
                }
                acc += self.hist[src as usize] * kv;
                wsum += kv;
            }
            self.hist_smooth[i] = if wsum > 0.0 { acc / wsum } else { 0.0 };
        }
    }

    fn pick_peaks(&self) -> PeakSet {
        let h = &self.hist_smooth;
        let total: f32 = h.iter().sum();
        let global_max = h.iter().fold(0.0f32, |a, &b| a.max(b));
        if total <= 0.0 || global_max <= 0.0 {
            return PeakSet::default();
        }

        let mut candidates: Vec<(f32, f32)> = Vec::new();
        for i in 0..HIST_BINS {
            let c = h[i];
            if c < global_max * PEAK_REL_THRESHOLD {
                continue;
            }
            let l = if i > 0 { h[i - 1] } else { 0.0 };
            let r = if i + 1 < HIST_BINS { h[i + 1] } else { 0.0 };
            if c < l || c < r {
                continue;
            }
            candidates.push((refine_peak(h, i), c));
        }
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut out = PeakSet::default();
        for (az, height) in candidates {
            if out.count >= MAX_PEAKS {
                break;
            }
            if out.peaks[..out.count]
                .iter()
                .any(|p| (p.azimuth_deg - az).abs() < MIN_PEAK_SEPARATION_DEG)
            {
                continue;
            }

            let win = (10.0 / HIST_STEP_DEG).round() as isize;
            let center = ((az + 90.0) / HIST_STEP_DEG).round() as isize;
            let mut local = 0.0;
            for d in -win..=win {
                let idx = center + d;
                if idx >= 0 && (idx as usize) < HIST_BINS {
                    local += h[idx as usize];
                }
            }

            out.peaks[out.count] = Peak {
                azimuth_deg: az,
                weight: (height / global_max).clamp(0.0, 1.0),
                confidence: (local / total).clamp(0.0, 1.0),
            };
            out.count += 1;
        }
        out
    }
}

fn refine_peak(h: &[f32], i: usize) -> f32 {
    let l = if i > 0 { h[i - 1] } else { 0.0 };
    let c = h[i];
    let r = if i + 1 < h.len() { h[i + 1] } else { 0.0 };
    let denom = l - 2.0 * c + r;
    let delta = if denom.abs() > 1e-12 {
        (0.5 * (l - r) / denom).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    (-90.0 + (i as f32 + delta) * HIST_STEP_DEG).clamp(-90.0, 90.0)
}

fn vote(hist: &mut [f32], az_deg: f32, weight: f32) {
    if weight <= 0.0 || !az_deg.is_finite() {
        return;
    }
    let pos = (az_deg + 90.0) / HIST_STEP_DEG;
    let base = pos.floor();
    let frac = pos - base;
    let i = base as isize;
    if i >= 0 && (i as usize) < HIST_BINS {
        hist[i as usize] += weight * (1.0 - frac);
    }
    let j = i + 1;
    if j >= 0 && (j as usize) < HIST_BINS {
        hist[j as usize] += weight * frac;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::fft::unpack_two_real;

    const N: usize = 1024;
    const FS: f32 = 48000.0;
    fn noise(state: &mut u32) -> f32 {
        *state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        ((*state >> 8) as f32 / 8388608.0) - 1.0
    }

    fn spectra(l: &[f32], r: &[f32]) -> (Vec<Complex>, Vec<Complex>) {
        let fft = Fft::new(N);
        let mut packed: Vec<Complex> = (0..N)
            .map(|i| {
                let w = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / N as f32).cos();
                Complex::new(l[i] * w, r[i] * w)
            })
            .collect();
        fft.forward(&mut packed);
        let mut ls = vec![Complex::ZERO; N / 2 + 1];
        let mut rs = vec![Complex::ZERO; N / 2 + 1];
        unpack_two_real(&packed, &mut ls, &mut rs);
        (ls, rs)
    }

    fn delayed_pair(src: &[f32], d: i32) -> (Vec<f32>, Vec<f32>) {
        let pick = |i: i32| -> f32 {
            if i >= 0 && (i as usize) < src.len() {
                src[i as usize]
            } else {
                0.0
            }
        };
        (
            (0..N).map(|i| pick(i as i32 - d)).collect(),
            (0..N).map(|i| pick(i as i32)).collect(),
        )
    }

    fn tonal(seed: u32, len: usize, bins: &[usize]) -> Vec<f32> {
        let mut s = seed;
        let phases: Vec<f32> = bins
            .iter()
            .map(|_| noise(&mut s) * std::f32::consts::PI)
            .collect();
        (0..len)
            .map(|i| {
                bins.iter()
                    .zip(&phases)
                    .map(|(&k, &ph)| {
                        (2.0 * std::f32::consts::PI * k as f32 * i as f32 / N as f32 + ph).sin()
                    })
                    .sum::<f32>()
                    / bins.len() as f32
            })
            .collect()
    }

    fn broadband(seed: u32, len: usize) -> Vec<f32> {
        let bins: Vec<usize> = (5..64).collect();
        tonal(seed, len, &bins)
    }

    #[test]
    fn gcc_phat_sign_and_accuracy() {
        let mut loc = Localizer::new(N, FS, Calibration::cs2());
        let src = broadband(777, N + 128);
        for d in [-30i32, -20, -10, -3, 0, 3, 10, 20, 30] {
            let (l, r) = delayed_pair(&src, d);
            let (ls, rs) = spectra(&l, &r);
            let (tau, quality) = loc.gcc_phat(&ls, &rs).expect("пустой кросс-спектр");
            assert!((tau - d as f32).abs() < 0.6, "задержка {d}: получено {tau}");
            assert!(quality > 0.8, "низкое качество пика: {quality}");
        }
    }

    #[test]
    fn azimuth_sign_follows_delay() {
        let mut loc = Localizer::new(N, FS, Calibration::cs2());
        let src = broadband(4242, N + 128);

        for (d, expect) in [(25i32, 45.0f32), (-25, -45.0), (0, 0.0)] {
            let (l, r) = delayed_pair(&src, d);
            let (ls, rs) = spectra(&l, &r);
            let set = loc.analyze(&ls, &rs);
            assert!(set.count > 0, "источник не найден при задержке {d}");
            let az = set.peaks[0].azimuth_deg;
            match d {
                x if x > 0 => assert!(az > expect, "правый источник вышел на {az}"),
                x if x < 0 => assert!(az < expect, "левый источник вышел на {az}"),
                _ => assert!(az.abs() < 10.0, "центральный источник вышел на {az}"),
            }
        }
    }

    #[test]
    fn ild_alone_gives_direction() {
        let mut loc = Localizer::new(N, FS, Calibration::cs2());
        let bins: Vec<usize> = (100..240).collect();
        let src = tonal(999, N + 128, &bins);
        let (l0, r0) = delayed_pair(&src, 0);
        let l: Vec<f32> = l0.iter().map(|v| v * 0.5).collect();

        let (ls, rs) = spectra(&l, &r0);
        let set = loc.analyze(&ls, &rs);
        assert!(set.count > 0, "источник не найден");
        let az = set.peaks[0].azimuth_deg;
        assert!(
            (8.0..45.0).contains(&az),
            "разница в 6 дБ должна давать умеренный угол, вышло {az}"
        );
    }

    #[test]
    fn consistent_cues_yield_single_source() {
        let mut loc = Localizer::new(N, FS, Calibration::cs2());
        let bins: Vec<usize> = (5..250).collect();
        let src = tonal(555, N + 128, &bins);
        let (l0, r0) = delayed_pair(&src, 25);
        let l: Vec<f32> = l0.iter().map(|v| v * 0.209).collect();

        let (ls, rs) = spectra(&l, &r0);
        let set = loc.analyze(&ls, &rs);
        assert_eq!(
            set.count, 1,
            "согласованные подсказки должны дать одну метку"
        );
        assert!(set.peaks[0].azimuth_deg > 40.0);
    }

    #[test]
    fn separates_two_spectrally_distinct_sources() {
        let mut loc = Localizer::new(N, FS, Calibration::cs2());
        let low: Vec<usize> = (5..18).collect();
        let high: Vec<usize> = (26..64).collect();
        let a = tonal(111, N + 128, &low);
        let b = tonal(222, N + 128, &high);

        let (al, ar) = delayed_pair(&a, 28);
        let (bl, br) = delayed_pair(&b, -28);
        let l: Vec<f32> = (0..N).map(|i| al[i] + bl[i]).collect();
        let r: Vec<f32> = (0..N).map(|i| ar[i] + br[i]).collect();

        let (ls, rs) = spectra(&l, &r);
        let set = loc.analyze(&ls, &rs);
        assert!(set.count >= 2, "источники не разделились: {}", set.count);
        let left = (0..set.count).any(|i| set.peaks[i].azimuth_deg < -40.0);
        let right = (0..set.count).any(|i| set.peaks[i].azimuth_deg > 40.0);
        assert!(left && right, "найдены не оба направления");
    }

    #[test]
    fn silence_yields_nothing() {
        let mut loc = Localizer::new(N, FS, Calibration::cs2());
        let zeros = vec![0.0f32; N];
        let (ls, rs) = spectra(&zeros, &zeros);
        assert_eq!(loc.analyze(&ls, &rs).count, 0);
        assert!(loc.response().iter().all(|&v| v == 0.0));
    }

    #[test]
    fn response_covers_the_front_hemisphere() {
        assert_eq!(Localizer::response_azimuth(0), -90.0);
        assert_eq!(Localizer::response_azimuth(HIST_BINS - 1), 90.0);
    }
}
