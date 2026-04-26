use worker::*;

use crate::db::{circle_id, db};
use crate::util::{json_error, new_id, now_secs};

fn check_admin(req: &Request, env: &Env) -> std::result::Result<(), Response> {
    let secret = match env.secret("ADMIN_SECRET") {
        Ok(s) => s.to_string(),
        Err(_) => match env.var("ADMIN_SECRET") {
            Ok(s) => s.to_string(),
            Err(_) => {
                return Err(json_error(503, "admin not configured").unwrap());
            }
        },
    };
    let provided = req
        .headers()
        .get("Authorization")
        .ok()
        .flatten()
        .and_then(|h| h.strip_prefix("Bearer ").map(|t| t.to_string()));
    if provided.as_deref() != Some(&secret) {
        return Err(json_error(401, "invalid admin secret").unwrap());
    }
    Ok(())
}

pub async fn set_passphrase(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    if let Err(r) = check_admin(&req, &ctx.env) {
        return Ok(r);
    }

    #[derive(serde::Deserialize)]
    struct Body {
        passphrase: String,
    }
    let body: Body = match req.json().await {
        Ok(b) => b,
        Err(e) => return json_error(400, &format!("bad body: {}", e)),
    };

    let p = body.passphrase.trim();
    if p.len() < 8 {
        return json_error(400, "passphrase too short (min 8 chars)");
    }
    if p.len() > 200 {
        return json_error(400, "passphrase too long (max 200 chars)");
    }

    let now = now_secs();
    db(&ctx.env)?
        .prepare(
            "INSERT INTO admin_settings (key, value, updated_at)
             VALUES ('invite_passphrase', ?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(&[p.into(), now.into()])?
        .run()
        .await?;

    Response::ok(serde_json::json!({"ok": true, "updated_at": now}).to_string()).map(|mut r| {
        let _ = r.headers_mut().set("Content-Type", "application/json");
        r
    })
}

pub async fn create_invite(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    if let Err(r) = check_admin(&req, &ctx.env) {
        return Ok(r);
    }

    let code = format!("inv-{}", &new_id()[..8]);
    let cid = circle_id(&ctx.env);
    let now = now_secs();

    db(&ctx.env)?
        .prepare("INSERT INTO invite_codes (code, circle_id, created_at) VALUES (?1, ?2, ?3)")
        .bind(&[code.clone().into(), cid.into(), now.into()])?
        .run()
        .await?;

    let body = serde_json::json!({ "code": code }).to_string();
    Response::ok(body).map(|mut r| {
        let _ = r.headers_mut().set("Content-Type", "application/json");
        r
    })
}
