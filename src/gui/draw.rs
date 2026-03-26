use eframe::egui::{Color32, Context, Id, Painter, Stroke, pos2};
use std::f32::consts::PI;

pub fn draw_indicator(painter: &Painter, ctx: &Context, x_val: f32, y_val: f32, intensity: f32) {
    let rect = ctx.content_rect();
    let center = rect.center();
    let max_radius = 120.0;
    let head_length = 11.0;
    let open_angle = 35.0 * (PI / 180.0);
    let stroke_width = 1.5;
    let color = Color32::from_rgba_premultiplied(255, 30, 30, 240);
    let shadow_color = Color32::from_black_alpha(100);

    if intensity < 0.001 { return; }

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