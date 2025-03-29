use chrono::Utc;
use sqlx::FromRow;

fn slice_conversion(from: &[u64]) -> Option<&[i64]> {
    from.iter()
        .all(|&n| n <= i64::MAX as u64)
        // safe as every number lies into max range
        .then_some(unsafe { std::mem::transmute::<_, &[i64]>(from) })
}

#[derive(FromRow, Debug)]
pub struct CachedSong {
    pub id: i32,

    pub deezer_id: i64,
    pub file_id: String,

    pub song_name: String,
    pub song_artist: String,
}

#[derive(FromRow)]
pub struct HistoryRecord {
    pub id: i32,

    pub user_id: i64,
    pub song_id: i32,

    pub search_date: chrono::DateTime<Utc>,
}

#[derive(FromRow)]
pub struct HistorySong {
    pub song_name: String,
    pub song_artist: String,
    pub search_date: chrono::DateTime<Utc>,
}

#[derive(FromRow)]
pub struct LikedSong {
    pub liked_by: i64,
    pub song_id: i64,

    pub sent_by: i64,
    pub liked_date: chrono::DateTime<Utc>,
}

pub mod queries {
    use anyhow::Context;
    use chrono::Utc;
    use deezer_downloader::song::SongMetadata;
    use sqlx::PgExecutor;
    use teloxide::types::UserId;

    use super::{slice_conversion, CachedSong, HistorySong};

    /// a value of true means the song got liked by calling this method,
    /// a value of false means the liked was toggled off.
    pub async fn toggle_like_song(
        user: UserId,
        song_id: i32,
        sent_by: Option<UserId>,
        executor: impl PgExecutor<'_>,
    ) -> anyhow::Result<bool> {
        let user: i64 = user.0.try_into().expect("id too big for postgres");
        let sent_by: Option<i64> =
            sent_by.map(|user| user.0.try_into().expect("id too big for postgres"));

        sqlx::query_scalar(
            r#"
            INSERT INTO likes (liked_by, song_id, sent_by, like_date, liked)
            VALUES ($1, $2, $3, $4, true)
            ON CONFLICT (liked_by, song_id)
            DO UPDATE SET liked = NOT likes.liked
            RETURNING liked
            "#,
        )
        .bind(user)
        .bind(song_id)
        .bind(sent_by)
        .bind(Utc::now())
        .fetch_one(executor)
        .await
        .map_err(Into::into)
    }

    /// returns the amount likes a song has
    pub async fn song_likes(song_id: i64, executor: impl PgExecutor<'_>) -> anyhow::Result<i64> {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM likes WHERE song_id = $1
            "#,
        )
        .bind(song_id)
        .fetch_one(executor)
        .await
        .map_err(Into::into)
    }

    pub async fn get_cached_history_no_repeat(
        user: i64,
        n: i32,
        executor: impl PgExecutor<'_>,
    ) -> anyhow::Result<Vec<CachedSong>> {
        sqlx::query_as(
            r#"
            SELECT songs.* FROM (
                SELECT DISTINCT history.song_id, MAX(history.id) as hid
                FROM history
                WHERE history.user_id = $1
                GROUP BY history.song_id
            )
            INNER JOIN songs ON song_id = songs.id
            ORDER BY hid DESC
            LIMIT $2
            "#,
        )
        .bind(user)
        .bind(n)
        .fetch_all(executor)
        .await
        .map_err(Into::into)
    }

    pub async fn get_history(
        user: i64,
        n: i32,
        executor: impl PgExecutor<'_>,
    ) -> anyhow::Result<Vec<HistorySong>> {
        sqlx::query_as(
            r#"
            SELECT songs.song_name, songs.song_artist, history.search_date FROM history
            INNER JOIN songs ON history.song_id = songs.id
            WHERE history.user_id = $1
            ORDER BY history.id DESC
            LIMIT $2
            "#,
        )
        .bind(user)
        .bind(n)
        .fetch_all(executor)
        .await
        .map_err(Into::into)
    }

    pub async fn push_history(
        cached_song_id: i32,
        user: i64,
        executor: impl PgExecutor<'_>,
    ) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO history(user_id, song_id, search_date) VALUES ($1, $2, $3)")
            .bind(user)
            .bind(cached_song_id)
            .bind(Utc::now())
            .execute(executor)
            .await?;

        Ok(())
    }

    pub async fn by_deezer_ids(
        deezer_ids: &[u64],
        executor: impl PgExecutor<'_>,
    ) -> anyhow::Result<Vec<CachedSong>> {
        let postgres_conversion: &[i64] =
            slice_conversion(deezer_ids).expect("ids are too big for postgres");

        sqlx::query_as("SELECT * FROM songs WHERE deezer_id = ANY($1)")
            .bind(postgres_conversion)
            .fetch_all(executor)
            .await
            .map_err(Into::into)
    }

    pub async fn insert_song(
        deezer_id: u64,
        file_id: &str,
        song: &SongMetadata,
        executor: impl PgExecutor<'_>,
    ) -> anyhow::Result<CachedSong> {
        let postgres_conversion: i64 = deezer_id
            .try_into()
            .expect("deezer id is too large for postgres");

        sqlx::query_as(
            r#"
                INSERT INTO songs (deezer_id, file_id, song_name, song_artist)
                VALUES ($1, $2, $3, $4)
                RETURNING *
            "#,
        )
        .bind(postgres_conversion)
        .bind(file_id)
        .bind(&song.title)
        .bind(&song.artist.name)
        .fetch_one(executor)
        .await
        .map_err(Into::into)
    }
}
