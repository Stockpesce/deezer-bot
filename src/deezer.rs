use anyhow::Context;
use serde::Deserialize;

const BASE_URL: &str = "https://api.deezer.com";

#[derive(Default)]
pub struct Deezer {
    pub client: reqwest::Client,
}

#[derive(Deserialize)]
pub struct Artist {
    pub name: String,
}

#[derive(Deserialize)]
pub struct Album {
    pub title: String,
    pub cover: String,
}

#[derive(Deserialize)]
pub struct Song {
    pub id: u64,
    pub title: String,
    pub preview: String,
    pub duration: u64,

    pub artist: Artist,
    pub album: Album,
}

impl Deezer {
    pub async fn search(&self, search: &str, limit: u32) -> anyhow::Result<Vec<Song>> {
        #[derive(Deserialize)]
        struct SearchResult {
            data: Vec<Song>,
        }

        self.client
            .get(format!("{BASE_URL}/search"))
            .query(&[("q", search), ("limit", &limit.to_string())])
            .send()
            .await?
            .json::<SearchResult>()
            .await
            .context("failed parsing json")
            .map(|result| result.data)
    }
}
