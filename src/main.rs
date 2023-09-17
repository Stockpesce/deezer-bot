// id: 31355561

mod deezer;

use std::sync::Arc;

use anyhow::Context;
use deezer::{Deezer, Song};
use deezer_downloader::Downloader;
use teloxide::{
    dispatching::UpdateFilterExt,
    payloads::{AnswerInlineQuerySetters, SendAudioSetters},
    prelude::Dispatcher,
    requests::Requester,
    types::{
        ChatId, ChosenInlineResult, InlineKeyboardButton, InlineKeyboardButtonKind,
        InlineKeyboardMarkup, InlineQuery, InlineQueryResult, InlineQueryResultAudio, InputFile,
        InputMedia, InputMediaAudio, Update,
    },
    Bot,
};

fn make_query_result(result: &Song) -> InlineQueryResult {
    InlineQueryResultAudio::new(
        result.id.to_string(),
        result.preview.parse().unwrap(),
        &result.title,
    )
    .performer(&result.artist.name)
    .audio_duration(result.duration.to_string())
    .caption("The file is downloading... please wait.")
    .reply_markup(InlineKeyboardMarkup::new([[InlineKeyboardButton::new(
        "Loading...",
        InlineKeyboardButtonKind::CallbackData("callback".to_string()),
    )]]))
    .into()
}

async fn search(bot: Bot, q: InlineQuery, deezer: Arc<Deezer>) -> anyhow::Result<()> {
    const RESULT_LIMIT: usize = 5;

    if q.query.len() <= 3 {
        return Ok(());
    }

    let search_result = deezer
        .search(&q.query, RESULT_LIMIT as u32)
        .await
        .context("failed search on deezer")?;

    let songs = search_result
        .iter()
        .take(RESULT_LIMIT)
        .map(make_query_result)
        .map(Into::into);

    bot.answer_inline_query(&q.id, songs).cache_time(0).await?;

    Ok(())
}

async fn chosen(
    bot: Bot,
    result: ChosenInlineResult,
    downloader: Arc<Downloader>,
    settings: Arc<Settings>,
) -> anyhow::Result<()> {
    let message_id = result
        .inline_message_id
        .context("did not receive inline message id")?;

    let track_id: u64 = result.result_id.parse()?;

    let song = downloader.download_song(track_id).await?;

    let audio = bot
        .send_audio(settings.buffer_channel, InputFile::memory(song.content))
        .performer(song.metadata.artist.name)
        .title(song.metadata.title)
        .await?;

    let audio = audio.audio().context("just sent an audio")?;

    let input_media = InputMediaAudio::new(InputFile::file_id(&audio.file.id));
    bot.edit_message_media_inline(message_id, InputMedia::Audio(input_media))
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
    dotenv::dotenv()?;
    env_logger::init();

    let settings = Arc::new(Settings::from_env()?);

    let bot = Bot::from_env();
    let deezer = Arc::new(Deezer::default());

    let downloader = deezer_downloader::Downloader::new().await?;
    let downloader = Arc::new(downloader);

    let tree = dptree::entry()
        .branch(Update::filter_inline_query().endpoint(search))
        .branch(Update::filter_chosen_inline_result().endpoint(chosen));

    Dispatcher::builder(bot, tree)
        .enable_ctrlc_handler()
        .dependencies(dptree::deps![deezer, downloader, settings])
        .build()
        .dispatch()
        .await;

    todo!()
}
