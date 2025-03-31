use anyhow::Context;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use teloxide::{
    payloads::{AnswerCallbackQuerySetters, EditMessageReplyMarkupInlineSetters},
    prelude::Requester,
    types::CallbackQuery,
    Bot,
};

use crate::{db::queries, markup};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum CallbackData {
    Nothing,
    Like { song_id: i32 }, // song id
}

pub fn callback_decoder<T: Serialize + DeserializeOwned>() -> impl Fn(CallbackQuery) -> Option<T> {
    move |query: CallbackQuery| {
        let data = query.data?;
        crate::encoding::decode(&data)
            .inspect_err(|err| log::error!("Error decoding callback data! {err}"))
            .ok()
    }
}

pub async fn handle_callback(
    bot: Bot,
    query: CallbackQuery,
    data: CallbackData,
    pool: Pool<Postgres>,
) -> anyhow::Result<()> {
    match data {
        CallbackData::Nothing => {
            bot.answer_callback_query(query.id)
                .text("Nothing to see here!")
                .await?;
        }
        CallbackData::Like { song_id: id } => {
            println!("im likin here");

            let from = query
                .message
                .as_ref()
                .and_then(|msg| msg.from())
                .map(|from| from.id);

            let liked = queries::toggle_like_song(&query.from.id, id, from, &pool).await?;
            let liked = ["unliked", "liked"][liked as usize];

            let message = format!("You {liked} the song!");
            bot.answer_callback_query(&query.id).text(message).await?;
        }
    };

    Ok(())
}
