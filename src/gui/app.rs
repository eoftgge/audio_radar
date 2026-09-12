use crate::gui::draw::draw_radar;
use crate::types::RadarFrame;
use eframe::Frame;
use eframe::egui::{Context, Id, LayerId, Order, ViewportCommand, Visuals, WindowLevel};
use std::sync::mpsc;
use std::time::Duration;

const MAX_FPS: u64 = 144;
const FRAME_TIME: Duration = Duration::from_millis(1000 / MAX_FPS);

pub struct IndicatorApp {
    rx: mpsc::Receiver<RadarFrame>,
    current: RadarFrame,
    initialized: bool,
}

impl IndicatorApp {
    pub fn new(rx: mpsc::Receiver<RadarFrame>) -> Self {
        Self {
            rx,
            current: RadarFrame::default(),
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

        // Анализ идёт примерно втрое чаще отрисовки, поэтому берётся последний
        // кадр. Атаку при этом терять нельзя: она короткая, а показать её
        // нужно обязательно.
        let mut transient = false;
        while let Ok(frame) = self.rx.try_recv() {
            transient |= frame.transient;
            self.current = frame;
        }
        self.current.transient |= transient;

        let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::from("indicator")));
        draw_radar(&painter, ctx, &self.current);

        // Уровень окна подтверждается каждый кадр, а не один раз при запуске:
        // полноэкранная игра иначе перекрывает оверлей при переключении.
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
        ctx.request_repaint_after(FRAME_TIME);
    }

    fn clear_color(&self, _: &Visuals) -> [f32; 4] {
        [0., 0., 0., 0.]
    }
}
