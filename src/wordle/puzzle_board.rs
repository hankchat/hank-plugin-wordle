use crate::wordle::Tile;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub struct PuzzleBoard {
    pub board: Vec<Vec<Tile>>,
}

impl From<Vec<Vec<Tile>>> for PuzzleBoard {
    fn from(board: Vec<Vec<Tile>>) -> Self {
        PuzzleBoard { board }
    }
}

impl From<String> for PuzzleBoard {
    fn from(value: String) -> Self {
        let mut board: Vec<Vec<Tile>> = Vec::new();

        let mut lines = value.lines();
        while let Some(line) = lines.next() {
            if board.len() == 6 {
                break;
            }

            if line.is_empty() {
                continue;
            }

            // @TODO there's currently no constaints on how long a row is, but
            // we should ensure there is exactly five for it to be valid.

            let row: Vec<Tile> = if line.contains("::") {
                // Handle Slack messages which convert emoji to textual representation.
                line.split("::")
                    .into_iter()
                    .map(|t| {
                        let t = t.replace(":", "");
                        t.try_into().unwrap()
                    })
                    .collect()
            } else {
                // Handle Discord messages which just use raw emoji.
                line.split("")
                    .filter(|&x| !x.is_empty())
                    .into_iter()
                    .map(|t| t.to_string().try_into().unwrap())
                    .collect()
            };

            board.push(row);
        }

        // @TODO technically the board should have _at least_ one row, and if
        // that row is not completely green it is not valid either.

        PuzzleBoard { board }
    }
}

impl From<PuzzleBoard> for String {
    fn from(puzzle: PuzzleBoard) -> Self {
        puzzle
            .board
            .into_iter()
            .map(|line| {
                line.into_iter()
                    .map(|tile| String::from(tile))
                    .collect::<Vec<String>>()
                    .join("")
            })
            .collect::<Vec<String>>()
            .join("\n")
    }
}
