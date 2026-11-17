//! Logical widget dimensions, independent from platform/DPI and timer accounting.
pub const DEFAULT_SIZE: (u32, u32) = (180, 190);
pub const MIN_SIZE: (u32, u32) = (80, 90);
pub const MAX_SIZE: (u32, u32) = (600, 640);

pub fn bounded_size(size: (u32, u32)) -> (u32, u32) {
    (
        size.0.clamp(MIN_SIZE.0, MAX_SIZE.0),
        size.1.clamp(MIN_SIZE.1, MAX_SIZE.1),
    )
}

pub fn effective_size(saved: Option<(u32, u32)>) -> (u32, u32) {
    bounded_size(saved.unwrap_or(DEFAULT_SIZE))
}

pub fn resized_size(origin: (u32, u32), delta: (i32, i32)) -> (u32, u32) {
    let width = (i64::from(origin.0) + i64::from(delta.0))
        .clamp(i64::from(MIN_SIZE.0), i64::from(MAX_SIZE.0));
    let height = (i64::from(origin.1) + i64::from(delta.1))
        .clamp(i64::from(MIN_SIZE.1), i64::from(MAX_SIZE.1));
    (width as u32, height as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resize_is_relative_and_clamped_without_overflow() {
        assert_eq!(resized_size(DEFAULT_SIZE, (40, 30)), (220, 220));
        assert_eq!(resized_size(DEFAULT_SIZE, (i32::MIN, i32::MAX)), (80, 640));
        assert_eq!(effective_size(None), DEFAULT_SIZE);
        assert_eq!(effective_size(Some((0, u32::MAX))), (80, 640));
    }
}
