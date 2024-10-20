use crate::wordle::Tile;
use anyhow::{bail, Context as _};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PuzzleBoard {
    pub board: Vec<Vec<Tile>>,
}

impl From<Vec<Vec<Tile>>> for PuzzleBoard {
    fn from(board: Vec<Vec<Tile>>) -> Self {
        PuzzleBoard { board }
    }
}

impl TryFrom<String> for PuzzleBoard {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let mut board: Vec<Vec<Tile>> = Vec::new();

        let mut lines = value.lines();
        while let Some(line) = lines.next() {
            if board.len() == 6 {
                break;
            }

            if line.is_empty() {
                continue;
            }

            let row: Vec<Tile> = if line.contains("::") {
                // Handle Slack messages which convert emoji to textual representation.
                line.split("::")
                    .into_iter()
                    .map(|t| {
                        let t = t.replace(":", "");
                        t.try_into()
                            .context("couldn't convert slack emoji name to tile")
                    })
                    .collect::<Result<_, _>>()?
            } else {
                // Handle Discord messages which just use raw emoji.
                line.split("")
                    .filter(|&x| !x.is_empty())
                    .into_iter()
                    .map(|t| {
                        t.to_string()
                            .try_into()
                            .context("couldn't convert discord emoji to tile")
                    })
                    .collect::<Result<_, _>>()?
            };

            if row.len() > 5 {
                bail!("invalid puzzle board, row was {} long", row.len());
            }

            board.push(row);
        }

        match board.len() {
            0 => bail!("invalid puzzle board, now rows"),
            1 => {
                if !board.first().unwrap().iter().all(|t| *t == Tile::Green) {
                    bail!("invalid puzzle board, only one row and not all green");
                }
            }
            _ => (),
        }

        Ok(PuzzleBoard { board })
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
