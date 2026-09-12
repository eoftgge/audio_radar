use crate::gui::draw::draw_radar;
use crate::types::RadarFrame;
use eframe::Frame;
use eframe::egui::{Context, Id, LayerId, Order, ViewportCommand, Visuals, WindowLevel};
use std::sync::mpsc;
use std::time::Duration;

const MAX_FPS: u64 = 144;
const FRAME_TIME: Duration = Duration::from_millis(1000 / MAX_FPS);
const PULSE_TAU_S: f32 = 0.16;

pub struct IndicatorApp {
    rx: mpsc::Receiver<RadarFrame>,
    current: RadarFrame,
    pulse: f32,
    initialized: bool,
}

impl IndicatorApp {
    pub fn new(rx: mpsc::Receiver<RadarFrame>) -> Self {
        Self {
            rx,
            current: RadarFrame::default(),
            pulse: 0.0,
            initialized: false,
        }
    }
}

impl eframe::App for IndicatorApp {
    fn update(&mut self, ctx: &Context, _: &mut Frame) {
        if !self.initialized {
            ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(true));
            self.initialized = true;
        }

        let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.25);
        self.pulse *= (-dt / PULSE_TAU_S).exp();

        let mut transient = false;
        while let Ok(frame) = self.rx.try_recv() {
            transient |= frame.transient;
            self.current = frame;
        }
        if transient {
            self.pulse = 1.0;
        }

        let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::from("indicator")));
        draw_radar(&painter, ctx, &self.current, self.pulse);

        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
        ctx.request_repaint_after(FRAME_TIME);
    }

    fn clear_color(&self, _: &Visuals) -> [f32; 4] {
        [0., 0., 0., 0.]
    }
}
