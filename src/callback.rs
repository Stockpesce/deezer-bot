use serde::{de::DeserializeOwned, Deserialize, Serialize};
use teloxide::{
    payloads::AnswerCallbackQuerySetters,
    prelude::Requester,
    types::{CallbackQuery, InlineQuery},
    Bot,
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum CallbackData {
    Nothing,
    Like { id: i32 }, // song id
}

pub async fn handle_callback(
    bot: Bot,
    query: CallbackQuery,
    data: CallbackData,
) -> anyhow::Result<()> {
    match data {
        CallbackData::Nothing => {
            bot.answer_callback_query(query.id)
                .text("Nothing to see here!")
                .await?;
        }
        _ => unimplemented!(),
    };

    Ok(())
}
