//! Racks in their server room: the positions of the room's grid
//! (`DCRoom::getAllPositions`, `getFilled`) and the check of a rack's place
//! (`Rack::prepareInput`).

use crate::php;

/// A place of a server room, `"<column>,<row>"` as stored by the racks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    pub position: String,
    /// Column in letters, as shown (`Toolbox::getBijectiveIndex`).
    pub column: String,
    pub row: i32,
}

/// Letters of a column: A to Z, then AA...
pub fn column_letters(mut index: i32) -> String {
    let mut letters = Vec::new();
    while index > 0 {
        index -= 1;
        letters.push(char::from(b'A' + u8::try_from(index % 26).unwrap_or(0)));
        index /= 26;
    }
    letters.iter().rev().collect()
}

/// Places of a room of `columns` by `rows` not taken by a rack (`taken`:
/// the positions of its racks outside the trash), the one of the rack being
/// edited (`current`) excepted.
pub fn free_positions(
    columns: i32,
    rows: i32,
    taken: &[String],
    current: Option<&str>,
) -> Vec<Position> {
    // `getFilled`: positions looking like coordinates, the current excepted.
    let filled: Vec<&str> = taken
        .iter()
        .map(String::as_str)
        .filter(|position| looks_like_coordinates(position))
        .filter(|position| {
            current.is_none_or(|current| current.is_empty() || !php::loose_eq(current, position))
        })
        .collect();
    let mut positions = Vec::new();
    for column in 1..=columns {
        for row in 1..=rows {
            let position = format!("{column},{row}");
            if !filled.contains(&position.as_str()) {
                positions.push(Position {
                    position,
                    column: column_letters(column),
                    row,
                });
            }
        }
    }
    positions
}

/// `preg_match('/(\d+),\s?(\d+)/')`: digits, a comma, an optional space,
/// digits, anywhere in the text.
fn looks_like_coordinates(position: &str) -> bool {
    let bytes = position.as_bytes();
    (0..bytes.len()).any(|comma| {
        bytes[comma] == b',' && comma > 0 && bytes[comma - 1].is_ascii_digit() && {
            let after = &bytes[comma + 1..];
            let after = after.strip_prefix(b" ").unwrap_or(after);
            after.first().is_some_and(u8::is_ascii_digit)
        }
    })
}

/// Whether the place of a rack is checked (`Rack::prepareInput`): in a
/// room, and at a position not equal to `0` for PHP (`$position == 0`).
pub fn position_is_checked(room: i32, position: &str) -> bool {
    room != 0 && php::numeric(position) != Some(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_are_letters() {
        assert_eq!(column_letters(1), "A");
        assert_eq!(column_letters(26), "Z");
        assert_eq!(column_letters(27), "AA");
        assert_eq!(column_letters(52), "AZ");
        assert_eq!(column_letters(53), "BA");
    }

    #[test]
    fn taken_places_are_left_out() {
        let taken = vec!["1,2".to_owned(), "2, 1".to_owned(), "x".to_owned()];
        let free: Vec<String> = free_positions(2, 2, &taken, None)
            .into_iter()
            .map(|position| position.position)
            .collect();
        // "2, 1" is taken for getFilled but is not the key "2,1".
        assert_eq!(free, ["1,1", "2,1", "2,2"]);
        let free = free_positions(2, 2, &taken, Some("1,2"));
        assert_eq!(free.len(), 4);
        assert_eq!(free[1].column, "A");
        assert_eq!(free[1].row, 2);
    }

    #[test]
    fn zero_positions_are_not_checked() {
        assert!(!position_is_checked(0, "1,1"));
        assert!(!position_is_checked(3, "0"));
        assert!(position_is_checked(3, ""));
        assert!(position_is_checked(3, "1,1"));
    }
}
