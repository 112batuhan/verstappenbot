use sqlx::{PgPool, Pool};

use anyhow::Result;

pub struct DbSound {
    pub prompt: String,
    pub language: String,
    pub file_name: String,
}

pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn new() -> Result<Self> {
        let pool = Pool::connect(
            std::env::var("DATABASE_URL")
                .expect("DATABASE_URL must be set")
                .as_str(),
        )
        .await
        .map_err(anyhow::Error::from)?;
        Ok(Self { pool })
    }

    pub async fn get_sounds(&self, server_id: &str) -> Result<Vec<DbSound>> {
        sqlx::query_as!(
            DbSound,
            r#"SELECT prompt, language, file_name FROM sounds WHERE server_id = $1"#,
            server_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(anyhow::Error::from)
    }

    pub async fn add_sound(
        &self,
        server_id: &str,
        prompt: &str,
        language: &str,
        file_name: &str,
    ) -> Result<()> {
        sqlx::query!(
            r#"INSERT INTO sounds (server_id, prompt, language, file_name) VALUES ($1, $2, $3, $4)"#,
            server_id,
            prompt,
            language,
            file_name,
        )
        .execute(&self.pool)
        .await
        .map_err(anyhow::Error::from)?;

        Ok(())
    }

    pub async fn remove_sound(&self, server_id: &str, prompt: &str) -> Result<DbSound> {
        sqlx::query_as!(
            DbSound,
            r#"DELETE FROM sounds WHERE server_id = $1 AND prompt = $2 returning prompt, language, file_name"#,
            server_id,
            prompt,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(anyhow::Error::from)
    }
}
