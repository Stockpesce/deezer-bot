// id: 31355561

mod db;
mod deezer;
mod inline;

use std::sync::Arc;

use anyhow::Context;
use deezer::Deezer;
use sqlx::{pool::PoolOptions, Pool, Postgres};
use teloxide::{
    dispatching::{HandlerExt, UpdateFilterExt},
    prelude::Dispatcher,
    requests::Requester,
    types::{ChatId, Message, Update},
    utils::command::BotCommands,
    Bot,
};

use crate::db::queries;

pub struct Settings {
    buffer_channel: ChatId,
}

impl Settings {
    fn from_env() -> anyhow::Result<Self> {
        let buffer_channel =
            std::env::var("BUFFER_CHANNEL").context("missing BUFFER_CHANNEL env var")?;

        let buffer_channel: i64 = buffer_channel.parse().context("invalid BUFFER_CHANNEL")?;

        Ok(Self {
            buffer_channel: ChatId(buffer_channel),
        })
    }
}

#[derive(BotCommands, PartialEq, Debug, Clone)]
#[command(rename_rule = "lowercase", parse_with = "split")]
enum Commands {
    #[command(description = "Display your search history")]
    History,
}

async fn history_command(bot: Bot, msg: Message, pool: Pool<Postgres>) -> anyhow::Result<()> {
    use std::fmt::Write;

    let from = msg.from().context("command was not sent by a user")?;

    let history = queries::get_history(from.id.0 as i64, 10, &pool).await?;
    let text = String::from("Your last 10 searches:\n\n");
    let history_formatted = history
        .into_iter()
        .enumerate()
        .map(|(n, song)| (n + 1, song))
        .fold(text, |mut s, (n, song)| {
            writeln!(&mut s, "{n}) {} - {}", song.song_artist, song.song_name).ok();
            s
        });

    bot.send_message(msg.chat.id, history_formatted).await?;

    Ok(())
}

async fn unknown_command(bot: Bot, msg: Message) -> anyhow::Result<()> {
    // some groups might enable access to all messages
    // by mistake
    if msg.chat.is_private() {
        bot.send_message(msg.chat.id, "Uknown command!").await?;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();

    let db_url = std::env::var("DATABASE_URL").context("missing DB_URL")?;
    let pool = PoolOptions::<Postgres>::new()
        .max_connections(12)
        .min_connections(4)
        .connect(&db_url)
        .await
        .context("couldn't connect to the database")?;

    sqlx::migrate!().run(&pool).await?;

    let settings = Arc::new(Settings::from_env()?);

    let client = reqwest::Client::builder()
        // IPv4 only, ipv6 botapi isn't reachable from everywhere
        .local_address("0.0.0.0".parse().map(Some).unwrap())
        .build()
        .unwrap();
    let bot = Bot::from_env_with_client(client);

    let deezer = Arc::new(Deezer::default());

    let downloader = deezer_downloader::Downloader::new().await?;
    let downloader = Arc::new(downloader);

    let command_handler = dptree::entry()
        .filter_command::<Commands>()
        .branch(dptree::case![Commands::History].endpoint(history_command));

    let tree = dptree::entry()
        .branch(Update::filter_inline_query().endpoint(inline::inline_query))
        .branch(Update::filter_chosen_inline_result().endpoint(inline::chosen))
        .branch(
            Update::filter_message()
                .branch(command_handler)
                .endpoint(unknown_command),
        );

    bot.set_my_commands(Commands::bot_commands()).await?;

    Dispatcher::builder(bot, tree)
        .enable_ctrlc_handler()
        .dependencies(dptree::deps![deezer, downloader, settings, pool])
        .build()
        .dispatch()
        .await;

    todo!()
}
