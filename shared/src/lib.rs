use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub display_name: String,
    pub avatar_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: String,
    pub author: User,
    pub body: Option<String>,
    pub image_key: Option<String>,
    pub video_key: Option<String>,
    pub created_at: i64,
    pub reactions: Vec<ReactionSummary>,
    pub comment_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactionSummary {
    pub emoji: String,
    pub count: u32,
    pub mine: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub author: User,
    pub body: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewPostRequest {
    pub body: Option<String>,
    pub image_key: Option<String>,
    pub video_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadUrlRequest {
    /// "image" or "video"
    pub kind: String,
    /// Optional file extension chosen by the client (e.g. "mp4", "webm").
    /// Worker validates against an allowlist per kind. Defaults: image=webp,
    /// video=webm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ext: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactRequest {
    pub post_id: String,
    pub emoji: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub invite_code: String,
    pub display_name: String,
    /// 4-character minimum PIN chosen by the user at registration.
    /// Used to sign in on new devices without a new invite code.
    pub pin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub display_name: String,
    pub pin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: User,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadUrlResponse {
    pub upload_url: String,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushSubscribeRequest {
    pub endpoint: String,
    pub keys_p256dh: String,
    pub keys_auth: String,
}
