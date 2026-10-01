use crate::Result;
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::{str::FromStr, time::Duration};

#[derive(Debug, Clone)]
pub struct DbConfig {
    pub url: String,
    /// Optional direct (non-pooled) URL used only for migrations.
    pub migration_url: Option<String>,
    pub max_connections: u32,
    /// `Some(0)` disables the prepared-statement cache. Needed only behind
    /// Supabase's *transaction* pooler (port 6543), which cannot do prepared
    /// statements. Direct and session-pooler connections do not need it.
    pub statement_cache: Option<usize>,
}

#[derive(Clone)]
pub struct Store {
    pool: PgPool,
    migration_url: Option<String>,
}

fn options(url: &str, statement_cache: Option<usize>) -> Result<PgConnectOptions> {
    let mut o = PgConnectOptions::from_str(url)?;
    if let Some(n) = statement_cache {
        o = o.statement_cache_capacity(n);
    }
    Ok(o)
}

impl Store {
    pub async fn connect(cfg: &DbConfig) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(cfg.max_connections)
            .acquire_timeout(Duration::from_secs(10))
            .connect_with(options(&cfg.url, cfg.statement_cache)?)
            .await?;
        Ok(Self {
            pool,
            migration_url: cfg.migration_url.clone(),
        })
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self {
            pool,
            migration_url: None,
        }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Run embedded migrations. Uses `migration_url` (direct connection) when
    /// configured, otherwise the main pool.
    pub async fn migrate(&self) -> Result<()> {
        static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");
        if let Some(url) = &self.migration_url {
            let pool = PgPoolOptions::new()
                .max_connections(1)
                .connect_with(options(url, None)?)
                .await?;
            MIGRATOR.run(&pool).await?;
            pool.close().await;
        } else {
            MIGRATOR.run(&self.pool).await?;
        }
        Ok(())
    }
}
