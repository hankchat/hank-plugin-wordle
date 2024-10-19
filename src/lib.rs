use anyhow::Result;
use derive_masked::DisplayMasked;
use hank_pdk::{http, info, plugin_fn, warn, FnResult, Hank, HttpRequest};
use hank_types::channel::ChannelKind;
use hank_types::database::PreparedStatement;
use hank_types::message::Message;
use hank_types::plugin::{CommandContext, Metadata};
use hank_types::user::User;
use serde::Deserialize;
use std::sync::OnceLock;
use wordle::Puzzle;

mod wordle;

// @TODO i should probably use EST instead of UTC since NYT is EST

#[plugin_fn]
pub fn plugin() -> FnResult<()> {
    let mut hank = Hank::new(
        Metadata::new(
            "wordle",
            "jackyyll",
            "A wordle plugin to record daily Wordle puzzles",
            "0.1.0",
        )
        .allowed_hosts(vec!["www.nytimes.com"])
        .handles_commands(true)
        .build(),
    );

    hank.register_install_handler(install);
    hank.register_initialize_handler(initialize);
    hank.register_chat_message_handler(handle_message);
    hank.register_chat_command_handler(wordle_chat_commands);

    hank.start()
}

pub fn install() {
    let query = "
CREATE TABLE IF NOT EXISTS puzzle (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    submitter TEXT NOT NULL,
    submitted_by INTEGER NOT NULL,
    submitted_at TEXT NOT NULL,
    submitted_date TEXT GENERATED ALWAYS AS (date(submitted_at)),
    day_offset INTEGER NOT NULL,
    attempts INTEGER NOT NULL,
    solved INTEGER NOT NULL,
    hard_mode INTEGER NOT NULL,
    board TEXT NOT NULL,
    UNIQUE(submitted_by, day_offset),
    UNIQUE(submitted_by, submitted_date)
);
";
    let _ = Hank::db_query(PreparedStatement::new(query).build());
}

#[derive(DisplayMasked, Deserialize)]
#[allow(dead_code)]
struct CurrentPuzzle {
    id: i32,
    days_since_launch: i32,
    print_date: String,
    #[masked]
    solution: String,
    editor: String,
}

fn get_current_puzzle(reset: bool) -> &'static CurrentPuzzle {
    static CURRENT_PUZZLE: OnceLock<CurrentPuzzle> = OnceLock::new();

    fn get_current_puzzle_inner() -> CurrentPuzzle {
        let req = HttpRequest::new(format!(
            "https://www.nytimes.com/svc/wordle/v2/{}.json",
            chrono::Utc::now().date_naive(),
        ));
        let res = http::request::<String>(&req, None);
        res.unwrap().json::<CurrentPuzzle>().unwrap()
    }

    if reset {
        let _ = CURRENT_PUZZLE.set(get_current_puzzle_inner());
    }

    CURRENT_PUZZLE.get_or_init(|| get_current_puzzle_inner())
}

pub fn initialize() {
    info!("Initializing Wordle...");

    // Cache the current days puzzle.
    let _ = get_current_puzzle(false);

    // Reload the cached puzzle daily.
    Hank::cron("0 0 * * *", || {
        let _ = get_current_puzzle(true);
    });
}

pub fn wordle_chat_commands(_context: CommandContext, _message: Message) {
    let statement =
        PreparedStatement::new("SELECT * FROM puzzle ORDER BY submitted_at DESC LIMIT ?")
            .values(["5"])
            .build();
    let puzzles = Hank::db_fetch::<Puzzle>(statement);

    info!("{:?}", puzzles);
}

pub fn handle_message(message: Message) {
    let Some(ref channel) = message.channel else {
        return;
    };

    // @TODO consider adding a flag in metadata that tells hank if this plugin should handle direct
    // messages or not.
    if channel.kind() != ChannelKind::ChatRoom {
        return;
    }

    // Record puzzles.
    let Ok(puzzle) = Puzzle::try_from(message.content.clone()) else {
        return;
    };

    let Some(ref user) = message.author else {
        return;
    };

    if puzzle.day_offset != get_current_puzzle(false).days_since_launch {
        let emojis = vec!["❌", "📅"];
        for emoji in emojis {
            Hank::react(emoji, message.clone());
        }
        return;
    }

    match insert_puzzle(&user, &puzzle) {
        Ok(_) => Hank::react("✅", message),
        Err(e) => {
            match e {
                InsertPuzzleError::UniqueConstraint(fields) => {
                    match fields
                        .iter()
                        .map(AsRef::as_ref)
                        .collect::<Vec<_>>()
                        .as_slice()
                    {
                        &["submitted_by", "day_offset"] => info!(
                            "{} has already submitted a puzzle for Wordle #{}",
                            user.name, puzzle.day_offset
                        ),
                        &["submitted_by", "submitted_date"] => {
                            info!("{} has already submitted a puzzle for today", user.name)
                        }
                        _ => warn!("unhandled unique constraint encountered: {:?}", fields),
                    }
                }
                InsertPuzzleError::UnknownError(e) => {
                    warn!("unhandled error encountered: {}", e)
                }
            }

            Hank::react("❌", message);
        }
    }
}

enum InsertPuzzleError {
    UnknownError(String),
    UniqueConstraint(Vec<String>),
}

fn insert_puzzle(user: &User, puzzle: &Puzzle) -> Result<(), InsertPuzzleError> {
    let query = "
INSERT INTO puzzle (submitter, submitted_by, submitted_at, day_offset, attempts, solved, hard_mode, board)
VALUES (?, ?, ?, ?, ?, ?, ?, ?)
";
    let statement = PreparedStatement::new(query)
        .values([
            user.name.clone(),
            user.id.to_string(),
            chrono::Utc::now().to_rfc3339(),
            puzzle.day_offset.to_string(),
            puzzle.attempts.to_string(),
            puzzle.solved.to_string(),
            puzzle.hard_mode.to_string(),
            puzzle.board.clone().into(),
        ])
        .build();

    let res = Hank::db_query(statement);
    if res.is_ok() {
        return Ok(());
    }

    let error = res.unwrap_err();
    if let Some(fields) =
        error.strip_prefix("error returned from database: (code: 2067) UNIQUE constraint failed: ")
    {
        let fields = fields
            .split(", ")
            .map(|f| f.replace("puzzle.", ""))
            .collect::<Vec<_>>();
        Err(InsertPuzzleError::UniqueConstraint(fields))
    } else {
        Err(InsertPuzzleError::UnknownError(error))
    }
}
