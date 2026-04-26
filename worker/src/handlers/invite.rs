use worker::*;

use crate::db::{circle_id, db};
use crate::util::{json_error, new_id, now_secs};

pub async fn redeem(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    #[derive(serde::Deserialize)]
    struct Body {
        passphrase: String,
    }
    let body: Body = match req.json().await {
        Ok(b) => b,
        Err(e) => return json_error(400, &format!("bad body: {}", e)),
    };

    let supplied = body.passphrase.trim();
    if supplied.is_empty() {
        return json_error(400, "passphrase required");
    }

    let row = db(&ctx.env)?
        .prepare("SELECT value FROM admin_settings WHERE key = 'invite_passphrase'")
        .first::<PassphraseRow>(None)
        .await?;
    let stored = match row {
        Some(r) => r.value,
        None => return json_error(503, "invites are not currently open"),
    };

    if !constant_time_eq(supplied.as_bytes(), stored.as_bytes()) {
        return json_error(401, "invalid passphrase");
    }

    let code = format!("p-{}", &new_id()[..8]);
    let cid = circle_id(&ctx.env);
    let now = now_secs();
    db(&ctx.env)?
        .prepare("INSERT INTO invite_codes (code, circle_id, created_at) VALUES (?1, ?2, ?3)")
        .bind(&[code.clone().into(), cid.into(), now.into()])?
        .run()
        .await?;

    Response::ok(serde_json::json!({ "code": code }).to_string()).map(|mut r| {
        let _ = r.headers_mut().set("Content-Type", "application/json");
        r
    })
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[derive(serde::Deserialize)]
struct PassphraseRow {
    value: String,
}
