use worker::*;

pub fn cors_headers(env: &Env, h: &mut Headers) -> Result<()> {
    let origin = env
        .var("ALLOWED_ORIGIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "*".into());
    h.set("Access-Control-Allow-Origin", &origin)?;
    h.set("Access-Control-Allow-Credentials", "true")?;
    h.set("Access-Control-Allow-Methods", "GET,POST,PUT,OPTIONS")?;
    h.set(
        "Access-Control-Allow-Headers",
        "Content-Type,Authorization",
    )?;
    Ok(())
}

pub fn json_error(status: u16, msg: &str) -> Result<Response> {
    let body = serde_json::json!({ "error": msg }).to_string();
    let mut r = Response::ok(body)?.with_status(status);
    r.headers_mut().set("Content-Type", "application/json")?;
    Ok(r)
}

pub fn now_secs() -> f64 {
    (Date::now().as_millis() / 1000) as f64
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
