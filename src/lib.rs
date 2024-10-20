use anyhow::{anyhow, Result};
use chrono::TimeZone;
use chrono_tz::America::New_York;
use derive_masked::DisplayMasked;
use hank_pdk::{http, info, plugin_fn, warn, FnResult, Hank, HttpRequest};
use hank_types::channel::{Channel, ChannelKind};
use hank_types::database::PreparedStatement;
use hank_types::message::Message;
use hank_types::plugin::{CommandContext, Metadata};
use hank_types::user::User;
use oxford_join::OxfordJoin;
use pluralizer::pluralize;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;
use wordle::Puzzle;

mod wordle;

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

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct PuzzleRow {
    id: u64,
    submitter: String,
    submitted_by: u64,
    submitted_at: chrono::DateTime<chrono::Local>,
    submitted_date: chrono::NaiveDate,
    puzzle: Puzzle,
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
    puzzle TEXT NOT NULL,
    UNIQUE(submitted_by, day_offset),
    UNIQUE(submitted_by, submitted_date)
);
";
    let _ = Hank::db_query(PreparedStatement::new(query).build());
}

#[derive(DisplayMasked, Deserialize)]
#[allow(dead_code)]
struct CurrentPuzzle {
    id: u32,
    days_since_launch: u32,
    print_date: chrono::NaiveDate,
    #[masked]
    solution: String,
    editor: String,
}

fn get_current_puzzle(reset: bool) -> &'static CurrentPuzzle {
    static CURRENT_PUZZLE: OnceLock<CurrentPuzzle> = OnceLock::new();

    fn get_current_puzzle_inner() -> CurrentPuzzle {
        let req = HttpRequest::new(format!(
            "https://www.nytimes.com/svc/wordle/v2/{}.json",
            now().date_naive(),
        ));
        let res = http::request::<String>(&req, None);
        res.unwrap().json::<CurrentPuzzle>().unwrap()
    }

    if reset {
        let _ = CURRENT_PUZZLE.set(get_current_puzzle_inner());
    }

    CURRENT_PUZZLE.get_or_init(get_current_puzzle_inner)
}

fn announce_daily_winner() {
    let Ok(winners) = find_todays_winners() else {
        return;
    };

    if winners.is_empty() {
        return;
    }

    let count = winners.len();
    let attempts = winners
        .first()
        .expect("there should be a first winner")
        .puzzle
        .attempts;
    let winners = winners
        .iter()
        .map(|w| format!("<@{}>", w.submitted_by))
        .collect::<Vec<_>>();

    let comments = HashMap::from([
        (1, "In only **1** attempt! Crazy!"),
        (2, "Wow, in only 2 attempts!"),
        (3, "3 attempts? Very nice."),
        (4, "Solved in only 4 attempts. Nice."),
        (5, "Solved in 5 attempts, phew."),
        (6, "6 attempts huh? Well, at least you solved it right?"),
    ]);

    let content = format!(
        "Congratulations to {} on being the top {} yesterday! {}",
        winners.oxford_and(),
        pluralize("Wordler", count as isize, false),
        comments.get(&attempts).expect("we should have a comment")
    );

    // @TODO how should the announcement channel get set? ideally it's not hardcoded.
    // do we just need a .wordle settings accouncement_channel #general
    // @note ideally i'd like to have a settings interface built in to hank
    // @note i wonder if bots know who owns them/invited them to the server? then on the daily
    // announcement, if there's no announcemnet_channel set, it can DM the owner to let them know
    Hank::send_message(Message {
        channel: Some(Channel {
            kind: ChannelKind::ChatRoom.into(),
            id: "1295918677127991298".to_string(),
            ..Default::default()
        }),
        content,
        ..Default::default()
    });
}

pub fn initialize() {
    info!("Initializing Wordle...");

    announce_daily_winner();
    // Cache the current days puzzle.
    let _ = get_current_puzzle(false);

    // Reload the cached puzzle daily.
    Hank::cron("0 0 * * *", || {
        let _ = get_current_puzzle(true);
    });

    Hank::cron("0 9 * * *", announce_daily_winner);
}

pub fn wordle_chat_commands(_context: CommandContext, _message: Message) {
    let statement =
        PreparedStatement::new("SELECT * FROM puzzle ORDER BY submitted_at DESC LIMIT ?")
            .values(["5"])
            .build();
    let puzzles = Hank::db_fetch::<PuzzleRow>(statement);

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
INSERT INTO puzzle (submitter, submitted_by, submitted_at, day_offset, attempts, solved, hard_mode, puzzle)
VALUES (?, ?, ?, ?, ?, ?, ?, ?)
";
    let statement = PreparedStatement::new(query)
        .values([
            user.name.clone(),
            user.id.to_string(),
            now().to_rfc3339(),
            puzzle.day_offset.to_string(),
            puzzle.attempts.to_string(),
            puzzle.solved.to_string(),
            puzzle.hard_mode.to_string(),
            puzzle.clone().into(),
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

fn find_puzzles() -> Result<Vec<PuzzleRow>> {
    let statement = PreparedStatement::new("SELECT * FROM puzzle").build();
    Ok(Hank::db_fetch::<PuzzleRow>(statement).map_err(|e| anyhow!(e))?)
}

fn find_todays_puzzles() -> Result<Vec<PuzzleRow>> {
    find_puzzles_by_date(&now().date_naive())
}

fn find_todays_winners() -> Result<Vec<PuzzleRow>> {
    find_puzzles_on_date_by_rank(&now().date_naive(), 1)
}

fn find_puzzles_on_date_by_rank(date: &chrono::NaiveDate, rank: u8) -> Result<Vec<PuzzleRow>> {
    let query = "
SELECT * 
FROM (SELECT *, RANK() OVER (ORDER BY attempts ASC) AS rank FROM puzzle WHERE submitted_date = date(?))
WHERE rank = CAST(? AS INTEGER)
ORDER BY submitted_at ASC
";
    let statement = PreparedStatement::new(query)
        .values([date.to_string(), rank.to_string()])
        .build();

    Ok(Hank::db_fetch::<PuzzleRow>(statement).map_err(|e| anyhow!(e))?)
}

fn find_puzzles_by_date(date: &chrono::NaiveDate) -> Result<Vec<PuzzleRow>> {
    let statement = PreparedStatement::new("SELECT * FROM puzzle WHERE submitted_date = date(?)")
        .values([date.to_string()])
        .build();

    Ok(Hank::db_fetch::<PuzzleRow>(statement).map_err(|e| anyhow!(e))?)
}

fn now() -> chrono::DateTime<chrono_tz::Tz> {
    New_York.from_utc_datetime(&chrono::Utc::now().naive_utc())
}
