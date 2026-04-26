use circle_shared::{AuthResponse, Comment, LoginRequest, NewPostRequest, Post, ReactRequest, RegisterRequest, UploadUrlRequest, UploadUrlResponse, User};
use gloo_net::http::Request;
use serde::{de::DeserializeOwned, Serialize};

// Set this at build time: `API_BASE=https://circle-api.<sub>.workers.dev trunk build --release`
// For dev with wrangler dev, leave default.
const API_BASE: Option<&str> = option_env!("API_BASE");

pub fn base() -> &'static str {
    API_BASE.unwrap_or("http://127.0.0.1:8787")
}

pub async fn register(req: RegisterRequest) -> Result<AuthResponse, String> {
    post_json("/api/auth/register", &req, None).await
}

pub async fn login(req: LoginRequest) -> Result<AuthResponse, String> {
    post_json("/api/auth/login", &req, None).await
}

pub async fn me(token: &str) -> Result<User, String> {
    get_json("/api/me", Some(token)).await
}

pub async fn list_posts(token: &str) -> Result<Vec<Post>, String> {
    get_json("/api/posts", Some(token)).await
}

pub async fn create_post(token: &str, req: NewPostRequest) -> Result<Post, String> {
    post_json("/api/posts", &req, Some(token)).await
}

pub async fn react(token: &str, post_id: &str, emoji: &str) -> Result<(), String> {
    let body = ReactRequest {
        post_id: post_id.to_string(),
        emoji: emoji.to_string(),
    };
    let _: serde_json::Value = post_json("/api/reactions", &body, Some(token)).await?;
    Ok(())
}

pub async fn list_comments(token: &str, post_id: &str) -> Result<Vec<Comment>, String> {
    get_json(&format!("/api/posts/{}/comments", post_id), Some(token)).await
}

pub async fn add_comment(token: &str, post_id: &str, body: &str) -> Result<(), String> {
    #[derive(serde::Serialize)]
    struct Body<'a> { body: &'a str }
    let _: serde_json::Value = post_json(
        &format!("/api/posts/{}/comments", post_id),
        &Body { body },
        Some(token),
    ).await?;
    Ok(())
}

pub async fn vapid_key(token: &str) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Resp { vapid_public_key: String }
    let r: Resp = get_json("/api/push/vapid", Some(token)).await?;
    Ok(r.vapid_public_key)
}

pub async fn get_upload_url(
    token: &str,
    kind: &str,
    ext: Option<&str>,
) -> Result<UploadUrlResponse, String> {
    post_json(
        "/api/upload-url",
        &UploadUrlRequest {
            kind: kind.to_string(),
            ext: ext.map(|s| s.to_string()),
        },
        Some(token),
    ).await
}

pub async fn upload_blob(token: &str, key: &str, blob: web_sys::Blob) -> Result<(), String> {
    use wasm_bindgen::JsValue;
    let resp = Request::put(&format!("{}/api/media/{}", base(), key))
        .header("Authorization", &format!("Bearer {}", token))
        .body(JsValue::from(blob))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        let status = resp.status();
        let msg = resp.text().await.unwrap_or_default();
        return Err(format!("upload failed (HTTP {}): {}", status, msg));
    }
    Ok(())
}

pub async fn delete_post(token: &str, post_id: &str) -> Result<(), String> {
    let resp = gloo_net::http::Request::delete(&format!("{}/api/posts/{}", base(), post_id))
        .header("Authorization", &format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(())
}

pub async fn subscribe_push(
    token: &str,
    endpoint: &str,
    p256dh: &str,
    auth: &str,
) -> Result<(), String> {
    let body = circle_shared::PushSubscribeRequest {
        endpoint: endpoint.to_string(),
        keys_p256dh: p256dh.to_string(),
        keys_auth: auth.to_string(),
    };
    let _: serde_json::Value = post_json("/api/push/subscribe", &body, Some(token)).await?;
    Ok(())
}

async fn get_json<T: DeserializeOwned>(path: &str, token: Option<&str>) -> Result<T, String> {
    let mut req = Request::get(&format!("{}{}", base(), path));
    if let Some(t) = token {
        req = req.header("Authorization", &format!("Bearer {}", t));
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<T>().await.map_err(|e| e.to_string())
}

async fn post_json<I: Serialize, O: DeserializeOwned>(
    path: &str,
    body: &I,
    token: Option<&str>,
) -> Result<O, String> {
    let mut req = Request::post(&format!("{}{}", base(), path));
    if let Some(t) = token {
        req = req.header("Authorization", &format!("Bearer {}", t));
    }
    let resp = req
        .json(body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<O>().await.map_err(|e| e.to_string())
}
