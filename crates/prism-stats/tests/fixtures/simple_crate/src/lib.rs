/// Adds two numbers.
///
/// ```
/// assert_eq!(simple_crate::add(1, 2), 3);
/// ```
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub fn undocumented() -> bool {
    true
}

/// A documented struct.
pub struct Foo {
    value: i32,
}

struct Private {
    data: String,
}

pub(crate) fn internal_fn() -> u32 {
    42
}

unsafe fn dangerous() {
    std::ptr::null::<u8>().read();
}

pub fn with_unsafe_block() {
    unsafe {
        let _ = std::ptr::null::<u8>().read();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        assert_eq!(add(1, 2), 3);
    }

    #[test]
    fn test_add_negative() {
        assert_eq!(add(-1, 1), 0);
    }
}
