use std::f32::consts::PI;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Complex {
    pub re: f32,
    pub im: f32,
}

impl Complex {
    pub const ZERO: Self = Self { re: 0.0, im: 0.0 };
    pub fn new(re: f32, im: f32) -> Self {
        Self { re, im }
    }

    pub fn add(self, o: Self) -> Self {
        Self::new(self.re + o.re, self.im + o.im)
    }

    pub fn sub(self, o: Self) -> Self {
        Self::new(self.re - o.re, self.im - o.im)
    }

    pub fn mul(self, o: Self) -> Self {
        Self::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }

    pub fn scale(self, s: f32) -> Self {
        Self::new(self.re * s, self.im * s)
    }

    pub fn conj(self) -> Self {
        Self::new(self.re, -self.im)
    }

    pub fn norm(self) -> f32 {
        (self.re * self.re + self.im * self.im).sqrt()
    }
}

pub struct Fft {
    n: usize,
    rev: Vec<usize>,
    twiddles: Vec<Complex>,
}

impl Fft {
    pub fn new(n: usize) -> Self {
        assert!(
            n.is_power_of_two() && n >= 2,
            "размер FFT должен быть степенью двойки"
        );
        let log_n = n.trailing_zeros();

        let mut rev = vec![0usize; n];
        for (i, slot) in rev.iter_mut().enumerate() {
            let mut r = 0usize;
            for bit in 0..log_n {
                if i & (1 << bit) != 0 {
                    r |= 1 << (log_n - 1 - bit);
                }
            }
            *slot = r;
        }

        let mut twiddles = Vec::with_capacity(n / 2);
        for j in 0..n / 2 {
            let a = -2.0 * PI * (j as f32) / (n as f32);
            twiddles.push(Complex::new(a.cos(), a.sin()));
        }

        Self { n, rev, twiddles }
    }

    pub fn forward(&self, data: &mut [Complex]) {
        self.run(data, false);
    }

    pub fn inverse(&self, data: &mut [Complex]) {
        self.run(data, true);
        let s = 1.0 / self.n as f32;
        for c in data.iter_mut() {
            *c = c.scale(s);
        }
    }

    fn run(&self, data: &mut [Complex], inverse: bool) {
        assert_eq!(
            data.len(),
            self.n,
            "длина буфера не совпадает с размером FFT"
        );

        for i in 0..self.n {
            let j = self.rev[i];
            if i < j {
                data.swap(i, j);
            }
        }

        let mut len = 2;
        while len <= self.n {
            let half = len / 2;
            let step = self.n / len;
            for start in (0..self.n).step_by(len) {
                for k in 0..half {
                    let mut w = self.twiddles[k * step];
                    if inverse {
                        w.im = -w.im;
                    }
                    let u = data[start + k];
                    let v = data[start + k + half].mul(w);
                    data[start + k] = u.add(v);
                    data[start + k + half] = u.sub(v);
                }
            }
            len <<= 1;
        }
    }
}

pub fn unpack_two_real(packed: &[Complex], left: &mut [Complex], right: &mut [Complex]) {
    let n = packed.len();
    let half = n / 2;
    assert!(
        left.len() > half && right.len() > half,
        "буферы спектров слишком малы"
    );

    for k in 0..=half {
        let a = packed[k];
        let b = packed[(n - k) % n].conj();

        left[k] = a.add(b).scale(0.5);

        let d = a.sub(b);
        right[k] = Complex::new(d.im * 0.5, -d.re * 0.5);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn naive_dft(x: &[Complex]) -> Vec<Complex> {
        let n = x.len();
        (0..n)
            .map(|k| {
                x.iter().enumerate().fold(Complex::ZERO, |acc, (m, &xm)| {
                    let a = -2.0 * PI * k as f32 * m as f32 / n as f32;
                    acc.add(xm.mul(Complex::new(a.cos(), a.sin())))
                })
            })
            .collect()
    }

    fn noise(state: &mut u32) -> f32 {
        *state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        ((*state >> 8) as f32 / 8388608.0) - 1.0
    }

    #[test]
    fn matches_naive_dft() {
        let n = 256;
        let fft = Fft::new(n);
        let mut seed = 12345;
        let src: Vec<Complex> = (0..n)
            .map(|_| Complex::new(noise(&mut seed), noise(&mut seed)))
            .collect();

        let want = naive_dft(&src);
        let mut got = src.clone();
        fft.forward(&mut got);

        for k in 0..n {
            assert!(
                got[k].sub(want[k]).norm() < 1e-2,
                "бин {k} расходится с DFT"
            );
        }
    }

    #[test]
    fn round_trip_restores_signal() {
        let n = 256;
        let fft = Fft::new(n);
        let mut seed = 999;
        let src: Vec<Complex> = (0..n)
            .map(|_| Complex::new(noise(&mut seed), noise(&mut seed)))
            .collect();

        let mut buf = src.clone();
        fft.forward(&mut buf);
        fft.inverse(&mut buf);

        for k in 0..n {
            assert!(buf[k].sub(src[k]).norm() < 1e-4, "бин {k} не восстановился");
        }
    }

    #[test]
    fn unpacks_two_real_spectra() {
        let n = 256;
        let fft = Fft::new(n);
        let mut seed = 4242;
        let l: Vec<f32> = (0..n).map(|_| noise(&mut seed)).collect();
        let r: Vec<f32> = (0..n).map(|_| noise(&mut seed)).collect();

        let mut packed: Vec<Complex> = (0..n).map(|i| Complex::new(l[i], r[i])).collect();
        fft.forward(&mut packed);
        let mut ls = vec![Complex::ZERO; n / 2 + 1];
        let mut rs = vec![Complex::ZERO; n / 2 + 1];
        unpack_two_real(&packed, &mut ls, &mut rs);

        let lref = naive_dft(&l.iter().map(|&v| Complex::new(v, 0.0)).collect::<Vec<_>>());
        let rref = naive_dft(&r.iter().map(|&v| Complex::new(v, 0.0)).collect::<Vec<_>>());
        for k in 0..=n / 2 {
            assert!(ls[k].sub(lref[k]).norm() < 1e-2, "левый спектр, бин {k}");
            assert!(rs[k].sub(rref[k]).norm() < 1e-2, "правый спектр, бин {k}");
        }
    }
}
