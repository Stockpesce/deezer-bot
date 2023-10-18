use anyhow::Context;
use sqlx::{FromRow, PgExecutor};

#[derive(FromRow)]
pub struct CachedSong {
    pub id: i32,

    pub deezer_id: i64,
    pub file_id: String,
}

fn slice_conversion(from: &[u64]) -> Option<&[i64]> {
    from.iter()
        .all(|&n| n <= i64::MAX as u64)
        // safe as every number lies into max range
        .then_some(unsafe { std::mem::transmute::<_, &[i64]>(from) })
}

impl CachedSong {
    pub async fn by_deezer_ids<'a>(
        deezer_ids: &[u64],
        executor: impl PgExecutor<'a>,
    ) -> anyhow::Result<Vec<Self>> {
        let postgres_conversion: &[i64] =
            slice_conversion(deezer_ids).context("ids are too big for postgres")?;

        sqlx::query_as("SELECT * FROM songs WHERE deezer_id = ANY($1)")
            .bind(postgres_conversion)
            .fetch_all(executor)
            .await
            .map_err(Into::into)
    }

    /// retrieves a song by its internal id
    pub async fn by_id<'a>(id: i32, executor: impl PgExecutor<'a>) -> anyhow::Result<CachedSong> {
        sqlx::query_as("SELECT * FROM songs WHERE id = $1")
            .bind(id)
            .fetch_one(executor)
            .await
            .map_err(Into::into)
    }

    pub async fn insert_song<'a>(
        deezer_id: u64,
        file_id: &str,
        executor: impl PgExecutor<'a>,
    ) -> anyhow::Result<CachedSong> {
        let postgres_conversion: i64 = deezer_id
            .try_into()
            .context("deezer id is too large for postgres")?;

        sqlx::query_as("INSERT INTO songs (deezer_id, file_id) VALUES ($1, $2) RETURNING *")
            .bind(postgres_conversion)
            .bind(file_id)
            .fetch_one(executor)
            .await
            .map_err(Into::into)
    }
}

#[derive(FromRow)]
pub struct HistoryRecord {
    pub id: i32,

    pub user_id: i64,
    pub song_id: i32,
}

impl HistoryRecord {
    pub async fn get_history<'a>(
        user: i64,
        n: i32,
        executor: impl PgExecutor<'a>,
    ) -> anyhow::Result<Vec<CachedSong>> {
        sqlx::query_as(
            r#"
                SELECT songs.* FROM history 
                RIGHT JOIN songs ON history.song_id = songs.id 
                WHERE history.user_id = $1 
                ORDER BY history.id DESC LIMIT $2
            "#,
        )
        .bind(user)
        .bind(n)
        .fetch_all(executor)
        .await
        .map_err(Into::into)
    }

    pub async fn push_history<'a>(
        cached_song_id: i32,
        user: i64,
        executor: impl PgExecutor<'a>,
    ) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO history(user_id, song_id) VALUES ($1, $2)")
            .bind(user)
            .bind(cached_song_id)
            .execute(executor)
            .await?;

        Ok(())
    }
}
