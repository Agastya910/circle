use gloo_storage::{LocalStorage, Storage};
use leptos::prelude::*;

const TOKEN_KEY: &str = "circle_token";

pub fn load_token() -> Option<String> {
    LocalStorage::get::<String>(TOKEN_KEY).ok()
}

pub fn save_token(t: &str) {
    let _ = LocalStorage::set(TOKEN_KEY, t);
}

pub fn clear_token() {
    LocalStorage::delete(TOKEN_KEY);
}

#[derive(Clone, Copy)]
pub struct AuthCtx {
    pub token: RwSignal<Option<String>>,
}

impl AuthCtx {
    pub fn new() -> Self {
        Self {
            token: RwSignal::new(load_token()),
        }
    }
    pub fn set(&self, t: String) {
        save_token(&t);
        self.token.set(Some(t));
    }
    pub fn logout(&self) {
        clear_token();
        self.token.set(None);
    }
}
