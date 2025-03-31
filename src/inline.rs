use std::sync::Arc;

use anyhow::Context;
use sqlx::{Pool, Postgres};
use teloxide::{
    payloads::{
        AnswerInlineQuerySetters, EditMessageMediaInlineSetters,
        EditMessageReplyMarkupInlineSetters, SendAudioSetters,
    },
    requests::Requester,
    types::*,
    Bot,
};

use crate::{
    db::queries,
    deezer::{Deezer, DeezerDownloader},
    encoding,
    markup::{self, QueryData},
    Settings,
};

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
    let cached_songs = queries::by_deezer_ids(&ids, &pool).await?;

    let cached_iter = cached_songs.iter().map(markup::make_cached_query_result);
    let virgin_iter = search_result
        .iter()
        .filter(|res| {
            // filter registered songs out
            !cached_songs
                .iter()
                .any(|song| song.deezer_id as u64 == res.id)
        })
        .map(markup::make_unregistered_query_result);

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

    let results = history.iter().map(markup::make_cached_query_result);
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

    let message_id = result
        .inline_message_id
        .context("did not receive inline message id")?;

    // TODO:
    // invert logic. Exit logic can be unified. Download logic can be nested OR separated
    // into an appropriate function
    //
    let track_id = match data {
        QueryData::Download(track_id) => track_id,
        QueryData::Cached(id) => {
            queries::push_history(id, result.from.id.0.try_into().unwrap(), &pool).await?;

            let likes = queries::song_likes(id, &pool).await?;
            let keyboard = markup::make_song_reply_markup(id, likes);

            bot.edit_message_reply_markup_inline(message_id)
                .reply_markup(keyboard)
                .await?;

            // result was cached and there's anything other to do
            return Ok(());
        }
    };

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
    let keyboard = markup::make_song_reply_markup(cached_song.id, 0);

    // edit temporary message with media and keyboard
    bot.edit_message_media_inline(message_id, InputMedia::Audio(input_media))
        .reply_markup(keyboard)
        .await?;

    queries::push_history(cached_song.id, result.from.id.0.try_into().unwrap(), &pool).await?;

    Ok(())
}
