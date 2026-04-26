use worker::*;

use circle_shared::{AuthResponse, LoginRequest, RegisterRequest, User};

use crate::auth::{create_session, delete_session, require_session};
use crate::db::{circle_id, db};
use crate::util::{json_error, new_id, now_secs};

pub async fn register(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let body: RegisterRequest = match req.json().await {
        Ok(b) => b,
        Err(e) => return json_error(400, &format!("bad body: {}", e)),
    };

    let display_name = body.display_name.trim();
    if display_name.is_empty() || display_name.len() > 40 {
        return json_error(400, "display_name required, max 40 chars");
    }
    if body.pin.len() < 4 {
        return json_error(400, "pin must be at least 4 characters");
    }

    let db = db(&ctx.env)?;
    let now = now_secs();
    let user_id = new_id();

    let invite = db
        .prepare("SELECT circle_id FROM invite_codes WHERE code = ?1 AND used_by IS NULL")
        .bind(&[body.invite_code.clone().into()])?
        .first::<InviteRow>(None)
        .await?;

    let invite = match invite {
        Some(i) => i,
        None => return json_error(403, "invalid or already-used invite code"),
    };

    let (pin_salt, pin_hash) = hash_pin(&body.pin);

    db.prepare("UPDATE invite_codes SET used_by = ?1, used_at = ?2 WHERE code = ?3 AND used_by IS NULL")
        .bind(&[user_id.clone().into(), now.into(), body.invite_code.into()])?
        .run().await?;

    db.prepare(
        "INSERT INTO users (id, circle_id, display_name, pin_hash, pin_salt, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&[
        user_id.clone().into(),
        invite.circle_id.clone().into(),
        display_name.into(),
        pin_hash.clone().into(),
        pin_salt.into(),
        now.into(),
    ])?
    .run().await?;

    let token = create_session(&ctx.env, &user_id).await?;
    let user = User { id: user_id, display_name: display_name.to_string(), avatar_key: None };
    Response::from_json(&AuthResponse { token, user })
}

pub async fn login(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let body: LoginRequest = match req.json().await {
        Ok(b) => b,
        Err(e) => return json_error(400, &format!("bad body: {}", e)),
    };

    if body.display_name.trim().is_empty() || body.pin.len() < 4 {
        return json_error(400, "display_name and pin (4+ chars) required");
    }

    let db = db(&ctx.env)?;

    let row = db
        .prepare(
            "SELECT id, display_name, avatar_key, pin_hash, pin_salt
             FROM users WHERE display_name = ?1 AND circle_id = ?2",
        )
        .bind(&[body.display_name.trim().into(), circle_id(&ctx.env).into()])?
        .first::<LoginRow>(None)
        .await?;

    let row = match row {
        Some(r) => r,
        None => return json_error(401, "invalid name or PIN"),
    };

    match (&row.pin_hash, &row.pin_salt) {
        (Some(stored_hash), Some(salt)) => {
            let (_, computed) = hash_pin_with_salt(salt, &body.pin);
            if computed != *stored_hash {
                return json_error(401, "invalid name or PIN");
            }
        }
        _ => return json_error(401, "this account has no PIN — re-register with an invite code"),
    }

    let token = create_session(&ctx.env, &row.id).await?;
    let user = User {
        id: row.id,
        display_name: row.display_name,
        avatar_key: row.avatar_key,
    };
    Response::from_json(&AuthResponse { token, user })
}

pub async fn me(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let session = match require_session(&req, &ctx.env).await {
        Ok(s) => s,
        Err(_) => return json_error(401, "unauthorized"),
    };
    Response::from_json(&User {
        id: session.user_id,
        display_name: session.display_name,
        avatar_key: session.avatar_key,
    })
}

pub async fn logout(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    if let Some(h) = req.headers().get("Authorization")? {
        if let Some(token) = h.strip_prefix("Bearer ") {
            delete_session(&ctx.env, token).await?;
        }
    }
    Response::ok(r#"{"ok":true}"#).map(|mut r| {
        let _ = r.headers_mut().set("Content-Type", "application/json");
        r
    })
}

/// Hash a PIN with a fresh random salt. Returns (salt_hex, hash_hex).
fn hash_pin(pin: &str) -> (String, String) {
    let mut salt_bytes = [0u8; 16];
    getrandom::getrandom(&mut salt_bytes).expect("getrandom");
    let salt = hex::encode(salt_bytes);
    let (_, hash) = hash_pin_with_salt(&salt, pin);
    (salt, hash)
}

fn hash_pin_with_salt(salt: &str, pin: &str) -> (String, String) {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(salt.as_bytes());
    h.update(b":");
    h.update(pin.as_bytes());
    (salt.to_string(), hex::encode(h.finalize()))
}

#[derive(serde::Deserialize)]
struct InviteRow {
    circle_id: String,
}

#[derive(serde::Deserialize)]
struct LoginRow {
    id: String,
    display_name: String,
    avatar_key: Option<String>,
    pin_hash: Option<String>,
    pin_salt: Option<String>,
}
