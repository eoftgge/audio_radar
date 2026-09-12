//! Калибровочная таблица: связь между бинауральными признаками и азимутом.
//!
//! # Соглашение об углах
//!
//! Азимут в градусах: `0` — прямо перед игроком, `+90` — справа, `-90` —
//! слева. Из стерео восстановима только передняя полусфера: источник спереди
//! и его зеркало сзади дают одинаковые ITD и ILD (конус неопределённости).
//! Поэтому таблица покрывает `[-90, +90]`, а фронт/тыл разрешается отдельно и
//! здесь не решается.
//!
//! ITD положителен, когда источник справа (правое ухо слышит раньше, левое
//! отстаёт).
//!
//! # Профиль CS2
//!
//! CS2 использует Steam Audio, его HRTF по умолчанию построен на наборе
//! KEMAR-типа. Значения по умолчанию заданы аналитически:
//!
//! Полоса ITD взята широкой (200-3000 Гц) намеренно. Для отдельного бина фаза
//! выше ~750 Гц уже неоднозначна, но SRP складывает бины когерентно, и чем
//! шире диапазон, тем сильнее гасятся ложные задержки. На узкой полосе
//! подавления не хватает, и между двумя источниками вырастает фантомный пик.
//!
//! * ITD — формула Вудворта `itd = (a/c)*(theta + sin theta)` при радиусе
//!   головы `a = 0.0875` м, что даёт максимум около 660 мкс;
//! * ILD — приближение `ild = ild_max(f) * sin(theta)` с ростом `ild_max` по
//!   частоте, как у сферической головы.
//!
//! Чтобы получить точную таблицу вместо приближения, нужно прогнать импульс
//! через HRTF Steam Audio на каждом азимуте и снять ITD/ILD — формат файла для
//! этого описан в [`Calibration::parse`].
//!
//! # Требуемые настройки CS2
//!
//! В игре обязательно выбрать вывод «Stereo Headphones», иначе Steam Audio не
//! применяет HRTF, панорамирование становится чисто амплитудным, ITD тождест-
//! венно равен нулю, и половина признаков в этом модуле теряет смысл.

/// Шаг таблицы по азимуту, градусы.
pub const AZ_STEP_DEG: f32 = 1.0;
/// Число узлов таблицы, покрывающих [-90, +90].
pub const AZ_STEPS: usize = 181;

pub fn az_index_to_deg(i: usize) -> f32 {
    -90.0 + i as f32 * AZ_STEP_DEG
}

/// Полоса частот со своей кривой ILD по азимуту.
#[derive(Clone, Debug)]
pub struct IldBand {
    pub f_lo: f32,
    pub f_hi: f32,
    /// ILD в дБ для каждого узла азимута, монотонно возрастает слева направо.
    pub lut: Vec<f32>,
}

#[derive(Clone, Debug)]
pub struct Calibration {
    pub name: String,
    /// Полоса, в которой оценивается ITD. Выше ~1.5 кГц фаза заворачивается.
    pub itd_band: (f32, f32),
    /// ITD в секундах для каждого узла азимута, монотонно возрастает.
    pub itd_lut: Vec<f32>,
    pub ild_bands: Vec<IldBand>,
}

impl Default for Calibration {
    fn default() -> Self {
        Self::cs2()
    }
}

impl Calibration {
    /// Профиль по умолчанию для CS2 / Steam Audio.
    pub fn cs2() -> Self {
        const HEAD_RADIUS_M: f32 = 0.0875;
        const SPEED_OF_SOUND: f32 = 343.0;

        let itd_lut = (0..AZ_STEPS)
            .map(|i| {
                let th = az_index_to_deg(i).to_radians();
                (HEAD_RADIUS_M / SPEED_OF_SOUND) * (th + th.sin())
            })
            .collect();

        // Максимальный ILD (на азимуте 90 градусов) растёт с частотой: на
        // низких частотах голова почти не затеняет, выше 6 кГц тень доходит до
        // ~18 дБ.
        let band_spec: [(f32, f32, f32); 5] = [
            (1500.0, 2500.0, 7.0),
            (2500.0, 4000.0, 11.0),
            (4000.0, 6000.0, 15.0),
            (6000.0, 9000.0, 18.0),
            (9000.0, 12000.0, 20.0),
        ];

        let ild_bands = band_spec
            .iter()
            .map(|&(f_lo, f_hi, ild_max)| IldBand {
                f_lo,
                f_hi,
                lut: (0..AZ_STEPS)
                    .map(|i| ild_max * az_index_to_deg(i).to_radians().sin())
                    .collect(),
            })
            .collect();

        Self {
            name: "cs2-steamaudio-default".to_string(),
            itd_band: (200.0, 3000.0),
            itd_lut,
            ild_bands,
        }
    }

    /// Максимальный физически возможный ITD, секунды.
    pub fn max_itd(&self) -> f32 {
        self.itd_lut.iter().fold(0.0f32, |acc, &v| acc.max(v.abs()))
    }

    /// ITD в секундах для заданного азимута, с линейной интерполяцией между
    /// узлами таблицы.
    pub fn azimuth_to_itd(&self, az_deg: f32) -> f32 {
        sample_lut(&self.itd_lut, az_deg)
    }

    /// Азимут по измеренному ITD. Вне диапазона таблицы зажимается к +-90.
    pub fn itd_to_azimuth(&self, itd_sec: f32) -> f32 {
        invert_monotonic(&self.itd_lut, itd_sec)
    }

    /// Азимут по ILD в дБ на частоте `freq_hz`, или `None`, если частота не
    /// попадает ни в одну калиброванную полосу.
    pub fn ild_to_azimuth(&self, ild_db: f32, freq_hz: f32) -> Option<f32> {
        let band = self
            .ild_bands
            .iter()
            .find(|b| freq_hz >= b.f_lo && freq_hz < b.f_hi)?;
        Some(invert_monotonic(&band.lut, ild_db))
    }

    /// Разбирает калибровку из текстового формата. Пустые строки и всё после
    /// `#` игнорируются.
    ///
    /// ```text
    /// name       cs2-measured
    /// itd_band   200 1400
    /// # азимут (град), ITD (мкс)
    /// itd        -90 -655
    /// itd        -89 -652
    /// ...
    /// # f_lo, f_hi (Гц), затем азимут и ILD (дБ)
    /// ild_band   4000 6000
    /// ild        -90 -15.2
    /// ...
    /// ```
    ///
    /// Узлы азимута, которых нет в файле, доинтерполируются по соседним, так
    /// что измерять каждый градус не обязательно — достаточно шага в 5-10
    /// градусов.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut cal = Calibration {
            name: "custom".to_string(),
            itd_band: (200.0, 1400.0),
            itd_lut: vec![f32::NAN; AZ_STEPS],
            ild_bands: Vec::new(),
        };
        let mut saw_itd = false;

        for (lineno, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut it = line.split_whitespace();
            let key = it.next().unwrap_or("");
            let rest: Vec<&str> = it.collect();
            let err = |m: &str| format!("строка {}: {}", lineno + 1, m);

            match key {
                "name" => {
                    cal.name = rest.join(" ");
                }
                "itd_band" => {
                    if rest.len() != 2 {
                        return Err(err("itd_band ожидает две частоты"));
                    }
                    cal.itd_band = (
                        rest[0].parse().map_err(|_| err("плохое число"))?,
                        rest[1].parse().map_err(|_| err("плохое число"))?,
                    );
                }
                "itd" => {
                    if rest.len() != 2 {
                        return Err(err("itd ожидает азимут и значение в мкс"));
                    }
                    let az: f32 = rest[0].parse().map_err(|_| err("плохой азимут"))?;
                    let us: f32 = rest[1].parse().map_err(|_| err("плохой ITD"))?;
                    let idx = az_to_index(az).ok_or_else(|| err("азимут вне [-90, 90]"))?;
                    cal.itd_lut[idx] = us * 1e-6;
                    saw_itd = true;
                }
                "ild_band" => {
                    if rest.len() != 2 {
                        return Err(err("ild_band ожидает две частоты"));
                    }
                    cal.ild_bands.push(IldBand {
                        f_lo: rest[0].parse().map_err(|_| err("плохое число"))?,
                        f_hi: rest[1].parse().map_err(|_| err("плохое число"))?,
                        lut: vec![f32::NAN; AZ_STEPS],
                    });
                }
                "ild" => {
                    if rest.len() != 2 {
                        return Err(err("ild ожидает азимут и значение в дБ"));
                    }
                    let band = cal
                        .ild_bands
                        .last_mut()
                        .ok_or_else(|| err("ild до первого ild_band"))?;
                    let az: f32 = rest[0].parse().map_err(|_| err("плохой азимут"))?;
                    let db: f32 = rest[1].parse().map_err(|_| err("плохой ILD"))?;
                    let idx = az_to_index(az).ok_or_else(|| err("азимут вне [-90, 90]"))?;
                    band.lut[idx] = db;
                }
                other => return Err(err(&format!("неизвестный ключ '{}'", other))),
            }
        }

        if !saw_itd {
            return Err("в калибровке нет ни одной строки itd".to_string());
        }
        fill_gaps(&mut cal.itd_lut)?;
        for band in &mut cal.ild_bands {
            fill_gaps(&mut band.lut)?;
        }
        Ok(cal)
    }
}

/// Значение калибровочной таблицы на произвольном азимуте, с линейной
/// интерполяцией между её узлами.
///
/// Индексировать таблицу напрямую нельзя: её сетка (`AZ_STEP_DEG`) не обязана
/// совпадать с сеткой, по которой считается отклик локализатора. Ровно на этом
/// уже был баг — таблица ILD читалась индексом чужой сетки, из-за чего весь
/// диапазон углов уезжал и любая разница уровней давала +-90 градусов.
pub fn sample_lut(lut: &[f32], az_deg: f32) -> f32 {
    debug_assert_eq!(lut.len(), AZ_STEPS, "таблица не на калибровочной сетке");
    if lut.is_empty() {
        return 0.0;
    }
    let pos = ((az_deg + 90.0) / AZ_STEP_DEG).clamp(0.0, (lut.len() - 1) as f32);
    let i = pos.floor() as usize;
    if i + 1 >= lut.len() {
        return lut[lut.len() - 1];
    }
    lut[i] + (lut[i + 1] - lut[i]) * (pos - i as f32)
}

fn az_to_index(az_deg: f32) -> Option<usize> {
    if !(-90.0..=90.0).contains(&az_deg) {
        return None;
    }
    Some(((az_deg + 90.0) / AZ_STEP_DEG).round() as usize)
}

/// Линейно доинтерполирует пропуски (NaN) и продлевает края константой.
fn fill_gaps(lut: &mut [f32]) -> Result<(), String> {
    let known: Vec<usize> = (0..lut.len()).filter(|&i| !lut[i].is_nan()).collect();
    if known.len() < 2 {
        return Err("нужно минимум два узла на таблицу".to_string());
    }
    let (first, last) = (known[0], known[known.len() - 1]);
    for i in 0..first {
        lut[i] = lut[first];
    }
    for i in last + 1..lut.len() {
        lut[i] = lut[last];
    }
    for w in known.windows(2) {
        let (a, b) = (w[0], w[1]);
        if b == a + 1 {
            continue;
        }
        let (va, vb) = (lut[a], lut[b]);
        for i in a + 1..b {
            let t = (i - a) as f32 / (b - a) as f32;
            lut[i] = va + (vb - va) * t;
        }
    }
    Ok(())
}

/// Обращает монотонно возрастающую таблицу «азимут -> признак»: по значению
/// признака возвращает азимут в градусах с линейной интерполяцией.
fn invert_monotonic(lut: &[f32], value: f32) -> f32 {
    if value <= lut[0] {
        return az_index_to_deg(0);
    }
    if value >= lut[lut.len() - 1] {
        return az_index_to_deg(lut.len() - 1);
    }
    // Таблица монотонна, поэтому работает двоичный поиск.
    let mut lo = 0usize;
    let mut hi = lut.len() - 1;
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if lut[mid] <= value {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let span = lut[hi] - lut[lo];
    let t = if span.abs() < f32::EPSILON {
        0.0
    } else {
        (value - lut[lo]) / span
    };
    az_index_to_deg(lo) + t * AZ_STEP_DEG
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn itd_table_is_invertible() {
        let cal = Calibration::cs2();
        for az in [-90.0f32, -60.0, -30.0, -5.0, 0.0, 5.0, 30.0, 60.0, 90.0] {
            let back = cal.itd_to_azimuth(cal.azimuth_to_itd(az));
            assert!((back - az).abs() < 1.0, "азимут {az} вернулся как {back}");
        }
    }

    #[test]
    fn itd_sign_points_right() {
        let cal = Calibration::cs2();
        assert!(cal.azimuth_to_itd(30.0) > 0.0, "справа ITD положителен");
        assert!(cal.azimuth_to_itd(-30.0) < 0.0, "слева ITD отрицателен");
        assert!(cal.azimuth_to_itd(0.0).abs() < 1e-9);
    }

    #[test]
    fn max_itd_matches_head_geometry() {
        // Радиус головы 8.75 см при скорости звука 343 м/с даёт около 656 мкс.
        let cal = Calibration::cs2();
        let us = cal.max_itd() * 1e6;
        assert!((us - 656.0).abs() < 5.0, "максимальный ITD вышел {us} мкс");
    }

    #[test]
    fn ild_grows_with_frequency() {
        let cal = Calibration::cs2();
        let low = cal.ild_to_azimuth(6.0, 2000.0).unwrap();
        let high = cal.ild_to_azimuth(6.0, 10000.0).unwrap();
        // Одна и та же разница уровней на высокой частоте означает угол ближе
        // к центру: там голова затеняет сильнее.
        assert!(high < low, "ожидалось {high} < {low}");
        assert!(low > 0.0 && high > 0.0, "положительный ILD — справа");
    }

    #[test]
    fn parses_and_interpolates_gaps() {
        let text = "\
name test
itd_band 200 3000
itd -90 -600
itd 0 0
itd 90 600
ild_band 3000 6000
ild -90 -12
ild 0 0
ild 90 12
";
        let cal = Calibration::parse(text).expect("не разобралось");
        assert_eq!(cal.name, "test");
        assert_eq!(cal.itd_band, (200.0, 3000.0));
        assert!((cal.max_itd() - 600e-6).abs() < 1e-9);
        // Узлы между заданными доинтерполированы линейно.
        assert!((cal.azimuth_to_itd(45.0) - 300e-6).abs() < 1e-6);
        assert_eq!(cal.ild_bands.len(), 1);
        assert!((sample_lut(&cal.ild_bands[0].lut, 45.0) - 6.0).abs() < 0.1);
    }

    #[test]
    fn rejects_broken_calibration() {
        assert!(
            Calibration::parse("itd_band 200 3000").is_err(),
            "нет узлов itd"
        );
        assert!(Calibration::parse("itd 0 0").is_err(), "одного узла мало");
        assert!(Calibration::parse("wat 1 2").is_err(), "неизвестный ключ");
        assert!(
            Calibration::parse("itd 200 5").is_err(),
            "азимут вне диапазона"
        );
        assert!(Calibration::parse("ild -10 3").is_err(), "ild до ild_band");
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let text = "\
# комментарий
itd -90 -600   # и в конце строки тоже

itd 90 600
";
        let cal = Calibration::parse(text).expect("не разобралось");
        assert!((cal.max_itd() - 600e-6).abs() < 1e-9);
    }

    #[test]
    fn sample_lut_interpolates_between_nodes() {
        let cal = Calibration::cs2();
        // Сетка отклика (2.5 градуса) не совпадает с калибровочной (1 градус),
        // поэтому таблицу нельзя индексировать чужим индексом.
        let a = sample_lut(&cal.itd_lut, 2.5);
        let lo = cal.itd_lut[92];
        let hi = cal.itd_lut[93];
        assert!(a > lo && a < hi, "{a} должно лежать между {lo} и {hi}");
    }
}
