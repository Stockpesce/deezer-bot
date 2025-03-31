use teloxide::types::{
    InlineKeyboardButton, InlineKeyboardButtonKind, InlineKeyboardMarkup, InlineQueryResult,
    InlineQueryResultAudio, InlineQueryResultCachedAudio,
};

use crate::{callback::CallbackData, db::CachedSong, deezer::Song, encoding};

#[derive(serde::Serialize, serde::Deserialize)]
/// Wrapper structure that gets serialized in the result id of the
/// inline query results, so that [`chosen`] knows what song we are talking about
pub enum QueryData {
    // deezer id,
    Download(u64), // this is the deezer id, not the file id, neither it is the database songs(id)
    // cached song id
    Cached(i32), // this is songs(id), not file id
}

/// case when a song has not been registered yet in the database
/// and yet has to be cached. Replaces it with its http preview.
pub fn make_unregistered_query_result(result: &Song) -> InlineQueryResult {
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

pub fn make_song_reply_markup(song_id: i32, likes: i64) -> InlineKeyboardMarkup {
    let reply_markup_encoded = encoding::encode(CallbackData::Like { song_id }).unwrap();

    InlineKeyboardMarkup::new([[InlineKeyboardButton::new(
        format!("Like ({})", likes),
        InlineKeyboardButtonKind::CallbackData(reply_markup_encoded),
    )]])
}

/// case when a song is already cached
pub fn make_cached_query_result(registered: &CachedSong) -> InlineQueryResult {
    let result_encoded_id = encoding::encode(QueryData::Cached(registered.id)).unwrap();

    let loading = encoding::encode(CallbackData::Nothing).unwrap();
    let keyboard = InlineKeyboardMarkup::new([[InlineKeyboardButton::new(
        "Loading...",
        InlineKeyboardButtonKind::CallbackData(loading),
    )]]);

    InlineQueryResultCachedAudio::new(result_encoded_id, &registered.file_id)
        .reply_markup(keyboard)
        .into()
}
