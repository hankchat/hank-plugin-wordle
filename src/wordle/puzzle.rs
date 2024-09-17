use crate::wordle::PuzzleBoard;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Puzzle {
    pub day_offset: i32,
    pub attempts: i32,
    #[serde(deserialize_with = "deserialize_bool")]
    pub solved: bool,
    #[serde(deserialize_with = "deserialize_bool")]
    pub hard_mode: bool,
    pub board: PuzzleBoard,
}

impl Puzzle {
    pub fn new(puzzle: String) -> Self {
        serde_json::from_str(&puzzle).unwrap()
    }
}

impl From<Puzzle> for String {
    fn from(puzzle: Puzzle) -> Self {
        let mut string = String::from("Wordle ");

        let day_offset = puzzle
            .day_offset
            .to_string()
            .as_bytes()
            .rchunks(3)
            .rev()
            .map(std::str::from_utf8)
            .collect::<Result<Vec<&str>, _>>()
            .unwrap()
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

        string
    }
}

impl TryFrom<String> for Puzzle {
    type Error = ();

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let mut lines = value.lines();
        let first_line = lines.next().unwrap_or("");

        let re =
            Regex::new(r"Wordle (?<day_offset>\d+,\d+) (?<attempts>(\d|X))\/6(?<hard_mode>\*)?")
                .unwrap();
        let Some(captures) = re.captures(first_line) else {
            return Err(());
        };

        let day_offset: i32 = captures["day_offset"].replace(",", "").parse().unwrap();
        let attempts: i32 = captures["attempts"].parse().unwrap_or(6);
        let solved = match &captures["attempts"] {
            "X" => false,
            _ => true,
        };
        let hard_mode = captures.name("hard_mode").is_some();

        Ok(Puzzle {
            day_offset,
            attempts,
            solved,
            hard_mode,
            board: lines
                .map(|l| String::from(l))
                .collect::<Vec<String>>()
                .join("\n")
                .into(),
        })
    }
}

fn deserialize_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde_json::Value;

    Ok(match serde::de::Deserialize::deserialize(deserializer)? {
        Value::Bool(b) => b,
        Value::String(s) => s == "true",
        Value::Number(n) => n.as_u64().unwrap_or(0) == 1,
        Value::Null => false,
        other => {
            return Err(serde::de::Error::unknown_variant(
                &format!("{}", other),
                &["0", "1", "true", "false", "'true'", "'false'"],
            ))
        }
    })
}
