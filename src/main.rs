// id: 31355561

mod db;
mod deezer;

use std::sync::Arc;

use anyhow::Context;
use db::HistoryRecord;
use deezer::{Deezer, Song};
use deezer_downloader::Downloader;
use sqlx::{pool::PoolOptions, Pool, Postgres};
use teloxide::{
    dispatching::UpdateFilterExt,
    payloads::{AnswerInlineQuerySetters, SendAudioSetters},
    prelude::Dispatcher,
    requests::Requester,
    types::{
        ChatId, ChosenInlineResult, InlineKeyboardButton, InlineKeyboardButtonKind,
        InlineKeyboardMarkup, InlineQuery, InlineQueryResult, InlineQueryResultAudio,
        InlineQueryResultCachedAudio, InputFile, InputMedia, InputMediaAudio, Update, UserId,
    },
    Bot,
};

use crate::db::CachedSong;

#[derive(serde::Serialize, serde::Deserialize)]
enum QueryData {
    // deezer id,
    Download(u64),
    // cached song id
    Cached(i32),
}

fn make_unregistered_query_result(result: &Song) -> InlineQueryResult {
    let id = serde_json::to_string(&QueryData::Download(result.id)).unwrap();

    InlineQueryResultAudio::new(id, result.preview.parse().unwrap(), &result.title)
        .performer(&result.artist.name)
        .audio_duration(result.duration.to_string())
        .caption("The file is downloading... please wait.")
        .reply_markup(InlineKeyboardMarkup::new([[InlineKeyboardButton::new(
            "Loading...",
            InlineKeyboardButtonKind::CallbackData("callback".to_string()),
        )]]))
        .into()
}

fn make_cached_query_result(registered: &CachedSong) -> InlineQueryResult {
    let id = serde_json::to_string(&QueryData::Cached(registered.id)).unwrap();
    InlineQueryResultCachedAudio::new(id, &registered.file_id).into()
}

async fn search(
    bot: Bot,
    q: InlineQuery,
    deezer: Arc<Deezer>,
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    const RESULT_LIMIT: usize = 5;

    let search_result = deezer
        .search(&q.query, RESULT_LIMIT as u32)
        .await
        .context("failed search on deezer")?;

    let ids: Vec<u64> = search_result.iter().map(|result| result.id).collect();
    let cached_ids = CachedSong::by_deezer_ids(&ids, &pool).await?;

    let cached_iter = cached_ids.iter().map(make_cached_query_result);
    let virgin_iter = search_result
        .iter()
        .filter(|res| {
            // filter registered songs out
            !cached_ids
                .iter()
                .any(|song| song.deezer_id as u64 == res.id)
        })
        .map(make_unregistered_query_result);

    let results = cached_iter.chain(virgin_iter);

    bot.answer_inline_query(&q.id, results)
        .cache_time(0)
        .await?;

    Ok(())
}

async fn history(bot: Bot, q: InlineQuery, pool: Pool<Postgres>) -> anyhow::Result<()> {
    let UserId(id) = q.from.id;
    let history = HistoryRecord::get_history(id.try_into().unwrap(), 10, &pool).await?;
    let results = history.iter().map(make_cached_query_result);

    let results: Vec<_> = results.collect();
    println!("{results:?}");

    bot.answer_inline_query(q.id, results).cache_time(0).await?;

    Ok(())
}

async fn inline_query(
    bot: Bot,
    q: InlineQuery,
    deezer: Arc<Deezer>,
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    match q.query.len() {
        0 => history(bot, q, pool).await,
        3.. => search(bot, q, deezer, pool).await,
        _ => Ok(()),
    }
}

async fn chosen(
    bot: Bot,
    result: ChosenInlineResult,
    downloader: Arc<Downloader>,
    settings: Arc<Settings>,
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    // null character = no need to download
    let data: QueryData = serde_json::from_str(&result.result_id)?;

    let track_id = match data {
        QueryData::Download(track_id) => track_id,
        QueryData::Cached(file_id) => {
            HistoryRecord::push_history(file_id, result.from.id.0.try_into().unwrap(), &pool)
                .await?;

            return Ok(());
        }
    };

    let message_id = result
        .inline_message_id
        .context("did not receive inline message id")?;

    let song = downloader.download_song(track_id).await?;

    let cover = reqwest::get(&song.metadata.album.cover_medium)
        .await?
        .bytes()
        .await?;
    let cover = InputFile::memory(cover);

    let audio = bot
        .send_audio(settings.buffer_channel, InputFile::memory(song.content))
        .performer(song.metadata.artist.name)
        .thumb(cover)
        .title(song.metadata.title)
        .await?;

    let audio = audio.audio().context("just sent an audio")?;
    let audio_file_id = &audio.file.id;

    let input_media = InputMediaAudio::new(InputFile::file_id(audio_file_id));
    bot.edit_message_media_inline(message_id, InputMedia::Audio(input_media))
        .await?;

    let cached_song = CachedSong::insert_song(track_id, audio_file_id, &pool).await?;
    HistoryRecord::push_history(cached_song.id, result.from.id.0.try_into().unwrap(), &pool)
        .await?;

    Ok(())
}

struct Settings {
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

    let bot = Bot::from_env();
    let deezer = Arc::new(Deezer::default());

    let downloader = deezer_downloader::Downloader::new().await?;
    let downloader = Arc::new(downloader);

    let tree = dptree::entry()
        .branch(Update::filter_inline_query().endpoint(inline_query))
        .branch(Update::filter_chosen_inline_result().endpoint(chosen));

    Dispatcher::builder(bot, tree)
        .enable_ctrlc_handler()
        .dependencies(dptree::deps![deezer, downloader, settings, pool])
        .build()
        .dispatch()
        .await;

    todo!()
}
