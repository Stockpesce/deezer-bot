use std::sync::Arc;

use anyhow::Context;
use deezer_downloader::Downloader;
use sqlx::{Pool, Postgres};
use teloxide::{
    payloads::{AnswerInlineQuerySetters, SendAudioSetters},
    requests::Requester,
    types::*,
    Bot,
};

use crate::{
    db::{CachedSong, HistoryRecord},
    deezer::{Deezer, Song},
    Settings,
};

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

    log::info!("Searching '{}' on deezer", q.query);

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
    let history = HistoryRecord::get_history_no_repeat(id.try_into().unwrap(), 10, &pool).await?;

    log::info!("Showing {} history results", history.len());

    let results = history.iter().map(make_cached_query_result);
    bot.answer_inline_query(q.id, results).cache_time(0).await?;

    Ok(())
}

pub async fn inline_query(
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

pub async fn chosen(
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

    let song = downloader
        .download_song(track_id)
        .await
        .context("downloading the file from deezer")?;

    let cover = reqwest::get(&song.metadata.album.cover_medium)
        .await?
        .bytes()
        .await?;
    let cover = InputFile::memory(cover);

    let audio = bot
        .send_audio(settings.buffer_channel, InputFile::memory(song.content))
        .performer(&song.metadata.artist.name)
        .thumb(cover)
        .title(&song.metadata.title)
        .await?;

    let audio = audio.audio().context("just sent an audio")?;
    let audio_file_id = &audio.file.id;

    let input_media = InputMediaAudio::new(InputFile::file_id(audio_file_id));
    bot.edit_message_media_inline(message_id, InputMedia::Audio(input_media))
        .await?;

    let cached_song =
        CachedSong::insert_song(track_id, audio_file_id, &song.metadata, &pool).await?;

    log::info!("Caching a new song: {cached_song:?}");

    HistoryRecord::push_history(cached_song.id, result.from.id.0.try_into().unwrap(), &pool)
        .await?;

    Ok(())
}
