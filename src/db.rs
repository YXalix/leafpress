use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

use crate::types::Comment;

/// Idempotent schema creation at startup
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS comments (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  post_slug TEXT NOT NULL,
  author TEXT NOT NULL,
  content TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS likes (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  post_slug TEXT NOT NULL,
  ip_hash TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_comments_slug ON comments(post_slug);
CREATE UNIQUE INDEX IF NOT EXISTS idx_likes_slug_ip ON likes(post_slug, ip_hash);
";

pub async fn init(path: &str) -> Result<SqlitePool> {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(opts)
        .await?;
    sqlx::raw_sql(SCHEMA).execute(&pool).await?;
    Ok(pool)
}

pub async fn comments_for(pool: &SqlitePool, slug: &str) -> Result<Vec<Comment>> {
    let rows = sqlx::query(
        "SELECT id, author, content, created_at FROM comments WHERE post_slug = ? ORDER BY id ASC",
    )
    .bind(slug)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| Comment {
            id: r.get("id"),
            author: r.get("author"),
            content: r.get("content"),
            created_at: r.get("created_at"),
        })
        .collect())
}

pub async fn add_comment(pool: &SqlitePool, slug: &str, author: &str, content: &str) -> Result<()> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    sqlx::query(
        "INSERT INTO comments (post_slug, author, content, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(slug)
    .bind(author)
    .bind(content)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_comment(pool: &SqlitePool, id: i64) -> Result<()> {
    sqlx::query("DELETE FROM comments WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Returns (like count, whether the current IP has already liked)
pub async fn like_state(pool: &SqlitePool, slug: &str, ip_hash: &str) -> Result<(i64, bool)> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM likes WHERE post_slug = ?")
        .bind(slug)
        .fetch_one(pool)
        .await?;
    let liked: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM likes WHERE post_slug = ? AND ip_hash = ?")
            .bind(slug)
            .bind(ip_hash)
            .fetch_one(pool)
            .await?;
    Ok((count, liked > 0))
}

pub async fn toggle_like(pool: &SqlitePool, slug: &str, ip_hash: &str) -> Result<(i64, bool)> {
    let (_, liked) = like_state(pool, slug, ip_hash).await?;
    if liked {
        sqlx::query("DELETE FROM likes WHERE post_slug = ? AND ip_hash = ?")
            .bind(slug)
            .bind(ip_hash)
            .execute(pool)
            .await?;
    } else {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
        sqlx::query(
            "INSERT OR IGNORE INTO likes (post_slug, ip_hash, created_at) VALUES (?, ?, ?)",
        )
        .bind(slug)
        .bind(ip_hash)
        .bind(now)
        .execute(pool)
        .await?;
    }
    let (count, now_liked) = like_state(pool, slug, ip_hash).await?;
    Ok((count, now_liked))
}
