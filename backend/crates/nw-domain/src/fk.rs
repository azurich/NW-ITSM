//! Legacy foreign key helpers.
//!
//! The ITSM-NG schema has no real foreign keys and uses `0` to mean
//! "no related row". These helpers convert that convention to `Option`.

/// Converts a legacy foreign key value (`0` = none) to an `Option`.
pub fn fk(id: i32) -> Option<i32> {
    (id > 0).then_some(id)
}

#[cfg(test)]
mod tests {
    use super::fk;

    #[test]
    fn zero_and_negative_mean_none() {
        assert_eq!(fk(0), None);
        assert_eq!(fk(-1), None);
        assert_eq!(fk(42), Some(42));
    }
}
