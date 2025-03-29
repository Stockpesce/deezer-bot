use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use teloxide::{
    payloads::AnswerCallbackQuerySetters,
    prelude::Requester,
    types::{CallbackQuery, InlineQuery},
    Bot,
};

use crate::db::queries;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum CallbackData {
    Nothing,
    Like { id: i32 }, // song id
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
        CallbackData::Like { id } => {
            println!("im likin here");

            let from = query
                .message
                .as_ref()
                .and_then(|msg| msg.from())
                .map(|from| from.id);

            let liked = queries::toggle_like_song(query.from.id, id, from, &pool).await?;
            let liked = ["unliked", "liked"][liked as usize];

            let message = format!("You {liked} the song!");
            bot.answer_callback_query(query.id).text(message).await?;
        }
    };

    Ok(())
}
