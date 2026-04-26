use gloo_storage::{LocalStorage, Storage};
use leptos::prelude::*;

const TOKEN_KEY: &str = "circle_token";
const USER_ID_KEY: &str = "circle_user_id";

pub fn load_token() -> Option<String> {
    LocalStorage::get::<String>(TOKEN_KEY).ok()
}

pub fn save_token(t: &str) {
    let _ = LocalStorage::set(TOKEN_KEY, t);
}

pub fn clear_token() {
    LocalStorage::delete(TOKEN_KEY);
}

pub fn load_user_id() -> Option<String> {
    LocalStorage::get::<String>(USER_ID_KEY).ok()
}

pub fn save_user_id(id: &str) {
    let _ = LocalStorage::set(USER_ID_KEY, id);
}

pub fn clear_user_id() {
    LocalStorage::delete(USER_ID_KEY);
}

#[derive(Clone, Copy)]
pub struct AuthCtx {
    pub token: RwSignal<Option<String>>,
    pub user_id: RwSignal<Option<String>>,
}

impl AuthCtx {
    pub fn new() -> Self {
        Self {
            token: RwSignal::new(load_token()),
            user_id: RwSignal::new(load_user_id()),
        }
    }

    pub fn set(&self, t: String, uid: String) {
        save_token(&t);
        save_user_id(&uid);
        self.token.set(Some(t));
        self.user_id.set(Some(uid));
    }

    pub fn logout(&self) {
        clear_token();
        clear_user_id();
        self.token.set(None);
        self.user_id.set(None);
    }
}
