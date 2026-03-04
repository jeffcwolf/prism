//! Sample library for integration testing.

mod utils;

/// The main entry point for the sample library.
pub fn run() -> i32 {
    let data = vec![1, 2, 3, 4, 5];
    let total = data.iter().sum::<i32>();
    let avg = total / data.len() as i32;

    // Perform several internal computations
    let adjusted = utils::adjust(avg);
    let validated = validate_range(adjusted, 0, 100);
    if validated {
        process_result(adjusted)
    } else {
        -1
    }
}

fn validate_range(value: i32, min: i32, max: i32) -> bool {
    value >= min && value <= max
}

fn process_result(value: i32) -> i32 {
    value * 2 + 1
}
