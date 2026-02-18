use anyhow::bail;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Tile {
    White,
    Black,
    Yellow,
    Green,
}

impl From<Tile> for String {
    fn from(value: Tile) -> Self {
        use Tile::*;

        let tile = match value {
            White => "⬜",
            Black => "⬛",
            Yellow => "🟨",
            Green => "🟩",
        };

        tile.into()
    }
}

impl TryFrom<String> for Tile {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        use Tile::*;

        Ok(match value.as_str() {
            "white_large_square" => White,
            "black_large_square" => Black,
            "large_yellow_square" => Yellow,
            "large_green_square" => Green,
            "⬜" => White,
            "⬛" => Black,
            "🟨" => Yellow,
            "🟩" => Green,
            _ => bail!("couldn't convert {} to tile", value),
        })
    }
}
