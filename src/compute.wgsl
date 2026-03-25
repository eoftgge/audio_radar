@group(0) @binding(0) var<storage, read> left_buf: array<f32>;
@group(0) @binding(1) var<storage, read> right_buf: array<f32>;
@group(0) @binding(2) var<storage, read_write> out_buf: array<f32>;

@compute @workgroup_size(128)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let shift_idx = global_id.x;
    if (shift_idx >= 81u) { return; }

    let max_shift: i32 = 40;
    let actual_shift: i32 = i32(shift_idx) - max_shift;
    let num_samples: i32 = 1024;

    var sum: f32 = 0.0;

    for (var i: i32 = 0; i < num_samples; i = i + 1) {
        let j: i32 = i + actual_shift;
        if (j >= 0 && j < num_samples) {
            sum = sum + (left_buf[i] * right_buf[j]);
        }
    }

    out_buf[shift_idx] = sum;
}