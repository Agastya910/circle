use worker::*;

use crate::db::{circle_id, db};

// Web Push fan-out. For v1 we send "tickle" notifications without payload:
// the service worker wakes, shows a generic notification, and the client
// fetches the latest posts itself when tapped. This avoids needing RFC 8291
// payload encryption (ECDH + HKDF + AES-GCM) in the first pass.
//
// Upgrade path for encrypted payloads: add aes-gcm + hkdf crates and
// implement RFC 8291 aes128gcm content encoding.
pub async fn fan_out_post(env: &Env, author_id: &str) -> Result<()> {
    let rows = db(env)?
        .prepare(
            "SELECT push_sub FROM users
             WHERE circle_id = ?1 AND id != ?2 AND push_sub IS NOT NULL",
        )
        .bind(&[circle_id(env).into(), author_id.into()])?
        .all()
        .await?;

    let subs: Vec<PushRow> = rows.results()?;
    for sub in subs {
        if let Some(s) = sub.push_sub {
            if let Err(e) = send_tickle(env, &s).await {
                console_log!("push send failed: {:?}", e);
            }
        }
    }
    Ok(())
}

#[derive(serde::Deserialize)]
struct PushRow {
    push_sub: Option<String>,
}

#[derive(serde::Deserialize)]
struct Sub {
    endpoint: String,
}

async fn send_tickle(env: &Env, sub_json: &str) -> Result<()> {
    let sub: Sub = serde_json::from_str(sub_json)
        .map_err(|e| Error::RustError(format!("bad sub: {}", e)))?;

    let jwt = vapid_jwt(env, &sub.endpoint)?;
    let vapid_pub = env
        .secret("VAPID_PUBLIC_KEY")
        .map(|v| v.to_string())
        .map_err(|_| Error::RustError("VAPID_PUBLIC_KEY missing".into()))?;

    let mut headers = Headers::new();
    headers.set(
        "Authorization",
        &format!("vapid t={}, k={}", jwt, vapid_pub),
    )?;
    headers.set("TTL", "86400")?;
    headers.set("Content-Length", "0")?;
    headers.set("Urgency", "normal")?;

    let req = Request::new_with_init(
        &sub.endpoint,
        RequestInit::new()
            .with_method(Method::Post)
            .with_headers(headers),
    )?;

    let resp = Fetch::Request(req).send().await?;
    if resp.status_code() == 410 {
        db(env)?
            .prepare("UPDATE users SET push_sub = NULL WHERE push_sub = ?1")
            .bind(&[sub_json.into()])?
            .run()
            .await?;
    }
    Ok(())
}

// VAPID JWT: ES256-signed { aud, exp, sub } using the P-256 private key.
// VAPID_PRIVATE_KEY: base64url-encoded 32-byte raw scalar.
fn vapid_jwt(env: &Env, endpoint: &str) -> Result<String> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64U, Engine};
    use p256::ecdsa::{signature::Signer, Signature, SigningKey};

    let subject = env
        .secret("VAPID_SUBJECT")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "mailto:admin@example.com".into());

    let priv_b64 = env
        .secret("VAPID_PRIVATE_KEY")
        .map(|v| v.to_string())
        .map_err(|_| Error::RustError("VAPID_PRIVATE_KEY missing".into()))?;

    let d = B64U
        .decode(&priv_b64)
        .map_err(|e| Error::RustError(format!("priv key b64: {}", e)))?;

    let signing_key = SigningKey::from_bytes(d.as_slice().into())
        .map_err(|e| Error::RustError(format!("priv key parse: {}", e)))?;

    let audience = audience_of(endpoint)?;
    let exp = (crate::util::now_secs() + 12.0 * 60.0 * 60.0) as i64;

    let header_b64 = B64U.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
    let claims = serde_json::json!({
        "aud": audience,
        "exp": exp,
        "sub": subject,
    })
    .to_string();
    let claims_b64 = B64U.encode(claims.as_bytes());

    let signing_input = format!("{}.{}", header_b64, claims_b64);
    let sig: Signature = signing_key.sign(signing_input.as_bytes());
    let sig_b64 = B64U.encode(sig.to_bytes());

    Ok(format!("{}.{}", signing_input, sig_b64))
}

fn audience_of(endpoint: &str) -> Result<String> {
    let rest = endpoint
        .split_once("://")
        .ok_or_else(|| Error::RustError("bad endpoint".into()))?;
    let host = rest.1.split('/').next().unwrap_or("");
    Ok(format!("{}://{}", rest.0, host))
}
