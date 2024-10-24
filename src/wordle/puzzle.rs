use crate::wordle::PuzzleBoard;
use anyhow::{bail, Context as _, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct Puzzle {
    pub day_offset: u32,
    pub attempts: u32,
    pub solved: bool,
    pub hard_mode: bool,
    pub board: PuzzleBoard,
}

impl Puzzle {
    pub fn new(puzzle: impl Into<String>) -> Result<Self> {
        Self::try_from(puzzle.into())
    }
}

impl TryFrom<Puzzle> for String {
    type Error = anyhow::Error;

    fn try_from(puzzle: Puzzle) -> Result<Self, Self::Error> {
        let mut string = String::from("Wordle ");

        let day_offset = puzzle
            .day_offset
            .to_string()
            .as_bytes()
            .rchunks(3)
            .rev()
            .map(std::str::from_utf8)
            .collect::<Result<Vec<&str>, _>>()
            .context("couldn't format day_offset")?
            .join(",");

        string.push_str(&day_offset);
        string.push(' ');

        string.push_str(&puzzle.attempts.to_string());
        string.push_str("/6");

        if puzzle.hard_mode {
            string.push('*');
        }

        string.push_str("\n\n");

        string.push_str(&String::from(puzzle.board));

        Ok(string)
    }
}

impl TryFrom<String> for Puzzle {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let mut lines = value.lines();
        let first_line = lines.next().context("couldn't get first line of puzzle")?;

        let re =
            Regex::new(r"Wordle (?<day_offset>\d+,\d+) (?<attempts>([1-6]|X))\/6(?<hard_mode>\*)?")
                .context("couldn't construct regex")?;
        let Some(captures) = re.captures(first_line) else {
            bail!("couldn't find Wordle header pattern".to_string());
        };

        let day_offset: u32 = captures["day_offset"]
            .replace(",", "")
            .parse()
            .context("couldn't convert day_offset to u32")?;
        let attempts: u32 = captures["attempts"]
            .parse()
            .context("couldn't convert attempts to u32")?;
        let solved = matches!(&captures["attempts"], "X");
        let hard_mode = captures.name("hard_mode").is_some();

        Ok(Puzzle {
            day_offset,
            attempts,
            solved,
            hard_mode,
            board: lines
                .map(String::from)
                .collect::<Vec<String>>()
                .join("\n")
                .try_into()
                .context("couldn't convert lines to puzzle board")?,
        })
    }
}
