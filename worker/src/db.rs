use worker::*;

pub fn db(env: &Env) -> Result<D1Database> {
    env.d1("DB")
}

pub fn circle_id(env: &Env) -> String {
    env.var("CIRCLE_ID")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "default".into())
}
