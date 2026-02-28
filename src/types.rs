pub enum RadarMessage {
    Direction(f32),
    Surround { x: f32, y: f32, intensity: f32 },
}
