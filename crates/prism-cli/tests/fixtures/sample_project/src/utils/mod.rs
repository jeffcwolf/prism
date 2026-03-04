/// Adjust a value with internal logic.
pub fn adjust(value: i32) -> i32 {
    let step1 = value + 10;
    let step2 = step1 * 3;
    let step3 = step2 - 5;
    clamp(step3, 0, 200)
}

fn clamp(value: i32, min: i32, max: i32) -> i32 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

fn normalize(value: i32) -> f64 {
    value as f64 / 200.0
}
