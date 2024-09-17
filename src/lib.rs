use extism_pdk::{info, plugin_fn, FnResult, Prost};
use hank_pdk::Hank;
use hank_types::{database::PreparedStatement, message::Message, plugin::Metadata};

#[plugin_fn]
pub fn get_metadata() -> FnResult<Prost<Metadata>> {
    Ok(Prost(Metadata {
        name: "sample-rust-plugin".into(),
        description: "A sample plugin to demonstrate some functionality.".into(),
        version: "0.1.0".into(),
        database: true,
    }))
}

#[plugin_fn]
pub fn install() -> FnResult<()> {
    let stmt = PreparedStatement {
        sql: "CREATE TABLE IF NOT EXISTS people (name TEXT, age INTEGER)".into(),
        ..Default::default()
    };
    Hank::db_query(stmt);

    Ok(())
}

#[plugin_fn]
pub fn initialize() -> FnResult<()> {
    info!("initializing...");

    Ok(())
}

#[plugin_fn]
pub fn handle_message(Prost(message): Prost<Message>) -> FnResult<()> {
    info!("{}: {}", message.author_name, message.content);

    Ok(())
}

#[plugin_fn]
pub fn handle_command(Prost(message): Prost<Message>) -> FnResult<()> {
    if message.content == "ping" {
        let response = Message {
            content: "Pong!".into(),
            ..message
        };
        Hank::send_message(response);
    }

    let people = Hank::db_query(PreparedStatement {
        sql: "SELECT * from people".into(),
        ..Default::default()
    });
    info!("{:?}", people);

    Ok(())
}
