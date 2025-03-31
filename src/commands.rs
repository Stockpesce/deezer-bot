use anyhow::Context;
use sqlx::{Pool, Postgres};
use teloxide::{
    macros::BotCommands,
    payloads::SendMessageSetters,
    prelude::Requester,
    types::{InlineKeyboardButton, Message, ParseMode, ReplyMarkup},
    Bot,
};

use crate::db::queries;

macro_rules! reply {
    ($bot: expr, $message: expr, $text: expr) => {
        $bot.send_message($message.chat.id, $text)
            .parse_mode(ParseMode::Html)
    };
}

#[derive(BotCommands, PartialEq, Debug, Clone)]
#[command(rename_rule = "lowercase", parse_with = "split")]
pub enum Commands {
    #[command(description = "Display your search history")]
    History,
    #[command(description = "Get a list of your favorite songs")]
    Liked,
    #[command(description = "Show start message")]
    Start,
}

impl std::fmt::Display for Commands {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::History => write!(f, "history"),
            Self::Start => write!(f, "start"),
            Self::Liked => write!(f, "liked"),
        }
    }
}

pub async fn history_command(bot: Bot, msg: Message, pool: Pool<Postgres>) -> anyhow::Result<()> {
    use std::fmt::Write;

    let from = msg.from().context("command was not sent by a user")?;

    let history = queries::get_history(from.id.0 as i64, 10, &pool).await?;
    let text = String::from("Your last 10 searches:\n\n");
    let history_formatted = history
        .into_iter()
        .enumerate()
        .map(|(n, song)| (n + 1, song))
        .fold(text, |mut s, (n, song)| {
            writeln!(&mut s, "{n}) {} - {}", song.song_artist, song.song_name).ok();
            s
        });

    reply!(bot, msg, history_formatted).await?;

    Ok(())
}

pub async fn start_command(bot: Bot, message: Message) -> anyhow::Result<()> {
    let button = InlineKeyboardButton::new(
        "Search a song",
        teloxide::types::InlineKeyboardButtonKind::SwitchInlineQueryCurrentChat("".into()),
    );

    let keyboard_markup = ReplyMarkup::inline_kb([[button]]);
    reply!(bot, message, "Hi, song searching is only available inline.\nStart searching by clicking the button below").reply_markup(keyboard_markup).await?;

    Ok(())
}

pub async fn liked_command(bot: Bot, message: Message, pool: Pool<Postgres>) -> anyhow::Result<()> {
    let Some(user) = message.from() else {
        reply!(
            bot,
            message,
            "Sorry, this command is only available for users!"
        )
        .await?;

        return Ok(());
    };

    let likes = queries::get_likes(&user.id, 10, &pool).await?;

    let text = "Here's the list of your latest favorite songs:\n".to_string();
    likes.into_iter().fold(text, |fold, item|);

    todo!()
}

pub async fn unknown_command(bot: Bot, msg: Message) -> anyhow::Result<()> {
    // some groups might enable access to all messages
    // by mistake
    if msg.chat.is_private() {
        bot.send_message(msg.chat.id, "Uknown command!").await?;
    }

    Ok(())
}
