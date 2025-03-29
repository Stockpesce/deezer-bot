use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use anyhow::Context;
use serde::Deserialize;
use tokio::sync::RwLock;

const BASE_URL: &str = "https://api.deezer.com";

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

#[derive(Default)]
pub struct Deezer {
    pub client: reqwest::Client,
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

struct InnerDeezerDownloader {
    renew_time: SystemTime,
    downloader: deezer_downloader::Downloader,
}

impl InnerDeezerDownloader {
    async fn renew(&mut self) -> anyhow::Result<()> {
        self.downloader.update_tokens().await?;
        self.renew_time = SystemTime::now();

        Ok(())
    }
}

#[derive(Clone)]
pub struct DeezerDownloader {
    inner: Arc<RwLock<InnerDeezerDownloader>>,
}

impl DeezerDownloader {
    const TRESHOLD: Duration = Duration::from_secs(60 * 60); // 1 hr

    pub fn new(downloader: deezer_downloader::Downloader) -> Self {
        let inner = InnerDeezerDownloader {
            renew_time: SystemTime::now(),
            downloader,
        };

        Self {
            inner: Arc::new(RwLock::new(inner)),
        }
    }

    // cheks whether it should gather a new token
    async fn should_renew(&self) -> bool {
        let reader = self.inner.read().await;
        SystemTime::now() > reader.renew_time + Self::TRESHOLD
    }

    // write block the downloader and renew it
    pub async fn renew(&self) -> anyhow::Result<()> {
        let mut inner = self.inner.write().await;
        inner.renew().await?;

        Ok(())
    }

    pub async fn download(&self, id: u64) -> anyhow::Result<deezer_downloader::song::Song> {
        if self.should_renew().await {
            log::info!("Downloader expired, renewing it");
            self.renew().await?;
            log::info!("Downloader renewed")
        }

        let inner = self.inner.read().await;

        deezer_downloader::Song::download(id, &inner.downloader).await
    }
}
