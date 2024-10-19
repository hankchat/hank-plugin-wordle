use hank_pdk::{info, plugin_fn, FnResult, Hank};
use hank_types::database::PreparedStatement;
use hank_types::message::{Message, Reaction};
use hank_types::plugin::{CommandContext, Metadata};
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
    submitted_by INTEGER NOT NULL,
    submitted_at TEXT NOT NULL,
    day_offset INTEGER NOT NULL,
    attempts INTEGER NOT NULL,
    solved INTEGER NOT NULL,
    hard_mode INTEGER NOT NULL,
    board TEXT NOT NULL
)
";
    Hank::db_query(PreparedStatement {
        sql: query.into(),
        ..Default::default()
    });
}

pub fn initialize() {
    info!("Initializing Wordle...");
}

pub fn wordle_chat_commands(_context: CommandContext, _message: Message) {
    let statement = PreparedStatement {
        sql: "SELECT * FROM puzzle ORDER BY submitted_at DESC LIMIT ?".into(),
        values: vec!["5".into()],
    };
    let puzzles = Hank::db_fetch::<Puzzle>(statement);

    info!("{:?}", puzzles);
}

pub fn handle_message(message: Message) {
    // Record puzzles.
    if let Ok(puzzle) = Puzzle::try_from(message.content.clone()) {
        insert_puzzle(
            &message.author.clone().expect("message author expected").id,
            puzzle,
        );
        Hank::react(Reaction {
            message: Some(message),
            emoji: "✅".into(),
        });
    };
}

fn insert_puzzle(user_id: &str, puzzle: Puzzle) {
    let statement = PreparedStatement {
        sql: "
INSERT INTO puzzle (submitted_by, submitted_at, day_offset, attempts, solved, hard_mode, board)
    VALUES (?, ?, ?, ?, ?, ?, ?)
"
        .to_string(),
        values: vec![
            user_id.to_string(),
            chrono::offset::Utc::now().to_string(),
            puzzle.day_offset.to_string(),
            puzzle.attempts.to_string(),
            puzzle.solved.to_string(),
            puzzle.hard_mode.to_string(),
            puzzle.board.into(),
        ],
    };

    Hank::db_query(statement);
}
