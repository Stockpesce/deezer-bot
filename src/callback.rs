use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub enum CallbackData {
    Nothing,
    Like { id: i32 }, // song id
}
