use worker::*;

use circle_shared::ReactRequest;

use crate::auth::require_session;
use crate::db::db;
use crate::util::{json_error, new_id, now_secs};

// Toggle reaction: insert if absent, delete if present.
pub async fn toggle(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let session = match require_session(&req, &ctx.env).await {
        Ok(s) => s,
        Err(_) => return json_error(401, "unauthorized"),
    };

    let body: ReactRequest = match req.json().await {
        Ok(b) => b,
        Err(e) => return json_error(400, &format!("bad body: {}", e)),
    };

    if !is_allowed_emoji(&body.emoji) {
        return json_error(400, "emoji not allowed");
    }

    let db = db(&ctx.env)?;

    let existing = db
        .prepare(
            "SELECT id FROM reactions
             WHERE post_id = ?1 AND user_id = ?2 AND emoji = ?3",
        )
        .bind(&[
            body.post_id.clone().into(),
            session.user_id.clone().into(),
            body.emoji.clone().into(),
        ])?
        .first::<IdRow>(None)
        .await?;

    if let Some(row) = existing {
        db.prepare("DELETE FROM reactions WHERE id = ?1")
            .bind(&[row.id.into()])?
            .run()
            .await?;
        return Response::ok(r#"{"state":"removed"}"#).map(|mut r| {
            let _ = r.headers_mut().set("Content-Type", "application/json");
            r
        });
    }

    db.prepare(
        "INSERT INTO reactions (id, post_id, user_id, emoji, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&[
        new_id().into(),
        body.post_id.into(),
        session.user_id.into(),
        body.emoji.into(),
        now_secs().into(),
    ])?
    .run()
    .await?;

    Response::ok(r#"{"state":"added"}"#).map(|mut r| {
        let _ = r.headers_mut().set("Content-Type", "application/json");
        r
    })
}

fn is_allowed_emoji(s: &str) -> bool {
    // Narrow allowlist keeps the data tidy and prevents abuse.
    matches!(s, "❤️" | "😂" | "🔥" | "👏" | "🙏" | "😢")
}

#[derive(serde::Deserialize)]
struct IdRow {
    id: String,
}
