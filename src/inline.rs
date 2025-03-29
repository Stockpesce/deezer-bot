use std::sync::Arc;

use anyhow::Context;
use sqlx::{Pool, Postgres};
use teloxide::{
    payloads::{AnswerInlineQuerySetters, EditMessageMediaInlineSetters, SendAudioSetters},
    requests::Requester,
    types::*,
    Bot,
};

use crate::{
    callback::{self, CallbackData},
    db::{queries, CachedSong},
    deezer::{Deezer, DeezerDownloader, Song},
    encoding, Settings,
};

#[derive(serde::Serialize, serde::Deserialize)]
/// Wrapper structure that gets serialized in the result id of the
/// inline query results, so that [`chosen`] knows what song we are talking about
enum QueryData {
    // deezer id,
    Download(u64), // this is the deezer id, not the file id, neither it is the database songs(id)
    // cached song id
    Cached(i32), // this is songs(id), not file id
}

/// case when a song has not been registered yet in the database
/// and yet has to be cached. Replaces it with its http preview.
fn make_unregistered_query_result(result: &Song) -> InlineQueryResult {
    let id = encoding::encode(QueryData::Download(result.id)).unwrap();

    InlineQueryResultAudio::new(id, result.preview.parse().unwrap(), &result.title)
        .performer(&result.artist.name)
        .audio_duration(result.duration.to_string())
        .caption("The file is downloading... please wait.")
        .reply_markup(InlineKeyboardMarkup::new([[InlineKeyboardButton::new(
            "Loading...",
            InlineKeyboardButtonKind::CallbackData(
                encoding::encode(CallbackData::Nothing).unwrap(),
            ),
        )]]))
        .into()
}

fn make_song_reply_markup(song: &CachedSong) -> InlineKeyboardMarkup {
    let reply_markup_encoded = encoding::encode(CallbackData::Like { id: song.id }).unwrap();

    InlineKeyboardMarkup::new([[InlineKeyboardButton::new(
        "Like",
        InlineKeyboardButtonKind::CallbackData(reply_markup_encoded),
    )]])
}

/// case when a song is already cached
fn make_cached_query_result(registered: &CachedSong) -> InlineQueryResult {
    let result_encoded_id = encoding::encode(QueryData::Cached(registered.id)).unwrap();
    let keyboard = make_song_reply_markup(registered);

    InlineQueryResultCachedAudio::new(result_encoded_id, &registered.file_id)
        .reply_markup(keyboard)
        .into()
}

/// inline search, when the query is not empty
async fn search(
    bot: Bot,
    q: InlineQuery,
    deezer: Arc<Deezer>,
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    const RESULT_LIMIT: usize = 5;

    log::debug!("Searching '{}' on deezer", q.query);

    let search_result = deezer
        .search(&q.query, RESULT_LIMIT as u32)
        .await
        .context("failed search on deezer")?;

    let ids: Vec<u64> = search_result.iter().map(|result| result.id).collect();
    let cached_ids = queries::by_deezer_ids(&ids, &pool).await?;

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

/// inline search when the query is empty
async fn history(bot: Bot, q: InlineQuery, pool: Pool<Postgres>) -> anyhow::Result<()> {
    let UserId(id) = q.from.id;
    let history = queries::get_cached_history_no_repeat(id.try_into().unwrap(), 10, &pool).await?;

    log::debug!("Showing {} history results", history.len());

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
        // when query is empty show the user its history
        0 => history(bot, q, pool).await,
        3.. => search(bot, q, deezer, pool).await,
        _ => Ok(()),
    }
}

/// callback from telegram when the user has chosen an inline result.
///
/// it is called so that
pub async fn chosen(
    bot: Bot,
    result: ChosenInlineResult,
    downloader: DeezerDownloader,
    settings: Arc<Settings>,
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    // null character = no need to download
    // let data: QueryData = serde_json::from_str(&result.result_id)?;
    let data: QueryData = encoding::decode(&result.result_id)?;

    let track_id = match data {
        QueryData::Download(track_id) => track_id,
        QueryData::Cached(file_id) => {
            queries::push_history(file_id, result.from.id.0.try_into().unwrap(), &pool).await?;
            return Ok(());
        }
    };

    let message_id = result
        .inline_message_id
        .context("did not receive inline message id")?;

    let song = downloader
        .download(track_id)
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

    // cache song into database
    let cached_song = queries::insert_song(track_id, audio_file_id, &song.metadata, &pool).await?;
    log::debug!("Caching a new song: {cached_song:?}");

    let input_media = InputMediaAudio::new(InputFile::file_id(audio_file_id));
    let keyboard = make_song_reply_markup(&cached_song);

    // edit temporary message with media and keyboard
    bot.edit_message_media_inline(message_id, InputMedia::Audio(input_media))
        .reply_markup(keyboard)
        .await?;

    queries::push_history(cached_song.id, result.from.id.0.try_into().unwrap(), &pool).await?;

    Ok(())
}
