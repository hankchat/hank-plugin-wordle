use anyhow::{anyhow, Result};
use chrono::TimeZone;
use derive_masked::{DebugMasked, DisplayMasked};
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

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RankedPuzzleRow {
    #[serde(flatten)]
    row: PuzzleRow,
    rank: u32,
}

pub fn install() {
    let query = "
CREATE TABLE IF NOT EXISTS puzzle (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    submitter TEXT NOT NULL,
    submitted_by INTEGER NOT NULL,
    submitted_at TEXT NOT NULL,
    submitted_date TEXT NOT NULL,
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

// @TODO consider watching for messages that contain the solution and track who says the daily
// wordle word on the day
#[derive(DisplayMasked, DebugMasked, Deserialize, Default)]
#[allow(dead_code)]
struct CurrentPuzzle {
    id: u32,
    days_since_launch: u32,
    print_date: chrono::NaiveDate,
    #[masked]
    solution: String,
    editor: String,
}

impl CurrentPuzzle {
    pub fn from_calculated() -> Self {
        let today = Hank::datetime();
        let wordle_launch_day = today
            .offset()
            .with_ymd_and_hms(2021, 6, 19, 0, 0, 0)
            .unwrap();
        let days_since_launch = Hank::datetime()
            .signed_duration_since(wordle_launch_day)
            .num_days()
            .try_into()
            .expect("number of days should not exceed u32");
        let print_date = today.date_naive();
        CurrentPuzzle {
            days_since_launch,
            print_date,
            ..Default::default()
        }
    }
}

fn get_current_puzzle(reset: bool) -> &'static CurrentPuzzle {
    static CURRENT_PUZZLE: OnceLock<CurrentPuzzle> = OnceLock::new();

    fn get_current_puzzle_inner(retries: u8) -> Result<CurrentPuzzle> {
        let req = HttpRequest::new(format!(
            "https://www.nytimes.com/svc/wordle/v2/{}.json",
            Hank::datetime().date_naive(),
        ));
        match http::request::<String>(&req, None)?.json::<CurrentPuzzle>() {
            Ok(puzzle) => Ok(puzzle),
            Err(e) => {
                warn!(
                    "Error getting current puzzle, retrying {} more time(s): {}",
                    retries - 1,
                    e
                );
                if retries > 1 {
                    get_current_puzzle_inner(retries - 1)
                } else {
                    Err(e)
                }
            }
        }
    }

    if reset {
        match get_current_puzzle_inner(2) {
            Ok(puzzle) => {
                let _ = CURRENT_PUZZLE.set(puzzle);
            }
            Err(e) => {
                warn!("Failed to get current puzzle after 2 retries, begrudgingly returning old puzzle: {}", e);

                return if let Some(puzzle) = CURRENT_PUZZLE.get() {
                    puzzle
                } else {
                    warn!("Failed to get the old puzzle! There was never one set! Just gonna fake it :shrug:");
                    CURRENT_PUZZLE.get_or_init(CurrentPuzzle::from_calculated)
                };
            }
        }
    }

    let current = CURRENT_PUZZLE.get_or_init(|| match get_current_puzzle_inner(2) {
        Ok(puzzle) => puzzle,
        Err(e) => {
            warn!("Failed to init current puzzle after 2 retries, falling back to calculated puzzle: {}", e);
            CurrentPuzzle::from_calculated()
        }
    });

    let today = Hank::datetime().date_naive();
    if current.print_date != today {
        warn!(
            "Cached puzzle is out of date, refreshing... {} (current date: {})",
            current, today
        );
        get_current_puzzle(true)
    } else {
        current
    }
}

fn announce_yesterdays_winners() {
    let Ok(winners) = find_yesterdays_winners() else {
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
        "Congratulations to {} on being the top {} yesterday! <:limesDab:795850581725020250> {}",
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
            id: "664538126613741590".to_string(),
            ..Default::default()
        }),
        content,
        ..Default::default()
    });
}

pub fn initialize() {
    info!("Initializing...");

    // Cache the current days puzzle.
    let _ = get_current_puzzle(false);

    // Reload the cached puzzle daily.
    Hank::cron("0 0 0 * * *", || {
        let _ = get_current_puzzle(true);
    });

    Hank::cron("0 0 9 * * *", announce_yesterdays_winners);
}

pub fn wordle_chat_commands(_context: CommandContext, message: Message) {
    let leaderboard =
        find_puzzles_by_date_ordered_by_rank(&Hank::datetime().date_naive()).unwrap_or_default();
    if leaderboard.is_empty() {
        return;
    }

    let mut response = String::from("**Today's Top Wordlers**\n");
    for (i, entry) in leaderboard.iter().enumerate() {
        let dab = if entry.rank == 1 {
            "<:limesDab:795850581725020250>"
        } else {
            ""
        };
        response.push_str(&format!(
            "{}. {} - {}/6 {}\n",
            i, entry.row.submitter, entry.row.puzzle.attempts, dab
        ));
    }

    Hank::respond(response, message)
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

    match insert_puzzle(user, &puzzle) {
        Ok(_) => Hank::react("✅", message),
        Err(e) => {
            match e {
                InsertPuzzleError::UniqueConstraint(fields) => {
                    match *fields
                        .iter()
                        .map(AsRef::as_ref)
                        .collect::<Vec<_>>()
                        .as_slice()
                    {
                        ["submitted_by", "day_offset"] => info!(
                            "{} has already submitted a puzzle for Wordle #{}",
                            user.name, puzzle.day_offset
                        ),
                        ["submitted_by", "submitted_date"] => {
                            info!("{} has already submitted a puzzle for today", user.name)
                        }
                        _ => warn!("unhandled unique constraint encountered: {:?}", fields),
                    }
                }
                InsertPuzzleError::PuzzleConersion(e) => {
                    warn!("there was a problem converting a puzzle on insert {}", e)
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
    PuzzleConersion(String),
}

fn insert_puzzle(user: &User, puzzle: &Puzzle) -> Result<(), InsertPuzzleError> {
    let now = Hank::datetime();
    let query = "
INSERT INTO puzzle (submitter, submitted_by, submitted_at, submitted_date, day_offset, attempts, solved, hard_mode, puzzle)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
";
    let statement = PreparedStatement::new(query)
        .values([
            user.name.clone(),
            user.id.to_string(),
            now.to_rfc3339(),
            now.date_naive().to_string(),
            puzzle.day_offset.to_string(),
            puzzle.attempts.to_string(),
            puzzle.solved.to_string(),
            puzzle.hard_mode.to_string(),
            puzzle
                .clone()
                .try_into()
                .map_err(|e: anyhow::Error| InsertPuzzleError::PuzzleConersion(e.to_string()))?,
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
    Hank::db_fetch::<PuzzleRow>(statement).map_err(|e| anyhow!(e))
}

fn find_todays_puzzles() -> Result<Vec<PuzzleRow>> {
    find_puzzles_by_date(&Hank::datetime().date_naive())
}

fn find_todays_winners() -> Result<Vec<PuzzleRow>> {
    find_puzzles_by_date_and_rank(&Hank::datetime().date_naive(), 1)
}

fn find_yesterdays_winners() -> Result<Vec<PuzzleRow>> {
    let yesterday = Hank::datetime() - chrono::Duration::days(1);
    find_puzzles_by_date_and_rank(&yesterday.date_naive(), 1)
}

fn find_puzzles_by_date_and_rank(date: &chrono::NaiveDate, rank: u8) -> Result<Vec<PuzzleRow>> {
    let query = "
SELECT * 
FROM (SELECT *, RANK() OVER (ORDER BY attempts ASC) AS rank FROM puzzle WHERE submitted_date = ?)
WHERE rank = CAST(? AS INTEGER)
ORDER BY submitted_at ASC
";
    let statement = PreparedStatement::new(query)
        .values([date.to_string(), rank.to_string()])
        .build();

    Hank::db_fetch::<PuzzleRow>(statement).map_err(|e| anyhow!(e))
}

fn find_puzzles_by_date_ordered_by_rank(date: &chrono::NaiveDate) -> Result<Vec<RankedPuzzleRow>> {
    let query = "
SELECT * 
FROM (SELECT *, RANK() OVER (ORDER BY attempts ASC) AS rank FROM puzzle WHERE submitted_date = ?)
ORDER BY rank, submitted_at ASC
";
    let statement = PreparedStatement::new(query)
        .values([date.to_string()])
        .build();

    Hank::db_fetch::<RankedPuzzleRow>(statement).map_err(|e| anyhow!(e))
}

fn find_puzzles_by_date(date: &chrono::NaiveDate) -> Result<Vec<PuzzleRow>> {
    let statement = PreparedStatement::new("SELECT * FROM puzzle WHERE submitted_date = ?")
        .values([date.to_string()])
        .build();

    Hank::db_fetch::<PuzzleRow>(statement).map_err(|e| anyhow!(e))
}
