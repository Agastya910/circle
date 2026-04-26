use worker::*;

mod auth;
mod db;
mod handlers;
mod push;
mod util;


use util::{cors_headers, json_error};

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    // CORS preflight
    if req.method() == Method::Options {
        let mut resp = Response::empty()?;
        cors_headers(&env, resp.headers_mut())?;
        return Ok(resp);
    }

    let router = Router::new();

    let res = router
        .post_async("/api/auth/register", handlers::auth::register)
        .post_async("/api/auth/login", handlers::auth::login)
        .post_async("/api/auth/logout", handlers::auth::logout)
        .get_async("/api/me", handlers::auth::me)
        .post_async("/api/admin/invite", handlers::admin::create_invite)
        .get_async("/api/posts", handlers::posts::list)
        .post_async("/api/posts", handlers::posts::create)
        .get_async("/api/posts/:id/comments", handlers::posts::comments)
        .post_async("/api/posts/:id/comments", handlers::posts::add_comment)
        .post_async("/api/reactions", handlers::reactions::toggle)
        .post_async("/api/upload-url", handlers::media::upload_url)
        .get_async("/api/media/:key", handlers::media::fetch)
        .put_async("/api/media/:key", handlers::media::fetch)
        .post_async("/api/push/subscribe", handlers::push::subscribe)
        .get_async("/api/push/vapid", handlers::push::vapid_public)
        .run(req, env.clone())
        .await;

    match res {
        Ok(mut r) => {
            cors_headers(&env, r.headers_mut())?;
            security_headers(r.headers_mut())?;
            Ok(r)
        }
        Err(e) => {
            let mut r = json_error(500, &format!("internal: {}", e))?;
            cors_headers(&env, r.headers_mut())?;
            Ok(r)
        }
    }
}

fn security_headers(h: &mut Headers) -> Result<()> {
    h.set("X-Content-Type-Options", "nosniff")?;
    h.set("X-Frame-Options", "DENY")?;
    h.set("Referrer-Policy", "strict-origin-when-cross-origin")?;
    h.set(
        "Permissions-Policy",
        "camera=(), microphone=(), geolocation=()",
    )?;
    Ok(())
}
