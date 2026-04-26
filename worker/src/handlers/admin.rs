use worker::*;

use crate::db::{circle_id, db};
use crate::util::{json_error, new_id, now_secs};

pub async fn create_invite(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Gate with ADMIN_SECRET env var. Callers send: Authorization: Bearer <secret>
    // Read from [vars] in wrangler.toml for dev; for prod set via `wrangler secret put ADMIN_SECRET`
    // and switch this line to ctx.env.secret("ADMIN_SECRET").
    let secret = match ctx.env.var("ADMIN_SECRET") {
        Ok(s) => s.to_string(),
        Err(_) => return json_error(503, "admin not configured (set ADMIN_SECRET var)"),
    };

    let provided = req
        .headers()
        .get("Authorization")?
        .and_then(|h| h.strip_prefix("Bearer ").map(|t| t.to_string()));

    if provided.as_deref() != Some(&secret) {
        return json_error(401, "invalid admin secret");
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
