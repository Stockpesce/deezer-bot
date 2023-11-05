// id: 31355561

mod db;
mod deezer;
mod inline;
mod telemetry;

use std::{convert::Infallible, sync::Arc};

use anyhow::Context;
use deezer::Deezer;
use hyper::{
    service::{make_service_fn, service_fn},
    Server,
};
use prometheus::{Registry, TextEncoder};
use sqlx::{pool::PoolOptions, Pool, Postgres};
use teloxide::{
    dispatching::{HandlerExt, UpdateFilterExt},
    payloads::SendMessageSetters,
    prelude::Dispatcher,
    requests::Requester,
    types::{ChatId, InlineKeyboardButton, Message, ParseMode, ReplyMarkup, Update},
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

    #[command(description = "Show start message")]
    Start,
}

impl std::fmt::Display for Commands {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::History => write!(f, "history"),
            Self::Start => write!(f, "start"),
        }
    }
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

async fn start_command(bot: Bot, message: Message) -> anyhow::Result<()> {
    let button = InlineKeyboardButton::new(
        "Search a song",
        teloxide::types::InlineKeyboardButtonKind::SwitchInlineQueryCurrentChat("".into()),
    );
    let keyboard_markup = ReplyMarkup::inline_kb([[button]]);

    bot.send_message(
        message.chat.id,
        "Hi, song searching is only available inline.\nStart searching by clicking the button below",
    )
    .reply_markup(keyboard_markup)
    .parse_mode(ParseMode::Html)
    .await?;

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

fn setup_bot() -> Bot {
    let client = reqwest::Client::builder()
        // IPv4 only, ipv6 botapi isn't reachable from everywhere
        .local_address("0.0.0.0".parse().map(Some).unwrap())
        .build()
        .unwrap();
    Bot::from_env_with_client(client)
}

async fn prometheus_serve(
    registry: Registry,
) -> Result<hyper::Response<hyper::Body>, hyper::Error> {
    let encoder = TextEncoder::new();
    let encoded = encoder.encode_to_string(&registry.gather()).unwrap();

    let response = hyper::Response::builder()
        .status(200)
        .body(hyper::Body::from(encoded))
        .unwrap();

    Ok(response)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();

    let registry = Registry::new();

    let telemetry = telemetry::setup_telemetry(registry.clone())?;

    let service = make_service_fn(move |_| {
        let registry = registry.clone();
        async move { Ok::<_, Infallible>(service_fn(move |_| prometheus_serve(registry.clone()))) }
    });

    let server = Server::bind(&"0.0.0.0:8080".parse().unwrap()).serve(service);
    tokio::spawn(server);

    // database setup
    let db_url = std::env::var("DATABASE_URL").context("missing DB_URL")?;
    let pool = PoolOptions::<Postgres>::new()
        .max_connections(12)
        .min_connections(4)
        .connect(&db_url)
        .await
        .context("couldn't connect to the database")?;

    sqlx::migrate!().run(&pool).await?;

    // read settings from env
    let settings = Arc::new(Settings::from_env()?);

    // deezer setup
    let deezer = Arc::new(Deezer::default());
    let downloader = deezer_downloader::Downloader::new().await?;
    let downloader = Arc::new(downloader);

    // bot setup
    let bot = setup_bot();

    let command_tree = dptree::entry()
        .filter_command::<Commands>()
        .inspect(telemetry::command_telemetry::<Commands>(&telemetry))
        .branch(dptree::case![Commands::History].endpoint(history_command))
        .branch(dptree::case![Commands::Start].endpoint(start_command));

    let tree = dptree::entry()
        .inspect(telemetry::update_telemetry(&telemetry))
        .branch(
            Update::filter_inline_query()
                .inspect(telemetry::inline_telemetry(&telemetry))
                .endpoint(inline::inline_query),
        )
        .branch(Update::filter_chosen_inline_result().endpoint(inline::chosen))
        .branch(
            Update::filter_message()
                .branch(command_tree)
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
