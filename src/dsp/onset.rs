pub struct OnsetDetector {
    prev_mag: Vec<f32>,
    history: Vec<f32>,
    hist_pos: usize,
    hist_filled: usize,
    factor: f32,
    floor: f32,
    refractory_frames: usize,
    cooldown: usize,
    warmup: usize,
    bin_lo: usize,
    bin_hi: usize,
}

impl OnsetDetector {
    pub fn new(
        num_bins: usize,
        bin_lo: usize,
        bin_hi: usize,
        history_len: usize,
        refractory_frames: usize,
    ) -> Self {
        Self {
            prev_mag: vec![0.0; num_bins],
            history: vec![0.0; history_len.max(4)],
            hist_pos: 0,
            hist_filled: 0,
            factor: 1.8,
            floor: 1e-4,
            refractory_frames,
            cooldown: 0,
            warmup: (history_len / 4).max(4),
            bin_lo,
            bin_hi: bin_hi.min(num_bins),
        }
    }

    pub fn push(&mut self, mag: &[f32]) -> bool {
        let mut flux = 0.0;
        for k in self.bin_lo..self.bin_hi {
            let d = mag[k] - self.prev_mag[k];
            if d > 0.0 {
                flux += d;
            }
        }
        self.prev_mag[..self.bin_hi].copy_from_slice(&mag[..self.bin_hi]);
        let threshold = self.median() * self.factor + self.floor;

        self.history[self.hist_pos] = flux;
        self.hist_pos = (self.hist_pos + 1) % self.history.len();
        self.hist_filled = (self.hist_filled + 1).min(self.history.len());

        if self.warmup > 0 {
            self.warmup -= 1;
            return false;
        }
        if self.cooldown > 0 {
            self.cooldown -= 1;
            return false;
        }
        if flux > threshold {
            self.cooldown = self.refractory_frames;
            return true;
        }
        false
    }

    fn median(&self) -> f32 {
        if self.hist_filled == 0 {
            return 0.0;
        }
        let mut buf: Vec<f32> = self.history[..self.hist_filled].to_vec();
        buf.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        buf[buf.len() / 2]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(level: f32, bins: usize) -> Vec<f32> {
        vec![level; bins]
    }

    #[test]
    fn fires_on_a_burst_and_stays_quiet_on_steady_level() {
        let bins = 128;
        let mut det = OnsetDetector::new(bins, 4, 100, 64, 4);

        let mut steady_hits = 0;
        for _ in 0..60 {
            if det.push(&flat(0.01, bins)) {
                steady_hits += 1;
            }
        }
        assert_eq!(steady_hits, 0, "ровный фон не является атакой");

        assert!(det.push(&flat(0.5, bins)), "скачок уровня не пойман");
    }

    #[test]
    fn refractory_suppresses_immediate_repeat() {
        let bins = 64;
        let mut det = OnsetDetector::new(bins, 2, 60, 32, 5);
        for _ in 0..40 {
            det.push(&flat(0.01, bins));
        }
        assert!(det.push(&flat(0.5, bins)));
        for step in 0..5 {
            assert!(
                !det.push(&flat(1.0, bins)),
                "срабатывание внутри паузы, шаг {}",
                step
            );
        }
    }

    #[test]
    fn decay_is_not_an_onset() {
        let bins = 64;
        let mut det = OnsetDetector::new(bins, 2, 60, 32, 0);
        for _ in 0..40 {
            det.push(&flat(0.5, bins));
        }
        assert!(!det.push(&flat(0.01, bins)), "затухание не является атакой");
    }
}
