use crate::gui::draw::draw_indicator;
use crate::types::RadarMessage;
use eframe::egui::{
    Context, Id, LayerId, Order, ViewportCommand, Visuals, WindowLevel,
};
use eframe::Frame;
use std::sync::mpsc;
use std::time::Duration;

const MAX_FPS: u64 = 144;
const FRAME_TIME: Duration = Duration::from_millis(1000 / MAX_FPS);

pub struct IndicatorApp {
    rx: mpsc::Receiver<RadarMessage>,
    current_message: RadarMessage,
    initialized: bool,
}

impl IndicatorApp {
    pub fn new(rx: mpsc::Receiver<RadarMessage>) -> Self {
        Self {
            rx,
            current_message: RadarMessage::Surround { x: 0.0, y: 0.0, intensity: 0.0 },
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
        while let Ok(message) = self.rx.try_recv() {
            self.current_message = message;
        }
        let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::from("indicator")));
        match self.current_message {
            RadarMessage::Surround { x, y, intensity } => draw_indicator(&painter, ctx, x, y, intensity),
        }
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
        ctx.request_repaint_after(FRAME_TIME);
    }

    fn clear_color(&self, _: &Visuals) -> [f32; 4] {
        [0., 0., 0., 0.]
    }
}
