use crate::types::RadarFrame;
use eframe::egui::{Color32, Context, Painter, Pos2, Shape, Stroke, pos2};

const RADIUS_FRACTION: f32 = 0.33;
const RADIUS_MIN: f32 = 150.0;

const ARC_THICKNESS: f32 = 12.0;
const ARC_SPAN_CONFIDENT_DEG: f32 = 8.0;
const ARC_SPAN_VAGUE_DEG: f32 = 34.0;
const HALO_WIDTH: f32 = 6.0;

const BACK_THICKNESS_SCALE: f32 = 0.42;
const BACK_ALPHA_SCALE: f32 = 0.45;

const LEVEL_FLOOR: f32 = 0.04;
const PULSE_GAIN: f32 = 0.45;

const MARKER: Color32 = Color32::from_rgb(150, 240, 255);
const HALO: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 150);

pub fn draw_radar(painter: &Painter, ctx: &Context, frame: &RadarFrame, pulse: f32) {
    let rect = ctx.content_rect();
    let center = rect.center();
    let radius = (rect.width().min(rect.height()) * RADIUS_FRACTION).max(RADIUS_MIN);
    let boost = 1.0 + PULSE_GAIN * pulse.clamp(0.0, 1.0);

    for i in 0..frame.count {
        let source = frame.sources[i];
        if source.level < LEVEL_FLOOR {
            continue;
        }

        let confidence = source.confidence.clamp(0.0, 1.0);
        let level = source.level.clamp(0.0, 1.0);
        let span = ARC_SPAN_VAGUE_DEG + (ARC_SPAN_CONFIDENT_DEG - ARC_SPAN_VAGUE_DEG) * confidence;
        let width = ARC_THICKNESS * (0.55 + 0.45 * level) * boost;
        let alpha = (110.0 + 145.0 * confidence).min(255.0) as u8;

        draw_arc(
            painter,
            center,
            radius,
            source.azimuth_deg,
            span,
            width,
            alpha,
        );
        draw_arc(
            painter,
            center,
            radius,
            mirror_azimuth(source.azimuth_deg),
            span,
            width * BACK_THICKNESS_SCALE,
            (alpha as f32 * BACK_ALPHA_SCALE) as u8,
        );
    }
}

fn mirror_azimuth(az_deg: f32) -> f32 {
    let m = 180.0 - az_deg;
    if m > 180.0 { m - 360.0 } else { m }
}

fn point(center: Pos2, az_deg: f32, radius: f32) -> Pos2 {
    let a = az_deg.to_radians();
    pos2(center.x + radius * a.sin(), center.y - radius * a.cos())
}

fn draw_arc(
    painter: &Painter,
    center: Pos2,
    radius: f32,
    az_deg: f32,
    span_deg: f32,
    width: f32,
    alpha: u8,
) {
    if alpha == 0 || width <= 0.0 {
        return;
    }

    let half = span_deg * 0.5;
    let steps = (span_deg / 2.0).ceil().max(2.0) as usize;
    let points: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            point(center, az_deg - half + span_deg * t, radius)
        })
        .collect();

    painter.add(Shape::line(
        points.clone(),
        Stroke::new(width + HALO_WIDTH, HALO),
    ));
    painter.add(Shape::line(
        points,
        Stroke::new(
            width,
            Color32::from_rgba_unmultiplied(MARKER.r(), MARKER.g(), MARKER.b(), alpha),
        ),
    ));
}
