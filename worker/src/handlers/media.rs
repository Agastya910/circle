use worker::*;

use circle_shared::{UploadUrlRequest, UploadUrlResponse};

use crate::auth::require_session;
use crate::db::{circle_id, db};
use crate::util::{json_error, new_id};

const MAX_IMAGE_BYTES: usize = 2_000_000;       // 2 MB
const MAX_VIDEO_BYTES: usize = 5_000_000;       // 5 MB compressed-on-client cap
// Per-user cumulative storage cap. R2 free tier is 10 GB; this keeps a
// 10-user circle inside half the free tier with comfortable headroom.
const MAX_USER_BYTES: f64 = 524_288_000.0;      // 500 MB per user

pub async fn upload_url(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let session = match require_session(&req, &ctx.env).await {
        Ok(s) => s,
        Err(_) => return json_error(401, "unauthorized"),
    };

    let body: UploadUrlRequest = req
        .json()
        .await
        .unwrap_or(UploadUrlRequest { kind: "image".into(), ext: None });

    let ext: &str = match body.kind.as_str() {
        "video" => match body.ext.as_deref() {
            Some("mp4") => "mp4",
            // Default to webm if unspecified or unknown.
            _ => "webm",
        },
        _ => "webp",
    };
    let key = format!("{}-{}.{}", session.user_id, new_id(), ext);
    let upload_url = format!("/api/media/{}", key);
    Response::from_json(&UploadUrlResponse { upload_url, key })
}

pub async fn fetch(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    match req.method() {
        Method::Put => put_media(req, ctx).await,
        Method::Get => get_media(req, ctx).await,
        _ => json_error(405, "method not allowed"),
    }
}

async fn put_media(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let session = match require_session(&req, &ctx.env).await {
        Ok(s) => s,
        Err(_) => return json_error(401, "unauthorized"),
    };

    let key = match ctx.param("key") {
        Some(k) => k.clone(),
        None => return json_error(400, "missing key"),
    };

    if !key.starts_with(&format!("{}-", session.user_id)) {
        return json_error(403, "key not owned");
    }

    let (max_bytes, content_type) = if key.ends_with(".webm") {
        (MAX_VIDEO_BYTES, "video/webm")
    } else if key.ends_with(".mp4") {
        (MAX_VIDEO_BYTES, "video/mp4")
    } else {
        (MAX_IMAGE_BYTES, "image/webp")
    };

    let bytes = req.bytes().await?;
    if bytes.len() > max_bytes {
        return json_error(
            413,
            &format!(
                "media too large ({} bytes, max {})",
                bytes.len(),
                max_bytes
            ),
        );
    }

    let used = db(&ctx.env)?
        .prepare("SELECT media_bytes FROM users WHERE id = ?1")
        .bind(&[session.user_id.clone().into()])?
        .first::<UserBytesRow>(None)
        .await?
        .map(|r| r.media_bytes)
        .unwrap_or(0.0);
    let added = bytes.len() as f64;
    if used + added > MAX_USER_BYTES {
        return json_error(
            413,
            &format!(
                "storage cap reached ({} of {} bytes used)",
                used as u64, MAX_USER_BYTES as u64
            ),
        );
    }

    ctx.env
        .bucket("MEDIA")?
        .put(&key, bytes.clone())
        .http_metadata(HttpMetadata {
            content_type: Some(content_type.into()),
            ..Default::default()
        })
        .execute()
        .await?;

    let _ = db(&ctx.env)?
        .prepare("UPDATE users SET media_bytes = media_bytes + ?1 WHERE id = ?2")
        .bind(&[added.into(), session.user_id.clone().into()])?
        .run()
        .await;

    Response::ok(r#"{"ok":true}"#).map(|mut r| {
        let _ = r.headers_mut().set("Content-Type", "application/json");
        r
    })
}

#[derive(serde::Deserialize)]
struct UserBytesRow {
    media_bytes: f64,
}

async fn get_media(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let token = {
        let from_header = req
            .headers()
            .get("Authorization")
            .ok()
            .flatten()
            .and_then(|h| h.strip_prefix("Bearer ").map(|t| t.to_string()));
        if let Some(t) = from_header {
            Some(t)
        } else {
            req.url().ok().and_then(|u| {
                u.query_pairs()
                    .find(|(k, _)| k == "t")
                    .map(|(_, v)| v.into_owned())
            })
        }
    };
    let token = match token {
        Some(t) => t,
        None => return json_error(401, "unauthorized"),
    };
    let session = match crate::auth::session_from_token(&ctx.env, &token).await {
        Ok(s) => s,
        Err(_) => return json_error(401, "unauthorized"),
    };

    let key = match ctx.param("key") {
        Some(k) => k.clone(),
        None => return json_error(400, "missing key"),
    };

    let row = db(&ctx.env)?
        .prepare(
            "SELECT 1 as found FROM posts
             WHERE (image_key = ?1 OR video_key = ?1) AND circle_id = ?2 LIMIT 1",
        )
        .bind(&[key.clone().into(), circle_id(&ctx.env).into()])?
        .first::<FoundRow>(None)
        .await?;
    if row.is_none() {
        if !key.starts_with(&format!("{}-", session.user_id)) {
            return json_error(404, "not found");
        }
    }

    let obj = ctx.env.bucket("MEDIA")?.get(&key).execute().await?;
    let obj = match obj {
        Some(o) => o,
        None => return json_error(404, "not found"),
    };
    let body = match obj.body() {
        Some(b) => b,
        None => return json_error(404, "not found"),
    };

    let content_type = if key.ends_with(".webm") {
        "video/webm"
    } else if key.ends_with(".mp4") {
        "video/mp4"
    } else {
        "image/webp"
    };

    let mut resp = Response::from_stream(body.stream()?)?;
    resp.headers_mut().set("Content-Type", content_type)?;
    resp.headers_mut().set("Cache-Control", "private, max-age=3600")?;
    Ok(resp)
}

#[derive(serde::Deserialize)]
struct FoundRow {
    #[allow(dead_code)]
    found: i64,
}
