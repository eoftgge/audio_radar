pub struct HighPassFilter {
    alpha: f32,
    prev_x: f32,
    prev_y: f32,
}

impl HighPassFilter {
    pub fn new(cutoff_freq: f32, sample_rate: f32) -> Self {
        let rc = 1.0 / (cutoff_freq * 2.0 * std::f32::consts::PI);
        let dt = 1.0 / sample_rate;
        let alpha = rc / (rc + dt);
        Self { alpha, prev_x: 0.0, prev_y: 0.0 }
    }

    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.alpha * (self.prev_y + x - self.prev_x);
        self.prev_x = x;
        self.prev_y = y;
        y
    }
}