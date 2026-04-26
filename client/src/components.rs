use circle_shared::{Comment, LoginRequest, NewPostRequest, Post, RegisterRequest};
use leptos::prelude::*;
use leptos::html;

use crate::api;
use crate::state::AuthCtx;

#[component]
pub fn App() -> impl IntoView {
    let auth = AuthCtx::new();
    provide_context(auth);

    wasm_bindgen_futures::spawn_local(async {
        register_service_worker().await;
    });

    view! {
        <div class="app">
            <Show
                when=move || auth.token.get().is_some()
                fallback=move || view! { <Login /> }
            >
                <Home />
            </Show>
        </div>
    }
}

#[component]
fn Login() -> impl IntoView {
    let auth = expect_context::<AuthCtx>();
    let join_mode = RwSignal::new(false);
    let invite = RwSignal::new(String::new());
    let name = RwSignal::new(String::new());
    let pin = RwSignal::new(String::new());
    let err = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

    let do_submit = move || {
        if busy.get() { return; }
        let n = name.get();
        let p = pin.get();
        if n.trim().is_empty() || p.len() < 4 {
            err.set(Some("name and a 4+ character PIN required".into()));
            return;
        }
        err.set(None);
        busy.set(true);

        if join_mode.get() {
            let i = invite.get();
            if i.trim().is_empty() {
                err.set(Some("invite code required".into()));
                busy.set(false);
                return;
            }
            wasm_bindgen_futures::spawn_local(async move {
                match api::register(RegisterRequest { invite_code: i, display_name: n, pin: p }).await {
                    Ok(a) => auth.set(a.token, a.user.id),
                    Err(e) => {
                        err.set(Some(if e.contains("403") {
                            "invite code is invalid or already used".into()
                        } else {
                            e
                        }));
                        busy.set(false);
                    }
                }
            });
        } else {
            wasm_bindgen_futures::spawn_local(async move {
                match api::login(LoginRequest { display_name: n, pin: p }).await {
                    Ok(a) => auth.set(a.token, a.user.id),
                    Err(e) => {
                        err.set(Some(if e.contains("401") {
                            "name or PIN incorrect".into()
                        } else {
                            e
                        }));
                        busy.set(false);
                    }
                }
            });
        }
    };
    let submit = move |_: web_sys::MouseEvent| do_submit();

    view! {
        <div class="login">
            <h2>"Circle"</h2>
            <div class="login-tabs">
                <button
                    class=move || if !join_mode.get() { "tab active" } else { "tab" }
                    on:click=move |_| { join_mode.set(false); err.set(None); }
                >"sign in"</button>
                <button
                    class=move || if join_mode.get() { "tab active" } else { "tab" }
                    on:click=move |_| { join_mode.set(true); err.set(None); }
                >"join with invite"</button>
            </div>

            {move || join_mode.get().then(|| view! {
                <input
                    placeholder="invite code"
                    on:input=move |ev| invite.set(event_target_value(&ev))
                />
            })}

            <input
                placeholder="your name"
                on:input=move |ev| name.set(event_target_value(&ev))
            />
            <input
                type="password"
                placeholder=move || if join_mode.get() { "choose a PIN (4+ chars)" } else { "PIN" }
                on:input=move |ev| pin.set(event_target_value(&ev))
                on:keydown=move |ev: web_sys::KeyboardEvent| { if ev.key() == "Enter" { do_submit(); } }
            />
            <button class="btn" on:click=submit disabled=move || busy.get()>
                {move || if busy.get() { "…" } else if join_mode.get() { "join" } else { "sign in" }}
            </button>
            {move || err.get().map(|e| view! { <p class="error">{e}</p> })}
        </div>
    }
}

#[component]
fn Home() -> impl IntoView {
    let auth = expect_context::<AuthCtx>();
    let posts = RwSignal::new(Vec::<Post>::new());
    let loading = RwSignal::new(true);

    let reload = move || {
        if let Some(t) = auth.token.get() {
            wasm_bindgen_futures::spawn_local(async move {
                match api::list_posts(&t).await {
                    Ok(p) => { posts.set(p); loading.set(false); }
                    Err(_) => { loading.set(false); }
                }
            });
        }
    };

    Effect::new(move |_| { reload(); });

    let on_posted = move |p: Post| {
        posts.update(|list| list.insert(0, p));
    };

    let on_deleted = move |id: String| {
        posts.update(|list| list.retain(|p| p.id != id));
    };

    let logout = move |_| { auth.logout(); };

    let enable_push = move |_| {
        if let Some(t) = auth.token.get() {
            wasm_bindgen_futures::spawn_local(async move {
                if let Err(e) = enable_web_push(&t).await {
                    leptos::logging::warn!("push: {}", e);
                }
            });
        }
    };

    view! {
        <div class="header">
            <h1>"Circle"</h1>
            <div style="display:flex;gap:8px">
                <button class="btn btn-ghost" on:click=enable_push title="enable notifications">"🔔"</button>
                <button class="btn btn-ghost" on:click=logout>"leave"</button>
            </div>
        </div>
        <Compose on_posted=on_posted />
        <Show
            when=move || loading.get()
            fallback=move || view! {
                <Show
                    when=move || !posts.get().is_empty()
                    fallback=move || view! {
                        <div class="empty">
                            <p>"nothing new."</p>
                            <p>"you're caught up."</p>
                        </div>
                    }
                >
                    <For
                        each=move || posts.get()
                        key=|p| p.id.clone()
                        children=move |p| view! { <PostCard post=p on_deleted=on_deleted /> }
                    />
                </Show>
            }
        >
            <div class="empty"><p>"loading…"</p></div>
        </Show>
    }
}

#[component]
fn Compose(on_posted: impl Fn(Post) + 'static + Copy) -> impl IntoView {
    let auth = expect_context::<AuthCtx>();
    let body = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let err = RwSignal::new(None::<String>);

    // Multi-image: up to 4 files + their object-URL previews.
    let pending_files: RwSignal<Vec<web_sys::File>> = RwSignal::new(vec![]);
    let preview_urls: RwSignal<Vec<String>> = RwSignal::new(vec![]);

    // Single video (still one at a time; mutually exclusive with images).
    let pending_video: RwSignal<Option<web_sys::Blob>> = RwSignal::new(None);
    let video_preview_url: RwSignal<Option<String>> = RwSignal::new(None);

    let compressing = RwSignal::new(false);
    let file_input_ref = NodeRef::<html::Input>::new();
    let video_input_ref = NodeRef::<html::Input>::new();

    let revoke_image_previews = move || {
        for url in preview_urls.get() {
            let _ = web_sys::Url::revoke_object_url(&url);
        }
        preview_urls.set(vec![]);
        pending_files.set(vec![]);
    };

    let revoke_video_preview = move || {
        if let Some(url) = video_preview_url.get() {
            let _ = web_sys::Url::revoke_object_url(&url);
        }
        video_preview_url.set(None);
        pending_video.set(None);
    };

    let on_image_change = move |ev: web_sys::Event| {
        use wasm_bindgen::JsCast;
        let input = ev.target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok());
        let files_list = match input.and_then(|i| i.files()) {
            Some(f) => f,
            None => return,
        };

        let mut new_files = pending_files.get();
        let mut new_urls = preview_urls.get();
        let slots_left = 4usize.saturating_sub(new_files.len());

        for i in 0..files_list.length().min(slots_left as u32) {
            if let Some(file) = files_list.item(i) {
                if let Ok(url) = web_sys::Url::create_object_url_with_blob(&file) {
                    new_urls.push(url);
                }
                new_files.push(file);
            }
        }

        revoke_video_preview();
        if let Some(el) = video_input_ref.get() { el.set_value(""); }

        pending_files.set(new_files);
        preview_urls.set(new_urls);
        err.set(None);

        if let Some(el) = file_input_ref.get() { el.set_value(""); }
    };

    let on_video_change = move |ev: web_sys::Event| {
        use wasm_bindgen::JsCast;
        let input = ev.target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok());
        let file = match input.and_then(|i| i.files()).and_then(|fs| fs.item(0)) {
            Some(f) => f,
            None => return,
        };

        revoke_image_previews();
        if let Some(el) = file_input_ref.get() { el.set_value(""); }

        revoke_video_preview();
        err.set(None);
        compressing.set(true);

        wasm_bindgen_futures::spawn_local(async move {
            match compress_video(file).await {
                Ok(blob) => {
                    if let Ok(u) = web_sys::Url::create_object_url_with_blob(&blob) {
                        video_preview_url.set(Some(u));
                    }
                    pending_video.set(Some(blob));
                }
                Err(e) => err.set(Some(e)),
            }
            compressing.set(false);
        });
    };

    let remove_image = move |idx: usize| {
        let mut files = pending_files.get();
        let mut urls = preview_urls.get();
        if idx < urls.len() {
            let _ = web_sys::Url::revoke_object_url(&urls[idx]);
            urls.remove(idx);
        }
        if idx < files.len() { files.remove(idx); }
        pending_files.set(files);
        preview_urls.set(urls);
    };

    let clear_video = move |_: web_sys::MouseEvent| { revoke_video_preview(); };

    let submit = move |_: web_sys::MouseEvent| {
        if busy.get() || compressing.get() { return; }
        let b = body.get();
        let has_images = !pending_files.get().is_empty();
        let has_video = pending_video.get().is_some();
        if b.trim().is_empty() && !has_images && !has_video { return; }
        let token = match auth.token.get() { Some(t) => t, None => return };
        busy.set(true);
        err.set(None);

        wasm_bindgen_futures::spawn_local(async move {
            let mut media_keys: Vec<String> = Vec::new();
            let mut video_key: Option<String> = None;

            for file in pending_files.get() {
                match api::get_upload_url(&token, "image", None).await {
                    Ok(u) => match api::upload_blob(&token, &u.key, file.into()).await {
                        Ok(()) => media_keys.push(u.key),
                        Err(e) => {
                            err.set(Some(format!("image upload failed: {}", e)));
                            busy.set(false);
                            return;
                        }
                    },
                    Err(e) => {
                        err.set(Some(format!("upload URL failed: {}", e)));
                        busy.set(false);
                        return;
                    }
                }
            }

            if let Some(blob) = pending_video.get() {
                let ext = if blob.type_().starts_with("video/mp4") { "mp4" } else { "webm" };
                match api::get_upload_url(&token, "video", Some(ext)).await {
                    Ok(u) => match api::upload_blob(&token, &u.key, blob).await {
                        Ok(()) => video_key = Some(u.key),
                        Err(e) => {
                            err.set(Some(format!("video upload failed: {}", e)));
                            busy.set(false);
                            return;
                        }
                    },
                    Err(e) => {
                        err.set(Some(format!("upload URL failed: {}", e)));
                        busy.set(false);
                        return;
                    }
                }
            }

            let req = NewPostRequest {
                body: if b.trim().is_empty() { None } else { Some(b) },
                image_key: None,
                video_key,
                media_keys,
            };
            match api::create_post(&token, req).await {
                Ok(p) => {
                    on_posted(p);
                    body.set(String::new());
                    revoke_image_previews();
                    revoke_video_preview();
                    if let Some(el) = file_input_ref.get() { el.set_value(""); }
                    if let Some(el) = video_input_ref.get() { el.set_value(""); }
                }
                Err(e) => err.set(Some(e)),
            }
            busy.set(false);
        });
    };

    let can_add_image = move || pending_files.get().len() < 4 && pending_video.get().is_none();

    view! {
        <div class="compose">
            <textarea
                placeholder="what's happening?"
                prop:value=move || body.get()
                on:input=move |ev| body.set(event_target_value(&ev))
            />

            // Image thumbnails row (up to 4).
            {move || {
                let urls = preview_urls.get();
                if urls.is_empty() {
                    None
                } else {
                    Some(view! {
                        <div class="compose-images">
                            {urls.into_iter().enumerate().map(|(idx, url)| {
                                view! {
                                    <div class="compose-thumb">
                                        <img src=url.clone() class="compose-thumb-img" />
                                        <button
                                            class="compose-clear-img"
                                            on:click=move |_| remove_image(idx)
                                        >"✕"</button>
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    })
                }
            }}

            // Video preview.
            {move || video_preview_url.get().map(|url| {
                view! {
                    <div class="compose-preview">
                        <video src=url class="compose-preview-img" controls=true muted=true playsinline=true></video>
                        <button class="compose-clear-img" on:click=clear_video>"✕"</button>
                    </div>
                }
            })}

            {move || compressing.get().then(|| view! {
                <p class="hint">"compressing video…"</p>
            })}

            <div class="compose-actions">
                <div style="display:flex; gap:6px">
                    <button
                        class="btn btn-ghost btn-icon"
                        on:click=move |_| { if can_add_image() { if let Some(el) = file_input_ref.get() { el.click(); } } }
                        disabled=move || busy.get() || compressing.get() || !can_add_image()
                        title=move || { if pending_files.get().len() >= 4 { "max 4 images" } else { "attach photo(s)" } }
                    >"📷"</button>
                    <button
                        class="btn btn-ghost btn-icon"
                        on:click=move |_| { if let Some(el) = video_input_ref.get() { el.click(); } }
                        disabled=move || busy.get() || compressing.get() || !pending_files.get().is_empty()
                        title="attach video (≤15s)"
                    >"🎥"</button>
                </div>
                <input
                    type="file"
                    accept="image/*"
                    multiple=true
                    style="display:none"
                    node_ref=file_input_ref
                    on:change=on_image_change
                />
                <input
                    type="file"
                    accept="video/*"
                    style="display:none"
                    node_ref=video_input_ref
                    on:change=on_video_change
                />
                <button class="btn" on:click=submit disabled=move || busy.get() || compressing.get()>
                    {move || if busy.get() { "posting…" } else if compressing.get() { "…" } else { "post" }}
                </button>
            </div>
            {move || err.get().map(|e| view! { <p class="error">{e}</p> })}
        </div>
    }
}

async fn compress_video(file: web_sys::File) -> Result<web_sys::Blob, String> {
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;

    let win = web_sys::window().ok_or("no window")?;
    let f = js_sys::Reflect::get(&win, &JsValue::from_str("compressVideo"))
        .map_err(|e| format!("{:?}", e))?;
    let f: js_sys::Function = f.dyn_into().map_err(|_| "video.js not loaded")?;

    // (file, maxDurationSec=15, targetBitrate=800kbps, maxWidth=640)
    let promise = f.call3(
        &JsValue::NULL,
        &JsValue::from(file),
        &JsValue::from_f64(15.0),
        &JsValue::from_f64(800_000.0),
    ).map_err(|e| format!("{:?}", e))?;

    let promise: js_sys::Promise = promise.dyn_into()
        .map_err(|_| "compressVideo did not return a Promise")?;
    let result = JsFuture::from(promise).await
        .map_err(|e| e.as_string().unwrap_or_else(|| format!("{:?}", e)))?;

    let blob: web_sys::Blob = result.dyn_into()
        .map_err(|_| "compressVideo did not yield a Blob")?;
    Ok(blob)
}

#[component]
fn PostCard(post: Post, on_deleted: impl Fn(String) + Send + Sync + 'static + Copy) -> impl IntoView {
    let auth = expect_context::<AuthCtx>();
    let post = RwSignal::new(post);
    let show_comments = RwSignal::new(false);
    let comments: RwSignal<Vec<Comment>> = RwSignal::new(vec![]);
    let comments_loaded = RwSignal::new(false);
    let comment_text = RwSignal::new(String::new());
    let comment_busy = RwSignal::new(false);
    let carousel_idx: RwSignal<usize> = RwSignal::new(0);
    let delete_busy = RwSignal::new(false);

    let load_comments = move || {
        let pid = post.get().id.clone();
        if let Some(t) = auth.token.get() {
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(cs) = api::list_comments(&t, &pid).await {
                    comments.set(cs);
                    comments_loaded.set(true);
                }
            });
        }
    };

    let toggle_comments = move |_| {
        let was_open = show_comments.get();
        show_comments.set(!was_open);
        if !was_open && !comments_loaded.get() {
            load_comments();
        }
    };

    let do_comment = move || {
        if comment_busy.get() { return; }
        let text = comment_text.get();
        if text.trim().is_empty() { return; }
        let pid = post.get().id.clone();
        let token = match auth.token.get() { Some(t) => t, None => return };
        comment_busy.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            if api::add_comment(&token, &pid, &text).await.is_ok() {
                comment_text.set(String::new());
                post.update(|p| p.comment_count += 1);
                if let Ok(cs) = api::list_comments(&token, &pid).await {
                    comments.set(cs);
                }
            }
            comment_busy.set(false);
        });
    };
    let submit_comment = move |_: web_sys::MouseEvent| do_comment();

    let react = move |emoji: &'static str| {
        let pid = post.get().id.clone();
        let token = match auth.token.get() { Some(t) => t, None => return };
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = api::react(&token, &pid, emoji).await {
                leptos::logging::warn!("react: {}", e);
                return;
            }
            post.update(|p| {
                if let Some(r) = p.reactions.iter_mut().find(|r| r.emoji == emoji) {
                    if r.mine {
                        r.mine = false;
                        r.count = r.count.saturating_sub(1);
                    } else {
                        r.mine = true;
                        r.count += 1;
                    }
                } else {
                    p.reactions.push(circle_shared::ReactionSummary {
                        emoji: emoji.to_string(),
                        count: 1,
                        mine: true,
                    });
                }
                p.reactions.retain(|r| r.count > 0);
            });
        });
    };

    let do_delete = move |_: web_sys::MouseEvent| {
        if delete_busy.get() { return; }
        let pid = post.get().id.clone();
        let token = match auth.token.get() { Some(t) => t, None => return };
        delete_busy.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            match api::delete_post(&token, &pid).await {
                Ok(()) => on_deleted(pid),
                Err(e) => leptos::logging::warn!("delete failed: {}", e),
            }
            delete_busy.set(false);
        });
    };

    let is_mine = move || {
        auth.user_id.get().map(|uid| uid == post.get().author.id).unwrap_or(false)
    };

    let emojis = ["❤️", "😂", "🔥", "👏", "🙏", "😢"];

    view! {
        <div class="post">
            <div class="post-header">
                <span class="post-author">{move || post.get().author.display_name}</span>
                <span class="post-time">{move || format_time(post.get().created_at)}</span>
                {move || is_mine().then(|| view! {
                    <button
                        class="btn btn-ghost btn-sm post-delete"
                        on:click=do_delete
                        disabled=move || delete_busy.get()
                        title="delete post"
                    >
                        {move || if delete_busy.get() { "…" } else { "🗑" }}
                    </button>
                })}
            </div>
            {move || post.get().body.map(|b| view! { <p class="post-body">{b}</p> })}

            // Multi-image carousel (new media_keys path).
            {move || {
                let keys = post.get().media_keys;
                if keys.is_empty() {
                    None
                } else {
                    let count = keys.len();
                    Some(view! {
                        <div class="post-carousel">
                            {move || {
                                let idx = carousel_idx.get().min(count - 1);
                                let key = keys[idx].clone();
                                let token = auth.token.get().unwrap_or_default();
                                let src = format!("{}/api/media/{}?t={}", api::base(), key, token);
                                view! { <img class="post-image" src=src /> }
                            }}
                            {(count > 1).then(|| view! {
                                <div class="carousel-controls">
                                    <button
                                        class="carousel-btn"
                                        on:click=move |_| carousel_idx.update(|i| { if *i > 0 { *i -= 1; } })
                                        disabled=move || carousel_idx.get() == 0
                                    >"‹"</button>
                                    <span class="carousel-dot">
                                        {move || format!("{}/{}", carousel_idx.get() + 1, count)}
                                    </span>
                                    <button
                                        class="carousel-btn"
                                        on:click=move |_| carousel_idx.update(|i| { if *i + 1 < count { *i += 1; } })
                                        disabled=move || carousel_idx.get() + 1 >= count
                                    >"›"</button>
                                </div>
                            })}
                        </div>
                    })
                }
            }}

            // Legacy single image_key (old posts before multi-photo).
            {move || {
                if !post.get().media_keys.is_empty() {
                    None
                } else {
                    post.get().image_key.map(|k| {
                        let token = auth.token.get().unwrap_or_default();
                        let src = format!("{}/api/media/{}?t={}", api::base(), k, token);
                        view! { <img class="post-image" src=src /> }
                    })
                }
            }}

            // Video.
            {move || post.get().video_key.map(|k| {
                let token = auth.token.get().unwrap_or_default();
                let src = format!("{}/api/media/{}?t={}", api::base(), k, token);
                view! {
                    <video
                        class="post-video"
                        src=src
                        controls=true
                        playsinline=true
                        preload="metadata"
                    ></video>
                }
            })}

            <div class="reactions">
                {emojis.iter().map(|e| {
                    let e = *e;
                    view! {
                        <button
                            class=move || {
                                let mine = post.get().reactions.iter()
                                    .find(|r| r.emoji == e).map(|r| r.mine).unwrap_or(false);
                                if mine { "reaction mine" } else { "reaction" }
                            }
                            on:click=move |_| react(e)
                        >
                            {e}
                            {move || {
                                let c = post.get().reactions.iter()
                                    .find(|r| r.emoji == e).map(|r| r.count).unwrap_or(0);
                                if c > 0 { format!(" {}", c) } else { String::new() }
                            }}
                        </button>
                    }
                }).collect::<Vec<_>>()}
            </div>
            <button class="comments-toggle" on:click=toggle_comments>
                {move || {
                    let n = post.get().comment_count;
                    if show_comments.get() {
                        "hide comments".to_string()
                    } else if n == 0 {
                        "comment".to_string()
                    } else if n == 1 {
                        "1 comment".to_string()
                    } else {
                        format!("{} comments", n)
                    }
                }}
            </button>
            <Show when=move || show_comments.get()>
                <div class="comments">
                    <For
                        each=move || comments.get()
                        key=|c| c.id.clone()
                        children=|c| view! { <CommentItem comment=c /> }
                    />
                    <div class="comment-compose">
                        <input
                            class="comment-input"
                            placeholder="add a comment…"
                            prop:value=move || comment_text.get()
                            on:input=move |ev| comment_text.set(event_target_value(&ev))
                            on:keydown=move |ev: web_sys::KeyboardEvent| {
                                if ev.key() == "Enter" && !ev.shift_key() {
                                    ev.prevent_default();
                                    do_comment();
                                }
                            }
                        />
                        <button
                            class="btn btn-sm"
                            on:click=submit_comment
                            disabled=move || comment_busy.get()
                        >
                            {move || if comment_busy.get() { "…" } else { "send" }}
                        </button>
                    </div>
                </div>
            </Show>
        </div>
    }
}

#[component]
fn CommentItem(comment: Comment) -> impl IntoView {
    view! {
        <div class="comment">
            <div class="comment-header">
                <span class="comment-author">{comment.author.display_name}</span>
                <span class="comment-time">{format_time(comment.created_at)}</span>
            </div>
            <p class="comment-body">{comment.body}</p>
        </div>
    }
}

fn format_time(ts: i64) -> String {
    let now = js_sys::Date::now() as i64 / 1000;
    let diff = (now - ts).max(0);
    if diff < 60 { "just now".into() }
    else if diff < 3600 { format!("{}m ago", diff / 60) }
    else if diff < 86400 { format!("{}h ago", diff / 3600) }
    else { format!("{}d ago", diff / 86400) }
}

async fn register_service_worker() {
    use wasm_bindgen_futures::JsFuture;
    let win = match web_sys::window() { Some(w) => w, None => return };
    let _ = JsFuture::from(win.navigator().service_worker().register("/sw.js")).await;
}

async fn enable_web_push(token: &str) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let vapid = api::vapid_key(token).await?;
    let win = web_sys::window().ok_or("no window")?;

    let perm = JsFuture::from(
        web_sys::Notification::request_permission().map_err(|e| format!("{:?}", e))?
    ).await.map_err(|e| format!("{:?}", e))?;
    if perm.as_string().as_deref() != Some("granted") {
        return Err("notification permission denied".into());
    }

    let reg_js = JsFuture::from(
        win.navigator().service_worker().ready().map_err(|e| format!("{:?}", e))?
    ).await.map_err(|e| format!("{:?}", e))?;
    let reg: web_sys::ServiceWorkerRegistration = reg_js.dyn_into().map_err(|_| "bad reg")?;
    let pm = reg.push_manager().map_err(|e| format!("{:?}", e))?;

    let key_arr = b64url_to_u8array(&vapid)?;
    let mut opts = web_sys::PushSubscriptionOptionsInit::new();
    opts.set_user_visible_only(true);
    opts.set_application_server_key(key_arr.as_ref());

    let sub_js = JsFuture::from(
        pm.subscribe_with_options(&opts).map_err(|e| format!("{:?}", e))?
    ).await.map_err(|e| format!("{:?}", e))?;
    let sub: web_sys::PushSubscription = sub_js.dyn_into().map_err(|_| "bad sub")?;

    let endpoint = sub.endpoint();
    let p256dh = extract_key(&sub, "p256dh")?;
    let auth_k = extract_key(&sub, "auth")?;
    api::subscribe_push(token, &endpoint, &p256dh, &auth_k).await
}

fn b64url_to_u8array(s: &str) -> Result<js_sys::Uint8Array, String> {
    let padding = match s.len() % 4 { 0 => 0, r => 4 - r };
    let mut padded = s.replace('-', "+").replace('_', "/");
    for _ in 0..padding { padded.push('='); }
    let win = web_sys::window().ok_or("no window")?;
    let decoded = win.atob(&padded).map_err(|e| format!("{:?}", e))?;
    let bytes: Vec<u8> = decoded.chars().map(|c| c as u8).collect();
    let arr = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
    arr.copy_from(&bytes);
    Ok(arr)
}

fn extract_key(sub: &web_sys::PushSubscription, name: &str) -> Result<String, String> {
    let key_name = match name {
        "p256dh" => web_sys::PushEncryptionKeyName::P256dh,
        "auth" => web_sys::PushEncryptionKeyName::Auth,
        _ => return Err("unknown key".into()),
    };
    let buf = sub.get_key(key_name)
        .map_err(|e| format!("{:?}", e))?
        .ok_or("no key")?;
    let u8arr = js_sys::Uint8Array::new(&buf);
    let mut v = vec![0u8; u8arr.length() as usize];
    u8arr.copy_to(&mut v);
    let win = web_sys::window().unwrap();
    let s: String = v.iter().map(|b| *b as char).collect();
    let std = win.btoa(&s).unwrap();
    Ok(std.replace('+', "-").replace('/', "_").trim_end_matches('=').to_string())
}
