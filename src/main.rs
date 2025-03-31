// id: 31355561

mod callback;
mod commands;
mod db;
mod deezer;
mod encoding;
mod inline;
mod markup;
mod telemetry;

use std::sync::Arc;

use anyhow::Context;
use callback::CallbackData;
use commands::Commands;
use deezer::Deezer;
use deezer_downloader::downloader::DownloaderBuilder;
use log::LevelFilter;
use prometheus::Registry;
use sqlx::{pool::PoolOptions, Postgres};
use teloxide::{
    dispatching::{HandlerExt, UpdateFilterExt},
    prelude::Dispatcher,
    requests::Requester,
    types::{ChatId, Update},
    utils::command::BotCommands,
    Bot,
};

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

fn setup_bot() -> Bot {
    // IPv4 only, ipv6 botapi isn't reachable from everywhere
    let client = reqwest::Client::builder()
        .local_address("0.0.0.0".parse().map(Some).unwrap())
        .build()
        .unwrap();

    Bot::from_env_with_client(client)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    let default_level = cfg!(debug_assertions)
        .then_some(LevelFilter::Info)
        .unwrap_or(LevelFilter::Warn);

    env_logger::builder()
        .filter_level(default_level)
        .parse_default_env()
        .init();

    let db_url = std::env::var("DATABASE_URL").context("missing DATABASE_URL")?;
    let arl_cookie = std::env::var("ARL_COOKIE").context("missing arl cookie")?;

    let pool = PoolOptions::<Postgres>::new()
        .max_connections(12)
        .min_connections(4)
        .connect(&db_url)
        .await
        .context("couldn't connect to the database")?;

    sqlx::migrate!().run(&pool).await?;

    let registry = Registry::new();
    let telemetry = telemetry::setup_telemetry(registry.clone())?;
    telemetry::listen_prometheus_server(([0, 0, 0, 0], 8080), registry);

    // read settings from env
    let settings = Arc::new(Settings::from_env()?);

    // deezer setup
    let deezer_api = Arc::new(Deezer::default());
    let downloader = DownloaderBuilder::new()
        .arl_cookie(arl_cookie)
        .build()
        .await?;

    // my wrapper that tracks time
    // between token refreshes
    let downloader = deezer::DeezerDownloader::new(downloader);

    // bot setup
    let bot = setup_bot();

    let command_tree = dptree::entry()
        .filter_command::<Commands>()
        .inspect(telemetry::command_telemetry::<Commands>(&telemetry))
        .branch(dptree::case![Commands::History].endpoint(commands::history_command))
        .branch(dptree::case![Commands::Start].endpoint(commands::start_command));

    let tree = dptree::entry()
        .inspect(telemetry::update_telemetry(&telemetry))
        .branch(
            Update::filter_inline_query()
                .inspect(telemetry::inline_telemetry(&telemetry))
                .endpoint(inline::inline_query),
        )
        .branch(
            Update::filter_callback_query()
                .filter_map(callback::callback_decoder::<CallbackData>())
                .endpoint(callback::handle_callback),
        )
        .branch(Update::filter_chosen_inline_result().endpoint(inline::chosen))
        .branch(
            Update::filter_message()
                .branch(command_tree)
                .endpoint(commands::unknown_command),
        );

    // spawn ctrl-c handler
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        std::process::exit(0);
    });

    bot.set_my_commands(Commands::bot_commands()).await?;

    log::info!("Up and running!");
    Dispatcher::builder(bot, tree)
        // .enable_ctrlc_handler()
        .dependencies(dptree::deps![deezer_api, downloader, settings, pool])
        .build()
        .dispatch()
        .await;

    Ok(())
}
