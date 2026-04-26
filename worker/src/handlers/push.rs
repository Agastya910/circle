use worker::*;

use circle_shared::PushSubscribeRequest;

use crate::auth::require_session;
use crate::db::db;
use crate::util::json_error;

pub async fn subscribe(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let session = match require_session(&req, &ctx.env).await {
        Ok(s) => s,
        Err(_) => return json_error(401, "unauthorized"),
    };

    let body: PushSubscribeRequest = match req.json().await {
        Ok(b) => b,
        Err(e) => return json_error(400, &format!("bad body: {}", e)),
    };

    // Store the full subscription as JSON. send_tickle expects this shape.
    let sub_json = serde_json::json!({
        "endpoint": body.endpoint,
        "keys": { "p256dh": body.keys_p256dh, "auth": body.keys_auth },
    })
    .to_string();

    db(&ctx.env)?
        .prepare("UPDATE users SET push_sub = ?1 WHERE id = ?2")
        .bind(&[sub_json.into(), session.user_id.into()])?
        .run()
        .await?;

    Response::ok(r#"{"ok":true}"#).map(|mut r| {
        let _ = r.headers_mut().set("Content-Type", "application/json");
        r
    })
}

pub async fn vapid_public(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let key = ctx
        .env
        .secret("VAPID_PUBLIC_KEY")
        .map(|v| v.to_string())
        .unwrap_or_default();
    Response::from_json(&serde_json::json!({ "vapid_public_key": key }))
}
