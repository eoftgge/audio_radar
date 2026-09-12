/// Однополюсный фильтр, убирающий постоянную составляющую.
///
/// Прежний `HighPassFilter` со срезом 19 Гц по сути делал то же самое, но его
/// выход подмешивался как `raw * 0.3 + filtered * 1.5`, что сводилось к
/// усилению примерно в 1.8 раза и ни на что не влияло. Выбор полос теперь
/// целиком в частотной области, где он и должен быть, а здесь остаётся только
/// снятие смещения — иначе постоянная составляющая садится в нулевой бин и
/// искажает оценку общего уровня кадра.
pub struct DcBlocker {
    alpha: f32,
    prev_x: f32,
    prev_y: f32,
}

impl DcBlocker {
    pub fn new(cutoff_hz: f32, sample_rate: f32) -> Self {
        let rc = 1.0 / (cutoff_hz * 2.0 * std::f32::consts::PI);
        let dt = 1.0 / sample_rate;
        Self {
            alpha: rc / (rc + dt),
            prev_x: 0.0,
            prev_y: 0.0,
        }
    }

    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.alpha * (self.prev_y + x - self.prev_x);
        self.prev_x = x;
        self.prev_y = y;
        y
    }
}
