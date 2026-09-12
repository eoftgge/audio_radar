use crate::types::{RESPONSE_BINS, RadarFrame};
use eframe::egui::{Color32, Context, Painter, Pos2, Shape, Stroke, pos2};

const RING_RADIUS: f32 = 160.0;
const RESPONSE_HEIGHT: f32 = 18.0;
const MARKER_SIZE: f32 = 14.0;
const LEVEL_FLOOR: f32 = 0.03;
const FRONT_COLOR: Color32 = Color32::from_rgb(255, 60, 60);
const BACK_COLOR: Color32 = Color32::from_rgb(255, 170, 60);
const RING_COLOR: Color32 = Color32::from_rgb(150, 150, 160);

pub fn draw_radar(painter: &Painter, ctx: &Context, frame: &RadarFrame) {
    let center = ctx.content_rect().center();

    draw_ring(painter, center);
    draw_response(painter, center, frame);

    for i in 0..frame.count {
        let s = frame.sources[i];
        if s.level < LEVEL_FLOOR {
            continue;
        }

        let alpha = (60.0 + 195.0 * s.confidence.clamp(0.0, 1.0)) as u8;
        let size = MARKER_SIZE * (0.55 + 0.45 * s.level.clamp(0.0, 1.0));

        draw_marker(painter, center, s.azimuth_deg, size, FRONT_COLOR, alpha);
        draw_marker(
            painter,
            center,
            mirror_azimuth(s.azimuth_deg),
            size * 0.78,
            BACK_COLOR,
            (alpha as f32 * 0.55) as u8,
        );
    }

    draw_center(painter, center, frame.transient);
}

fn mirror_azimuth(az_deg: f32) -> f32 {
    let m = 180.0 - az_deg;
    if m > 180.0 { m - 360.0 } else { m }
}

fn point(center: Pos2, az_deg: f32, radius: f32) -> Pos2 {
    let a = az_deg.to_radians();
    pos2(center.x + radius * a.sin(), center.y - radius * a.cos())
}

fn draw_ring(painter: &Painter, center: Pos2) {
    let front: Vec<Pos2> = (-90..=90)
        .step_by(3)
        .map(|d| point(center, d as f32, RING_RADIUS))
        .collect();
    painter.add(Shape::line(
        front,
        Stroke::new(1.0_f32, RING_COLOR.gamma_multiply(0.55)),
    ));

    let mut d = 90.0f32;
    while d < 270.0 {
        let a = point(center, d, RING_RADIUS);
        let b = point(center, (d + 4.0).min(270.0), RING_RADIUS);
        painter.line_segment([a, b], Stroke::new(1.0_f32, RING_COLOR.gamma_multiply(0.3)));
        d += 9.0;
    }
}

fn draw_response(painter: &Painter, center: Pos2, frame: &RadarFrame) {
    for i in 0..RESPONSE_BINS {
        let v = frame.response[i];
        if v <= 0.02 {
            continue;
        }
        let az = RadarFrame::response_azimuth(i);
        let len = RESPONSE_HEIGHT * v;
        let alpha = (30.0 + 110.0 * v) as u8;

        for (angle, color, scale) in [
            (az, FRONT_COLOR, 1.0f32),
            (mirror_azimuth(az), BACK_COLOR, 0.6),
        ] {
            painter.line_segment(
                [
                    point(center, angle, RING_RADIUS),
                    point(center, angle, RING_RADIUS + len * scale),
                ],
                Stroke::new(
                    2.0_f32,
                    Color32::from_rgba_unmultiplied(
                        color.r(),
                        color.g(),
                        color.b(),
                        (alpha as f32 * scale) as u8,
                    ),
                ),
            );
        }
    }
}

fn draw_marker(painter: &Painter, center: Pos2, az_deg: f32, size: f32, color: Color32, alpha: u8) {
    let a = az_deg.to_radians();
    let radial = (a.sin(), -a.cos());
    let perp = (a.cos(), a.sin());

    let at = |along: f32, across: f32| {
        pos2(
            center.x + radial.0 * along + perp.0 * across,
            center.y + radial.1 * along + perp.1 * across,
        )
    };

    let tip = at(RING_RADIUS + size * 0.6, 0.0);
    let p1 = at(RING_RADIUS - size * 0.45, size * 0.38);
    let p2 = at(RING_RADIUS - size * 0.45, -size * 0.38);

    let fill = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);
    painter.add(Shape::convex_polygon(
        vec![tip, p1, p2],
        fill,
        Stroke::new(1.0_f32, Color32::from_black_alpha(160)),
    ));
}

fn draw_center(painter: &Painter, center: Pos2, transient: bool) {
    painter.circle_filled(center, 3.0, Color32::from_white_alpha(140));
    painter.line_segment(
        [
            point(center, 0.0, RING_RADIUS - 10.0),
            point(center, 0.0, RING_RADIUS - 2.0),
        ],
        Stroke::new(1.5_f32, RING_COLOR.gamma_multiply(0.8)),
    );
    if transient {
        painter.circle_stroke(
            center,
            7.0,
            Stroke::new(1.5_f32, Color32::from_white_alpha(90)),
        );
    }
}
