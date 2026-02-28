use eframe::egui::{Color32, Context, Id, Painter, Stroke, pos2};
use std::f32::consts::PI;

pub fn draw_indicator_xy(painter: &Painter, ctx: &Context, x_val: f32, y_val: f32, intensity: f32) {
    let rect = ctx.content_rect();
    let center = rect.center();
    let max_radius = 120.0;
    let head_length = 11.0;
    let open_angle = 35.0 * (PI / 180.0);
    let stroke_width = 1.5;
    let color = Color32::from_rgba_premultiplied(255, 30, 30, 240);
    let shadow_color = Color32::from_black_alpha(100);

    if intensity < 0.002 { return; }

    let anim_x = ctx.animate_value_with_time(Id::new("rad_x"), x_val, 0.04);
    let anim_y = ctx.animate_value_with_time(Id::new("rad_y"), -y_val, 0.04);
    let angle = (anim_y).atan2(anim_x);

    let tip_x = center.x + max_radius * angle.cos();
    let tip_y = center.y + max_radius * angle.sin();
    let tip_pos = pos2(tip_x, tip_y);
    let angle_left = angle + PI - open_angle;
    let angle_right = angle + PI + open_angle;

    let end_left = pos2(
        tip_x + head_length * angle_left.cos(),
        tip_y + head_length * angle_left.sin()
    );
    let end_right = pos2(
        tip_x + head_length * angle_right.cos(),
        tip_y + head_length * angle_right.sin()
    );
    let shadow_stroke = Stroke::new(stroke_width + 1.0, shadow_color);
    let shadow_offset = 1.0;
    let tip_shadow = pos2(tip_pos.x + shadow_offset, tip_pos.y + shadow_offset);
    let left_shadow = pos2(end_left.x + shadow_offset, end_left.y + shadow_offset);
    let right_shadow = pos2(end_right.x + shadow_offset, end_right.y + shadow_offset);

    painter.line_segment([tip_shadow, left_shadow], shadow_stroke);
    painter.line_segment([tip_shadow, right_shadow], shadow_stroke);

    let stroke = Stroke::new(stroke_width, color);
    painter.line_segment([tip_pos, end_left], stroke);
    painter.line_segment([tip_pos, end_right], stroke);

}

pub fn draw_indicator_ild(painter: &Painter, ctx: &Context, ild: f32) {
    let rect = ctx.content_rect();
    let center = rect.center();
    let max_radius = 120.0;
    let sensitivity = 15.0;
    let smoothing = 0.06;

    let target = (ild / sensitivity).clamp(-1.0, 1.0);
    let val = ctx.animate_value_with_time(Id::new("radar_smooth"), target, smoothing);
    let start_angle = -PI / 2.0;
    let max_deflection = PI / 3.0;

    let bg_color = Color32::from_white_alpha(30);
    let angles = [
        start_angle - max_deflection,
        start_angle,
        start_angle + max_deflection,
    ];

    for angle in angles {
        let bx = center.x + max_radius * angle.cos();
        let by = center.y + max_radius * angle.sin();
        painter.circle_filled(pos2(bx, by), 3.0, bg_color);
    }

    let is_loud = val.abs() > 0.1;
    let color = if is_loud {
        Color32::from_rgba_premultiplied(255, 50, 50, 230)
    } else {
        Color32::from_rgba_premultiplied(50, 255, 50, 100)
    };
    let current_angle = start_angle + (val * max_deflection);
    let x = center.x + max_radius * current_angle.cos();
    let y = center.y + max_radius * current_angle.sin();
    painter.circle_filled(pos2(x, y), 4.0, color);
    painter.circle_stroke(
        pos2(x, y),
        5.0,
        Stroke::new(1.0, Color32::from_white_alpha(100)),
    );

    if is_loud {
        painter.line_segment(
            [
                pos2(x, y),
                pos2(
                    center.x + (max_radius - 15.0) * current_angle.cos(),
                    center.y + (max_radius - 15.0) * current_angle.sin(),
                ),
            ],
            Stroke::new(2.0, color),
        );
    }
}
