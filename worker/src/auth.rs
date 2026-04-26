use worker::*;

use crate::db::db;
use crate::util::now_secs;

pub const SESSION_TTL_SECS: f64 = 60.0 * 60.0 * 24.0 * 30.0; // 30 days

#[derive(Debug, Clone)]
pub struct Session {
    pub user_id: String,
    pub display_name: String,
    pub avatar_key: Option<String>,
}

pub async fn create_session(env: &Env, user_id: &str) -> Result<String> {
    let token = random_token();
    let now = now_secs();
    let expires = now + SESSION_TTL_SECS;

    db(env)?
        .prepare("INSERT INTO sessions (token, user_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)")
        .bind(&[token.clone().into(), user_id.into(), now.into(), expires.into()])?
        .run()
        .await?;

    Ok(token)
}

pub async fn require_session(req: &Request, env: &Env) -> Result<Session> {
    let header = req
        .headers()
        .get("Authorization")?
        .ok_or_else(|| Error::RustError("missing Authorization".into()))?;
    let token = header
        .strip_prefix("Bearer ")
        .ok_or_else(|| Error::RustError("malformed Authorization".into()))?;
    session_from_token(env, token).await
}

pub async fn session_from_token(env: &Env, token: &str) -> Result<Session> {
    let now = now_secs();
    let row = db(env)?
        .prepare(
            "SELECT u.id, u.display_name, u.avatar_key
             FROM sessions s JOIN users u ON s.user_id = u.id
             WHERE s.token = ?1 AND s.expires_at > ?2",
        )
        .bind(&[token.into(), now.into()])?
        .first::<SessionRow>(None)
        .await?
        .ok_or_else(|| Error::RustError("invalid session".into()))?;
    Ok(Session {
        user_id: row.id,
        display_name: row.display_name,
        avatar_key: row.avatar_key,
    })
}

pub async fn delete_session(env: &Env, token: &str) -> Result<()> {
    db(env)?
        .prepare("DELETE FROM sessions WHERE token = ?1")
        .bind(&[token.into()])?
        .run()
        .await?;
    Ok(())
}

#[derive(serde::Deserialize)]
struct SessionRow {
    id: String,
    display_name: String,
    avatar_key: Option<String>,
}

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).expect("getrandom");
    hex::encode(bytes)
}
