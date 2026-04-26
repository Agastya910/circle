use worker::*;

use circle_shared::{Comment, NewPostRequest, Post, ReactionSummary, User};

use crate::auth::require_session;
use crate::db::{circle_id, db};
use crate::push::fan_out_post;
use crate::util::{json_error, new_id, now_secs};

pub async fn list(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let session = match require_session(&req, &ctx.env).await {
        Ok(s) => s,
        Err(_) => return json_error(401, "unauthorized"),
    };

    let db = db(&ctx.env)?;
    let rows = db
        .prepare(
            "SELECT p.id, p.body, p.image_key, p.video_key, p.media_keys, p.created_at,
                    u.id as author_id, u.display_name, u.avatar_key
             FROM posts p JOIN users u ON p.author_id = u.id
             WHERE p.circle_id = ?1 AND p.deleted_at IS NULL
             ORDER BY p.created_at DESC
             LIMIT 50",
        )
        .bind(&[circle_id(&ctx.env).into()])?
        .all()
        .await?;

    let raw: Vec<PostRow> = rows.results()?;
    let mut posts: Vec<Post> = Vec::with_capacity(raw.len());

    for r in raw {
        let reactions = load_reactions(&db, &r.id, &session.user_id).await?;
        let comment_count = load_comment_count(&db, &r.id).await?;
        let media_keys: Vec<String> = r.media_keys
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        posts.push(Post {
            id: r.id,
            author: User {
                id: r.author_id,
                display_name: r.display_name,
                avatar_key: r.avatar_key,
            },
            body: r.body.filter(|s| !s.is_empty()),
            image_key: r.image_key.filter(|s| !s.is_empty()),
            video_key: r.video_key.filter(|s| !s.is_empty()),
            media_keys,
            created_at: r.created_at,
            reactions,
            comment_count,
        });
    }

    Response::from_json(&posts)
}

pub async fn create(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let session = match require_session(&req, &ctx.env).await {
        Ok(s) => s,
        Err(_) => return json_error(401, "unauthorized"),
    };

    let body: NewPostRequest = match req.json().await {
        Ok(b) => b,
        Err(e) => return json_error(400, &format!("bad body: {}", e)),
    };

    let has_content = body.body.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false)
        || body.image_key.is_some()
        || body.video_key.is_some()
        || !body.media_keys.is_empty();
    if !has_content {
        return json_error(400, "post needs body, image, or video");
    }

    if body.media_keys.len() > 4 {
        return json_error(400, "max 4 images per post");
    }
    for key in &body.media_keys {
        if !key.starts_with(&format!("{}-", session.user_id)) {
            return json_error(403, "media key not owned by you");
        }
    }

    let media_keys_json = if body.media_keys.is_empty() {
        None
    } else {
        serde_json::to_string(&body.media_keys).ok()
    };

    let id = new_id();
    let now = now_secs();

    db(&ctx.env)?
        .prepare(
            "INSERT INTO posts (id, circle_id, author_id, body, image_key, video_key, media_keys, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(&[
            id.clone().into(),
            circle_id(&ctx.env).into(),
            session.user_id.clone().into(),
            body.body.clone().unwrap_or_default().into(),
            body.image_key.clone().unwrap_or_default().into(),
            body.video_key.clone().unwrap_or_default().into(),
            media_keys_json.clone().unwrap_or_default().into(),
            now.into(),
        ])?
        .run()
        .await?;

    let _ = fan_out_post(&ctx.env, &session.user_id).await;

    let post = Post {
        id,
        author: User {
            id: session.user_id.clone(),
            display_name: session.display_name.clone(),
            avatar_key: session.avatar_key.clone(),
        },
        body: body.body,
        image_key: body.image_key,
        video_key: body.video_key,
        media_keys: body.media_keys,
        created_at: now as i64,
        reactions: vec![],
        comment_count: 0,
    };
    Response::from_json(&post)
}

pub async fn delete(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let session = match require_session(&req, &ctx.env).await {
        Ok(s) => s,
        Err(_) => return json_error(401, "unauthorized"),
    };

    let post_id = match ctx.param("id") {
        Some(v) => v.clone(),
        None => return json_error(400, "missing id"),
    };

    let row = db(&ctx.env)?
        .prepare(
            "SELECT author_id FROM posts
             WHERE id = ?1 AND circle_id = ?2 AND deleted_at IS NULL",
        )
        .bind(&[post_id.clone().into(), circle_id(&ctx.env).into()])?
        .first::<AuthorRow>(None)
        .await?;

    let row = match row {
        Some(r) => r,
        None => return json_error(404, "post not found"),
    };

    if row.author_id != session.user_id {
        return json_error(403, "not your post");
    }

    let now = now_secs();
    db(&ctx.env)?
        .prepare("UPDATE posts SET deleted_at = ?1 WHERE id = ?2")
        .bind(&[now.into(), post_id.into()])?
        .run()
        .await?;

    Response::ok(r#"{"ok":true}"#).map(|mut r| {
        let _ = r.headers_mut().set("Content-Type", "application/json");
        r
    })
}

pub async fn comments(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let _session = match require_session(&req, &ctx.env).await {
        Ok(s) => s,
        Err(_) => return json_error(401, "unauthorized"),
    };

    let post_id = match ctx.param("id") {
        Some(v) => v.clone(),
        None => return json_error(400, "missing id"),
    };

    let rows = db(&ctx.env)?
        .prepare(
            "SELECT c.id, c.body, c.created_at,
                    u.id as author_id, u.display_name, u.avatar_key
             FROM comments c JOIN users u ON c.author_id = u.id
             WHERE c.post_id = ?1
             ORDER BY c.created_at ASC",
        )
        .bind(&[post_id.into()])?
        .all()
        .await?;

    let raw: Vec<CommentRow> = rows.results()?;
    let comments: Vec<Comment> = raw
        .into_iter()
        .map(|r| Comment {
            id: r.id,
            author: User {
                id: r.author_id,
                display_name: r.display_name,
                avatar_key: r.avatar_key,
            },
            body: r.body,
            created_at: r.created_at,
        })
        .collect();

    Response::from_json(&comments)
}

pub async fn add_comment(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let session = match require_session(&req, &ctx.env).await {
        Ok(s) => s,
        Err(_) => return json_error(401, "unauthorized"),
    };

    let post_id = match ctx.param("id") {
        Some(v) => v.clone(),
        None => return json_error(400, "missing id"),
    };

    #[derive(serde::Deserialize)]
    struct Body {
        body: String,
    }
    let payload: Body = match req.json().await {
        Ok(b) => b,
        Err(e) => return json_error(400, &format!("bad body: {}", e)),
    };
    if payload.body.trim().is_empty() {
        return json_error(400, "comment empty");
    }

    let id = new_id();
    let now = now_secs();
    db(&ctx.env)?
        .prepare(
            "INSERT INTO comments (id, post_id, author_id, body, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(&[
            id.clone().into(),
            post_id.into(),
            session.user_id.clone().into(),
            payload.body.into(),
            now.into(),
        ])?
        .run()
        .await?;

    Response::ok(serde_json::json!({"id": id}).to_string()).map(|r| {
        let mut r = r;
        let _ = r.headers_mut().set("Content-Type", "application/json");
        r
    })
}

async fn load_reactions(
    db: &D1Database,
    post_id: &str,
    user_id: &str,
) -> Result<Vec<ReactionSummary>> {
    let rows = db
        .prepare(
            "SELECT emoji,
                    COUNT(*) as cnt,
                    SUM(CASE WHEN user_id = ?2 THEN 1 ELSE 0 END) as mine
             FROM reactions
             WHERE post_id = ?1
             GROUP BY emoji
             ORDER BY cnt DESC",
        )
        .bind(&[post_id.into(), user_id.into()])?
        .all()
        .await?;
    let raw: Vec<ReactionAggRow> = rows.results()?;
    Ok(raw
        .into_iter()
        .map(|r| ReactionSummary {
            emoji: r.emoji,
            count: r.cnt as u32,
            mine: r.mine.unwrap_or(0) > 0,
        })
        .collect())
}

async fn load_comment_count(db: &D1Database, post_id: &str) -> Result<u32> {
    let row = db
        .prepare("SELECT COUNT(*) as cnt FROM comments WHERE post_id = ?1")
        .bind(&[post_id.into()])?
        .first::<CountRow>(None)
        .await?;
    Ok(row.map(|r| r.cnt as u32).unwrap_or(0))
}

#[derive(serde::Deserialize)]
struct PostRow {
    id: String,
    body: Option<String>,
    image_key: Option<String>,
    video_key: Option<String>,
    media_keys: Option<String>,
    created_at: i64,
    author_id: String,
    display_name: String,
    avatar_key: Option<String>,
}

#[derive(serde::Deserialize)]
struct AuthorRow {
    author_id: String,
}

#[derive(serde::Deserialize)]
struct CommentRow {
    id: String,
    body: String,
    created_at: i64,
    author_id: String,
    display_name: String,
    avatar_key: Option<String>,
}

#[derive(serde::Deserialize)]
struct ReactionAggRow {
    emoji: String,
    cnt: i64,
    mine: Option<i64>,
}

#[derive(serde::Deserialize)]
struct CountRow {
    cnt: i64,
}
