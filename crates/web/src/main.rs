use leptos::prelude::*;
use notepad_core::{
    Card, Note, Point, Workspace,
    markdown::{self, Resolution},
};
use wasm_bindgen::{JsCast, closure::Closure};
use wasm_bindgen_futures::spawn_local;
use web_sys::{HtmlInputElement, PointerEvent};

mod canvas;
mod persistence;
mod sync;
fn uid() -> String {
    format!("{}-{}", js_sys::Date::now(), js_sys::Math::random())
}
fn point(e: &PointerEvent, zoom: f64) -> Point {
    let rect = document()
        .get_element_by_id("world")
        .unwrap()
        .get_bounding_client_rect();
    Point {
        x: ((e.client_x() as f64 - rect.x()) / zoom).clamp(-100000., 100000.),
        y: ((e.client_y() as f64 - rect.y()) / zoom).clamp(-100000., 100000.),
    }
}
#[component]
fn Icon(#[prop(into)] name: String) -> impl IntoView {
    let path = match name.as_str() {
        "rectangle" => "M4 4h16v16H4z",
        "diamond" => "m12 3 9 9-9 9-9-9z",
        "ellipse" => "M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0",
        "arrow" => "M4 20 20 4M9 4h11v11",
        "line" => "M4 20 20 4",
        "hand" => {
            "M8 13V6a2 2 0 0 1 4 0v6-8a2 2 0 0 1 4 0v8-5a2 2 0 0 1 4 0v9c0 5-4 7-8 7-3 0-5-3-7-6l-2-4c-1-3 2-4 4-1l1 1"
        }
        "eraser" => "m3 14 10-11 8 8-10 10H8zM8 9l8 8",
        "redo" => "m16 4 5 5-5 5M21 9H11a7 7 0 0 0 0 14",
        "lock" => "M5 10h14v11H5zM8 10V6a4 4 0 0 1 8 0v4",
        "plus" => "M12 5v14M5 12h14",
        "search" => "m21 21-5-5M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0",
        "note" => "M6 3h9l4 4v14H6zM14 3v5h5M9 12h7M9 16h5",
        "canvas" => "M3 4h18v16H3zM8 8h5v5H8zM15 14h3v3h-3z",
        "pen" => "m4 20 4-1L20 7l-4-4L4 15zM13 6l5 5",
        "cursor" => "m5 3 15 10-8 1-3 7z",
        "text" => "M4 5h16M12 5v15M8 20h8",
        "link" => {
            "m10 13 4-4M8 15l-2 2a4 4 0 0 1-5-5l5-5a4 4 0 0 1 6 0M16 9l2-2a4 4 0 0 1 5 5l-5 5a4 4 0 0 1-6 0"
        }
        "mail" => "M3 5h18v14H3zM3 5l9 7 9-7",
        "image" => "M3 3h18v18H3zM3 17l6-6 5 5 3-3 4 4M16 7h.01",
        "undo" => "M8 4 3 9l5 5M3 9h10a7 7 0 0 1 0 14",
        "download" => "M12 3v12m-5-5 5 5 5-5M4 16v5h16v-5",
        "save" => "M4 3h13l3 3v15H4zM8 3v6h8V3M8 21v-7h8v7",
        "close" => "m6 6 12 12M6 18 18 6",
        "sticky" => "M4 3h16v12l-5 6H4zM14 21v-7h6",
        "connect" => "M3 4h5v5H3zM16 15h5v5h-5zM8 6h5v12h3m-3-3 3 3-3 3",
        "resize" => "m7 17 10-10M10 7h7v7",
        "menu" => "M4 6h16M4 12h16M4 18h16",
        _ => "M5 12h14",
    };
    view! { <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d=path/></svg> }
}
#[derive(Clone, Copy)]
struct Editor {
    workspace: RwSignal<Workspace>,
    workspace_epoch: RwSignal<u64>,
    sync_base: RwSignal<Option<Workspace>>,
    active: RwSignal<usize>,
    status: RwSignal<String>,
    history: RwSignal<Vec<Workspace>>,
    future: RwSignal<Vec<Workspace>>,
    ready: RwSignal<bool>,
    local_status: RwSignal<String>,
    local_generation: RwSignal<u64>,
    local_saved: RwSignal<u64>,
    local_running: RwSignal<bool>,
}
impl Editor {
    fn create_note(self, title: String, body: String) -> Option<usize> {
        if !self.ready.get_untracked() {
            return None;
        }
        if self.workspace.with_untracked(|w| w.notes.len() >= 1000)
            || title.len() > 512
            || body.len() > markdown::MAX_MARKDOWN_BYTES
        {
            self.status
                .set("Note limit reached, or the note is too large.".into());
            return None;
        }
        self.snapshot();
        let mut index = 0;
        self.workspace.update(|w| {
            index = w.notes.len();
            w.notes.push(Note {
                id: uid(),
                title,
                body,
                cards: vec![],
                strokes: vec![],
                connections: vec![],
                shapes: vec![],
                ink: vec![],
            });
        });
        self.active.set(index);
        self.persist();
        Some(index)
    }
    fn rename(self, title: String) -> bool {
        let id = self
            .workspace
            .with_untracked(|w| w.notes[self.active.get_untracked()].id.clone());
        let result = self
            .workspace
            .try_update(|w| w.rename_note(&id, title))
            .unwrap_or(Err("Workspace unavailable"));
        match result {
            Ok(()) => {
                self.persist();
                true
            }
            Err(error) => {
                self.status.set(error.into());
                false
            }
        }
    }
    fn change(self, f: impl FnOnce(&mut Note)) {
        if !self.ready.get_untracked() {
            return;
        }
        self.workspace
            .update(|w| f(&mut w.notes[self.active.get_untracked()]));
        self.persist();
    }
    fn undo(self) {
        if !self.ready.get_untracked() {
            return;
        }
        if let Some(mut w) = self.history.try_update(|h| h.pop()).flatten() {
            self.future
                .update(|h| h.push(self.workspace.get_untracked()));
            w.revision = self.workspace.with_untracked(|current| current.revision);
            self.active.update(|i| *i = (*i).min(w.notes.len() - 1));
            self.workspace.set(w);
            self.workspace_epoch.update(|v| *v += 1);
            self.persist();
        }
    }
    fn redo(self) {
        if !self.ready.get_untracked() {
            return;
        }
        if let Some(mut w) = self.future.try_update(|h| h.pop()).flatten() {
            self.history
                .update(|h| h.push(self.workspace.get_untracked()));
            w.revision = self.workspace.with_untracked(|w| w.revision);
            self.active.update(|i| *i = (*i).min(w.notes.len() - 1));
            self.workspace.set(w);
            self.workspace_epoch.update(|v| *v += 1);
            self.persist();
        }
    }
    fn snapshot(self) {
        self.future.set(vec![]);
        self.history.update(|h| {
            h.push(self.workspace.get_untracked());
            if h.len() > 40 {
                h.remove(0);
            }
        });
    }
    fn persist(self) {
        self.local_generation.update(|v| *v += 1);
        self.local_status.set("Saving on this device…".into());
        self.status.set(String::new());
        if self.local_running.get_untracked() {
            return;
        }
        self.local_running.set(true);
        spawn_local(async move {
            loop {
                // Coalesce pointer and typing events, with only one write in flight.
                gloo_timers::future::TimeoutFuture::new(120).await;
                let generation = self.local_generation.get_untracked();
                let snapshot = self.workspace.get_untracked();
                match persistence::save(&snapshot, self.sync_base.get_untracked().as_ref()).await {
                    Ok(()) => {
                        self.local_saved.set(generation);
                        if self.local_generation.get_untracked() == generation {
                            self.local_status.set(
                                if persistence::is_desktop() {
                                    "Saved on this device"
                                } else {
                                    "Saved in this browser"
                                }
                                .into(),
                            );
                        }
                    }
                    Err(error) => {
                        self.local_status.set(error);
                        self.local_running.set(false);
                        return;
                    }
                }
                if self.local_generation.get_untracked() == generation {
                    break;
                }
            }
            self.local_running.set(false);
        });
    }
    fn add(self, kind: &str, text: String, x: f64, y: f64) {
        self.insert(
            self.workspace.get_untracked().notes[self.active.get_untracked()]
                .id
                .clone(),
            Card {
                id: uid(),
                kind: kind.into(),
                text,
                email: None,
                size: None,
                x: x.clamp(-100000., 100000.),
                y: y.clamp(-100000., 100000.),
            },
        );
    }
    fn insert(self, note_id: String, card: Card) {
        if !self.ready.get_untracked() {
            return;
        }
        if !self
            .workspace
            .get_untracked()
            .notes
            .iter()
            .any(|n| n.id == note_id)
        {
            self.status
                .set("The destination note no longer exists. Import the file again.".into());
            return;
        }
        if self.workspace.with_untracked(|w| {
            w.notes
                .iter()
                .find(|n| n.id == note_id)
                .is_some_and(|n| n.cards.len() >= 1000)
        }) {
            self.status.set("Card limit reached on this page.".into());
            return;
        }
        self.snapshot();
        self.workspace.update(|w| {
            if let Some(n) = w.notes.iter_mut().find(|n| n.id == note_id) {
                n.cards.push(card);
            }
        });
        self.persist();
    }
}
fn read_email(file: web_sys::File, editor: Editor, x: f64, y: f64) {
    if file.size() > notepad_core::MAX_EMAIL_BYTES as f64 {
        editor
            .status
            .set("Email too large. Choose an .eml file under 2 MB.".into());
        return;
    }
    let note_id = editor.workspace.get_untracked().notes[editor.active.get_untracked()]
        .id
        .clone();
    spawn_local(async move {
        match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
            Ok(buffer) => {
                match notepad_core::import_email(&js_sys::Uint8Array::new(&buffer).to_vec()) {
                    Ok((email, body)) => editor.insert(
                        note_id,
                        Card {
                            id: uid(),
                            kind: "email".into(),
                            text: body,
                            email: Some(email),
                            size: None,
                            x: x.clamp(-100000., 100000.),
                            y: y.clamp(-100000., 100000.),
                        },
                    ),
                    Err(error) => editor.status.set(error.into()),
                }
            }
            Err(_) => editor
                .status
                .set("Could not read this email. Try selecting the file again.".into()),
        }
    });
}
async fn markdown_file(file: web_sys::File) -> Result<(String, String), String> {
    if file.size() > markdown::MAX_MARKDOWN_BYTES as f64 {
        return Err("Markdown file is too large. Choose one under 1 MB.".into());
    }
    let buffer = wasm_bindgen_futures::JsFuture::from(file.array_buffer())
        .await
        .map_err(|_| "Could not read this Markdown file.".to_string())?;
    markdown::import_markdown(&file.name(), &js_sys::Uint8Array::new(&buffer).to_vec())
        .map_err(str::to_string)
}
fn read_markdown_drop(file: web_sys::File, editor: Editor, x: f64, y: f64) {
    let destination = editor
        .workspace
        .with_untracked(|w| w.notes[editor.active.get_untracked()].id.clone());
    spawn_local(async move {
        match markdown_file(file).await {
            Ok((title, body)) => {
                if !editor.ready.get_untracked() {
                    return;
                }
                if editor.workspace.with_untracked(|w| {
                    w.notes.len() >= 1000 || !w.notes.iter().any(|n| n.id == destination)
                }) {
                    editor
                        .status
                        .set("The destination is unavailable, or the notebook is full.".into());
                    return;
                }
                editor.snapshot();
                let id = uid();
                editor.workspace.update(|w| {
                    w.notes.push(Note {
                        id: id.clone(),
                        title,
                        body,
                        cards: vec![],
                        strokes: vec![],
                        connections: vec![],
                        shapes: vec![],
                        ink: vec![],
                    });
                    if let Some(note) = w.notes.iter_mut().find(|n| n.id == destination) {
                        note.cards.push(Card {
                            id: uid(),
                            kind: "note".into(),
                            text: id,
                            email: None,
                            size: None,
                            x: x.clamp(-100000., 100000.),
                            y: y.clamp(-100000., 100000.),
                        });
                    }
                });
                editor.persist();
            }
            Err(error) => editor.status.set(error),
        }
    });
}
fn read_capture(file: web_sys::File, editor: Editor, x: f64, y: f64) {
    if ["md", "markdown"].iter().any(|extension| {
        file.name()
            .to_ascii_lowercase()
            .ends_with(&format!(".{extension}"))
    }) {
        read_markdown_drop(file, editor, x, y);
    } else if file.name().to_ascii_lowercase().ends_with(".eml") || file.type_() == "message/rfc822"
    {
        read_email(file, editor, x, y);
    } else {
        read_image(file, editor, x, y);
    }
}

fn read_image(file: web_sys::File, editor: Editor, x: f64, y: f64) {
    if file.size() > 4_000_000. {
        editor
            .status
            .set("Image too large. Choose one under 4 MB.".into());
        return;
    }
    if !["image/png", "image/jpeg", "image/webp", "image/gif"].contains(&file.type_().as_str()) {
        editor
            .status
            .set("Choose a PNG, JPEG, WebP, or GIF image.".into());
        return;
    }
    let note_id = editor.workspace.get_untracked().notes[editor.active.get_untracked()]
        .id
        .clone();
    spawn_local(async move {
        match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
            Ok(buffer) => {
                use base64::Engine;
                let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
                let text = format!(
                    "data:{};base64,{}",
                    file.type_(),
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                );
                editor.insert(
                    note_id,
                    Card {
                        id: uid(),
                        kind: "image".into(),
                        text,
                        email: None,
                        size: None,
                        x: x.clamp(-100000., 100000.),
                        y: y.clamp(-100000., 100000.),
                    },
                );
            }
            Err(_) => editor
                .status
                .set("Could not read this image. Try selecting the file again.".into()),
        }
    });
}

fn download_text(text: &str, filename: &str) -> Result<(), String> {
    let parts = js_sys::Array::new();
    parts.push(&text.into());
    let blob = web_sys::Blob::new_with_str_sequence(&parts).map_err(|_| "Export failed")?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|_| "Export failed")?;
    let anchor: web_sys::HtmlAnchorElement =
        document().create_element("a").unwrap().unchecked_into();
    anchor.set_href(&url);
    anchor.set_download(filename);
    anchor.click();
    web_sys::Url::revoke_object_url(&url).ok();
    Ok(())
}
fn current_route() -> (Option<String>, &'static str) {
    let hash = web_sys::window()
        .unwrap()
        .location()
        .hash()
        .unwrap_or_default();
    let hash = hash.trim_start_matches('#');
    let view = hash
        .split('&')
        .find_map(|part| part.strip_prefix("view="))
        .unwrap_or(hash);
    let view = match view {
        "write" => "write",
        "read" => "read",
        _ => "canvas",
    };
    let id = hash
        .split('&')
        .find_map(|part| part.strip_prefix("note="))
        .and_then(markdown::decode_id);
    (id, view)
}
fn download(w: Workspace) -> Result<(), String> {
    download_text(
        &serde_json::to_string_pretty(&w).map_err(|e| e.to_string())?,
        "notepad-backup.json",
    )
}
#[component]
fn App() -> impl IntoView {
    let editor = Editor {
        workspace: RwSignal::new(Workspace::default()),
        workspace_epoch: RwSignal::new(0),
        sync_base: RwSignal::new(None),
        active: RwSignal::new(0),
        status: RwSignal::new("Opening workspace…".into()),
        history: RwSignal::new(vec![]),
        future: RwSignal::new(vec![]),
        ready: RwSignal::new(false),
        local_status: RwSignal::new("Opening local draft…".into()),
        local_generation: RwSignal::new(0),
        local_saved: RwSignal::new(0),
        local_running: RwSignal::new(false),
    };
    let e = editor;
    let mode = RwSignal::new(current_route().1);
    let query = RwSignal::new(String::new());
    let help = RwSignal::new(false);
    let menu = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let remote_enabled = RwSignal::new(!persistence::is_desktop());
    let remote_url = RwSignal::new(String::new());
    let auth = RwSignal::new(None::<notepad_core::AuthState>);
    let auth_open = RwSignal::new(false);
    let auth_error = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let empty_cache = RwSignal::new(false);
    let tab_blocked = RwSignal::new(String::new());
    let remote_epoch = RwSignal::new(0u64);
    let sync = sync::Sync::new(e, remote_enabled, auth, busy, remote_epoch);
    sync.start();
    let close_editor = e;
    let before_close = Closure::<dyn FnMut(web_sys::Event)>::new(move |ev: web_sys::Event| {
        if close_editor.local_generation.get_untracked() != close_editor.local_saved.get_untracked()
        {
            ev.prevent_default();
            js_sys::Reflect::set(&ev, &"returnValue".into(), &"Unsaved local changes".into()).ok();
        }
    });
    web_sys::window()
        .unwrap()
        .add_event_listener_with_callback("beforeunload", before_close.as_ref().unchecked_ref())
        .ok();
    before_close.forget();
    let picker = RwSignal::new(None::<&'static str>);
    let picked = RwSignal::new(String::new());
    let insertion = RwSignal::new((u32::MAX, u32::MAX));
    Effect::new(move |_| {
        let _ = e.active.get();
        let _ = mode.get();
        let _ = e.workspace_epoch.get();
        picker.set(None);
    });
    let search_results = Memo::new(move |_| {
        let query = query.get();
        if e.ready.get() {
            e.workspace
                .with(|w| markdown::search_notes(&w.notes, &query))
        } else {
            vec![]
        }
    });
    let note_catalog = Memo::new(move |_| e.workspace.with(|w| markdown::catalog(&w.notes)));
    let source = Memo::new(move |_| e.workspace.with(|w| w.notes[e.active.get()].body.clone()));
    let rendered =
        Memo::new(move |_| markdown::render_markdown(&source.get(), &note_catalog.get()));
    let route_applied = RwSignal::new(false);
    Effect::new(move |_| {
        if !e.ready.get() {
            return;
        }
        if !route_applied.get() {
            let (id, view) = current_route();
            if let Some(id) = id {
                if let Some(index) = e
                    .workspace
                    .with_untracked(|w| w.notes.iter().position(|n| n.id == id))
                {
                    e.active.set(index);
                } else {
                    e.status
                        .set("The requested note was not found in this workspace.".into());
                }
            }
            mode.set(view);
            route_applied.set(true);
            return;
        }
        let active = e.active.get();
        let view = mode.get();
        let id = e.workspace.with(|w| w.notes[active].id.clone());
        let hash = markdown::note_href(&id, view);
        let location = web_sys::window().unwrap().location();
        if location.hash().unwrap_or_default() != hash {
            location.set_hash(hash.trim_start_matches('#')).ok();
        }
    });
    let hash_change = Closure::<dyn FnMut()>::new(move || {
        if !e.ready.get_untracked() {
            return;
        }
        let (id, view) = current_route();
        if let Some(id) = id
            && let Some(index) = e
                .workspace
                .with_untracked(|w| w.notes.iter().position(|n| n.id == id))
            && e.active.get_untracked() != index
        {
            e.active.set(index);
        }
        if mode.get_untracked() != view {
            mode.set(view);
        }
    });
    web_sys::window()
        .unwrap()
        .add_event_listener_with_callback("hashchange", hash_change.as_ref().unchecked_ref())
        .ok();
    hash_change.forget();
    let open_note = move |id: &str| {
        if let Some(index) = e
            .workspace
            .with_untracked(|w| w.notes.iter().position(|n| n.id == id))
        {
            query.set(String::new());
            e.active.set(index);
            mode.set("read");
            menu.set(false);
        } else {
            e.status.set("That note is not in this workspace.".into());
        }
    };
    let note_click = move |ev: leptos::ev::MouseEvent| {
        if ev.ctrl_key() || ev.meta_key() || ev.shift_key() || ev.button() != 0 {
            return;
        }
        let Some(element) = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            .and_then(|el| {
                el.closest("[data-note-id], [data-create-note]")
                    .ok()
                    .flatten()
            })
        else {
            return;
        };
        if let Some(id) = element.get_attribute("data-note-id") {
            ev.prevent_default();
            open_note(&id);
        } else if let Some(title) = element.get_attribute("data-create-note") {
            ev.prevent_default();
            let index = note_catalog.get_untracked();
            match markdown::resolve_title(&index, &title) {
                Resolution::Found(note) => open_note(&note.id),
                Resolution::Ambiguous => e
                    .status
                    .set("Several notes share that title. Choose one from the note list.".into()),
                Resolution::Missing => {
                    if e.create_note(title, String::new()).is_some() {
                        mode.set("write");
                        query.set(String::new());
                    }
                }
            }
        }
    };
    let reload_server = move |_| {
        if busy.get_untracked()
            || sync.running.get_untracked()
            || !tab_blocked.get_untracked().is_empty()
        {
            return;
        }
        if !web_sys::window().unwrap().confirm_with_message("Replace this browser copy with the server copy? Export first to keep any unsaved changes.").unwrap_or(false) { return; }
        busy.set(true);
        let generation = e.local_generation.get_untracked();
        let epoch = remote_epoch.get_untracked();
        spawn_local(async move {
            match persistence::load_server().await {
                Ok(w) => {
                    if e.local_generation.get_untracked() != generation
                        || remote_epoch.get_untracked() != epoch
                    {
                        e.status.set("You edited while the server copy was loading. Local work kept; load again when ready.".into());
                        busy.set(false);
                        return;
                    }
                    e.active.set(0);
                    sync.conflicts.set(0);
                    sync.status.set(String::new());
                    e.sync_base.set(Some(w.clone()));
                    e.workspace.set(w);
                    e.workspace_epoch.update(|v| *v += 1);
                    e.history.set(vec![]);
                    e.future.set(vec![]);
                    e.ready.set(true);
                    e.persist();
                }
                Err(error) => {
                    e.status.set(error);
                    if let Ok(state) = persistence::auth_state().await {
                        auth.set(Some(state));
                    }
                }
            }
            busy.set(false);
        });
    };
    spawn_local(async move {
        match persistence::acquire_editor().await {
            Ok(true) => {}
            other => {
                let message = match other {
                    Ok(false) => "Your notebook is open in another tab. Close that tab, then reload here to continue.".to_string(),
                    Err(error) => error,
                    _ => unreachable!(),
                };
                tab_blocked.set(message);
                e.status.set(String::new());
                e.local_status
                    .set("Editing paused to protect this browser's draft".into());
                return;
            }
        }
        if persistence::is_desktop() {
            match persistence::invoke::<Option<String>>("connection", &serde_json::json!({})).await
            {
                Ok(url) => {
                    remote_enabled.set(url.is_some());
                    remote_url.set(url.unwrap_or_default());
                }
                Err(error) => {
                    e.local_status.set(error);
                    return;
                }
            }
        }
        match persistence::load().await {
            Ok(Some(w)) => {
                match persistence::load_sync_base().await {
                    Ok(base) => e.sync_base.set(base.filter(|b| b.revision == w.revision)),
                    Err(error) => e.status.set(error),
                }
                e.workspace.set(w);
                e.workspace_epoch.update(|v| *v += 1);
                e.ready.set(true);
                e.local_status.set(
                    if persistence::is_desktop() {
                        "Device copy restored"
                    } else {
                        "Browser copy restored"
                    }
                    .into(),
                );
                e.status.set(String::new());
            }
            Ok(None) => empty_cache.set(true),
            Err(error) => {
                e.local_status.set(error);
                e.status.set(String::new());
            }
        }
        match persistence::auth_state().await {
            Ok(state) => {
                let authenticated = state.authenticated;
                auth.set(Some(state));
                if empty_cache.get_untracked() {
                    if authenticated {
                        match persistence::load_server().await {
                            Ok(w) => {
                                e.sync_base.set(Some(w.clone()));
                                e.workspace.set(w);
                                e.workspace_epoch.update(|v| *v += 1);
                                e.ready.set(true);
                                e.persist();
                            }
                            Err(error) => {
                                e.ready.set(true);
                                e.persist();
                                e.status.set(error);
                            }
                        }
                    } else {
                        e.status.set(String::new());
                        e.local_status
                            .set("Sign in to open your server notebook".into());
                    }
                }
            }
            Err(error) => {
                if empty_cache.get_untracked() {
                    e.ready.set(true);
                    e.persist();
                }
                e.status.set(error);
            }
        }
    });
    let sign_in = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        auth_error.set(String::new());
        let setup = auth.get_untracked().is_some_and(|a| !a.configured);
        let entered = password.get_untracked();
        password.set(String::new());
        spawn_local(async move {
            let result = if setup {
                persistence::setup(entered).await
            } else {
                persistence::login(entered).await
            };
            match result {
                Ok(()) => {
                    auth.set(Some(notepad_core::AuthState {
                        configured: true,
                        authenticated: true,
                    }));
                    auth_open.set(false);
                    if empty_cache.get_untracked() && !e.ready.get_untracked() {
                        match persistence::load_server().await {
                            Ok(w) => {
                                e.sync_base.set(Some(w.clone()));
                                e.workspace.set(w);
                                e.workspace_epoch.update(|v| *v += 1);
                                e.ready.set(true);
                                e.persist();
                            }
                            Err(error) => e.status.set(error),
                        }
                    } else {
                        e.status.set(
                            "Signed in. Sync will check the server and keep your local edits."
                                .into(),
                        );
                    }
                }
                Err(error) => {
                    auth_error.set(error);
                    if let Ok(state) = persistence::auth_state().await {
                        auth.set(Some(state));
                    }
                }
            }
            busy.set(false);
            sync.request(false);
        });
    };
    let sign_out = move |_| {
        if busy.get_untracked() {
            return;
        }
        let message = if persistence::is_desktop() {
            "Sign out of the server? Your local desktop draft will stay on this device."
        } else {
            "Sign out and remove this browser's local copy? Save to server or export unsynced notes first."
        };
        if !web_sys::window()
            .unwrap()
            .confirm_with_message(message)
            .unwrap_or(false)
        {
            return;
        }
        busy.set(true);
        e.status.set("Signing out…".into());

        let was_ready = e.ready.get_untracked();
        e.ready.set(false);
        remote_epoch.update(|v| *v += 1);
        spawn_local(async move {
            while e.local_running.get_untracked() {
                gloo_timers::future::TimeoutFuture::new(20).await;
            }
            match persistence::logout().await {
                Ok(()) => {
                    auth.set(Some(notepad_core::AuthState {
                        configured: true,
                        authenticated: false,
                    }));
                    if persistence::is_desktop() {
                        e.ready.set(was_ready);
                        e.status
                            .set("Signed out. Desktop draft kept on this device.".into());
                    } else {
                        match persistence::clear_browser_draft().await {
                            Ok(()) => {
                                e.active.set(0);
                                e.history.set(vec![]);
                                e.future.set(vec![]);
                                e.sync_base.set(None);
                                sync.status.set(String::new());
                                sync.conflicts.set(0);
                                e.workspace.set(Workspace::default());
                                e.workspace_epoch.update(|v| *v += 1);
                                e.local_saved.set(e.local_generation.get_untracked());
                                empty_cache.set(true);
                                e.local_status.set("Browser copy removed".into());
                                e.status.set("Signed out".into());
                                if let Some(storage) =
                                    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
                                {
                                    storage.set_item("notepad.logout", &uid()).ok();
                                }
                            }
                            Err(error) => {
                                e.ready.set(was_ready);
                                e.status.set(format!(
                                    "Signed out on the server, but local removal failed: {error}"
                                ));
                            }
                        }
                    }
                }
                Err(error) => {
                    e.ready.set(was_ready);
                    e.status.set(error);
                }
            }
            busy.set(false);
        });
    };
    let on_logout =
        Closure::<dyn FnMut(web_sys::StorageEvent)>::new(move |event: web_sys::StorageEvent| {
            if event.key().as_deref() != Some("notepad.logout") {
                return;
            }
            e.ready.set(false);
            remote_epoch.update(|v| *v += 1);
            auth.set(Some(notepad_core::AuthState {
                configured: true,
                authenticated: false,
            }));
            spawn_local(async move {
                while e.local_running.get_untracked() {
                    gloo_timers::future::TimeoutFuture::new(20).await;
                }
                match persistence::clear_browser_draft().await {
                    Ok(()) => {
                        e.active.set(0);
                        e.history.set(vec![]);
                        e.future.set(vec![]);
                        e.sync_base.set(None);
                        sync.status.set(String::new());
                        sync.conflicts.set(0);
                        e.workspace.set(Workspace::default());
                        e.workspace_epoch.update(|v| *v += 1);
                        e.local_saved.set(e.local_generation.get_untracked());
                        empty_cache.set(true);
                        e.local_status.set("Browser copy removed".into());
                        e.status
                            .set("Signed out in another tab. Browser copy removed.".into());
                    }
                    Err(error) => e.status.set(error),
                }
            });
        });
    web_sys::window()
        .unwrap()
        .add_event_listener_with_callback("storage", on_logout.as_ref().unchecked_ref())
        .ok();
    on_logout.forget();
    let save = move |_| {
        if !remote_enabled.get_untracked() {
            e.persist();
        } else {
            sync.request(false);
        }
    };
    view! {
        <div class="app-shell" class:drawing-mode=move ||mode.get()=="canvas" data-theme=move ||if mode.get()=="canvas"{"drawing"}else{"notes"} on:click=note_click class:menu-open=move || menu.get()>
            <aside class="sidebar" aria-label="Workspace">
                <a class="brand" href="#canvas"><span class="brand-mark"><Icon name="note"/></span>"notepad"<span class="personal">"Personal"</span></a>
                <div class="workspace-name"><span class="avatar">"O"</span><div><strong>"My workspace"</strong><small>"A little room to think"</small></div></div>
                <label class="search"><Icon name="search"/><input placeholder="Find a note…" aria-label="Find a note" prop:value=move || query.get() on:input=move |ev| query.set(event_target_value(&ev))/></label>
                <div class="notes-heading"><span>"Your notes"</span><button title="New note" aria-label="New note" disabled=move || !e.ready.get() on:click=move |_| {
                    e.create_note("Untitled note".into(),String::new());mode.set("write");menu.set(false);
                }><Icon name="plus"/></button></div>
                <nav class="note-list" aria-label="Notes">
                {move || search_results.get().into_iter().map(|note| {
                    let id=note.id.clone();let selected_id=id.clone();let title=note.title;
                    view! { <button class="note-row" class:current=move ||e.workspace.with(|w|w.notes[e.active.get()].id==selected_id) on:click=move |_| {if let Some(i)=e.workspace.with_untracked(|w|w.notes.iter().position(|n|n.id==id)){e.active.set(i);}menu.set(false);} ><Icon name="note"/><span>{if title.is_empty(){"Untitled note".into()}else{title}}</span></button> }
                }).collect_view()}
                <Show when=move ||e.ready.get()&&search_results.get().is_empty()><p class="no-results">"No notes found. Try another search."</p></Show>
                </nav>
                <button class="new-note" disabled=move || !e.ready.get() on:click=move |_| { e.create_note("Untitled note".into(),String::new());mode.set("write");menu.set(false); }><Icon name="plus"/>"New note"</button>
                <div class="sidebar-bottom"><div class="backup-actions"><label class="import-picker">"Import Markdown"<input type="file" accept=".md,.markdown,text/markdown" aria-label="Import Markdown" disabled=move ||busy.get()||!e.ready.get() on:change=move|ev|{
                    let input:HtmlInputElement=event_target(&ev);if let Some(file)=input.files().and_then(|fs|fs.get(0)){spawn_local(async move{match markdown_file(file).await{Ok((title,body))=>{if e.create_note(title,body).is_some(){mode.set("read");query.set(String::new());menu.set(false);}},Err(error)=>e.status.set(error)}});}input.set_value("");
                }/></label><button disabled=move || busy.get() || sync.running.get() || !tab_blocked.get().is_empty() on:click=reload_server class:hidden-control=move || !remote_enabled.get()>"Load server copy"</button><label class="import-picker">"Import backup"<input type="file" accept="application/json,.json" aria-label="Import backup" disabled=move || busy.get() || sync.running.get() || !tab_blocked.get().is_empty() on:change=move |ev| {
                    let input:HtmlInputElement=event_target(&ev);
                    if let Some(file)=input.files().and_then(|f|f.get(0)) {
                        if file.size()>12_000_000. {e.status.set("Backup too large. Maximum size is 12 MB.".into());return;}
                        spawn_local(async move {
                            match wasm_bindgen_futures::JsFuture::from(file.text()).await.ok().and_then(|v|v.as_string()).and_then(|s|serde_json::from_str::<Workspace>(&s).ok()).and_then(|w|w.upgrade().ok()) {
                                Some(mut w) => {
                                    if busy.get_untracked()||sync.running.get_untracked()||!tab_blocked.get_untracked().is_empty(){e.status.set("Import paused during sign-in or sign-out. Choose the backup again.".into());return;}
                                    if web_sys::window().unwrap().confirm_with_message("Replace this browser workspace with the backup? Export first to keep your current notes.").unwrap_or(false) {
                                        w.revision=e.workspace.get_untracked().revision; e.snapshot(); e.active.set(0);e.workspace.set(w);e.workspace_epoch.update(|v|*v+=1);e.ready.set(true);e.persist();
                                    }
                                },
                                _ => e.status.set("Invalid backup. Choose a Notepad JSON export.".into()),
                            }
                        });
                    }
                    input.set_value("");
                }/></label></div><div class="local-info"><Icon name="save"/><div><strong>"Local workspace"</strong><small>{move || if !remote_url.get().is_empty() {remote_url.get()} else if persistence::is_desktop() {"Desktop notebook".into()} else {"Your notes. Your server.".into()}}</small></div></div><button class="help-link" on:click=move |_| help.update(|v| *v = !*v)>"Getting comfortable"<span>"?"</span></button></div>
            </aside>
            <main>
                <header class="topbar"><button class="mobile-menu" aria-label="Toggle notes" on:click=move |_| menu.update(|v| *v = !*v)><Icon name="menu"/></button><div class="breadcrumb">"My workspace"<span>"/"</span><strong>{move || if e.ready.get(){e.workspace.get().notes[e.active.get()].title.clone()}else{"Notebook".into()}}</strong></div><Show when=move || remote_enabled.get()&&tab_blocked.get().is_empty()><button class="session-control" disabled=move || busy.get() on:click=move |_| auth_open.update(|v|*v = !*v) class:hidden-control=move || auth.get().is_some() || auth_open.get()>"Sign in"</button><button class="session-control" class:hidden-control=move || !auth.get().is_some_and(|a|a.authenticated) disabled=move || busy.get() on:click=sign_out>"Sign out"</button></Show><button class="export" disabled=move || !e.ready.get() title="Export workspace as JSON" on:click=move |_| { if let Err(err)=download(e.workspace.get_untracked()) {e.status.set(err);} }><Icon name="download"/><span>"Export"</span></button><button class="primary" disabled=move || busy.get() || sync.running.get() || !e.ready.get() on:click=save><Icon name="save"/>{move || if busy.get() || sync.running.get() {"Syncing…"} else if remote_enabled.get() {"Save to server"} else {"Save locally"}}</button></header>
                <Show when=move || remote_enabled.get() && tab_blocked.get().is_empty() && (auth_open.get() || auth.get().is_some_and(|a|!a.authenticated))>
                    <section class="auth-panel" aria-label="Server sign-in">
                        <div><h2>{move ||if auth.get().is_some_and(|a|!a.configured){"Create your notebook"}else{"Sign in to your notebook"}}</h2><p>{move ||if auth.get().is_some_and(|a|!a.configured){"Choose a password. No minimum length or complexity rules."}else{"Local drafts stay on this device while you reconnect."}}</p></div>
                        <form on:submit=sign_in><label for="owner-password">"Password"</label><div class="auth-fields"><input id="owner-password" type="password" autocomplete=move ||if auth.get().is_some_and(|a|!a.configured){"new-password"}else{"current-password"} prop:value=move ||password.get() on:input=move |ev|password.set(event_target_value(&ev)) disabled=move ||busy.get()/><button class="primary" type="submit" disabled=move ||busy.get()>{move ||if busy.get(){"Opening…"}else if auth.get().is_some_and(|a|!a.configured){"Create notebook"}else{"Sign in"}}</button></div><p class="auth-error" role="status" aria-live="polite">{move ||auth_error.get()}</p></form>
                    </section>
                </Show>
                <Show when=move || e.ready.get() fallback=move ||view!{<div class="locked-workspace"><p>{move ||if !tab_blocked.get().is_empty(){tab_blocked.get()}else if auth.get().is_some_and(|a|!a.authenticated){"Sign in to open your notebook.".into()}else if auth.get().is_some(){"Load your server copy or import a backup to open your notebook.".into()}else{"Opening your notebook…".into()}}</p></div>}>
                <div class="note-header"><div class="note-identity"><Icon name="note"/><input aria-label="Note title" maxlength="160" disabled=move || !e.ready.get() prop:value=move || e.workspace.get().notes[e.active.get()].title.clone() on:focus=move |_| e.snapshot() on:input=move |ev| {let before=e.workspace.with_untracked(|w|w.notes[e.active.get_untracked()].title.clone());if !e.rename(event_target_value(&ev)){let input:HtmlInputElement=event_target(&ev);input.set_value(&before);}}/></div><div class="view-tabs" aria-label="Note view"><button class:active=move || mode.get()=="write" aria-pressed=move || mode.get()=="write" on:click=move |_| {mode.set("write");}><Icon name="note"/>"Write"</button><button class:active=move || mode.get()=="read" aria-pressed=move ||mode.get()=="read" on:click=move |_|mode.set("read")>"Read"</button><button class:active=move || mode.get()=="canvas" aria-pressed=move || mode.get()=="canvas" on:click=move |_| {mode.set("canvas");}><Icon name="canvas"/>"Canvas"</button></div></div>
                <Show when=move || help.get()><div class="help-panel"><strong>"A page when you need focus. A canvas when you need space."</strong><p>"Write in Markdown. On the canvas, add a card, drag its header to move it, or use the pen to draw. Drop images, .eml emails, web links, or plain text onto the board. Changes save on this device and sync automatically while signed in. If two devices edit the same page, keep both versions before syncing continues. Export keeps a portable backup."</p><button on:click=move |_| help.set(false)>"Got it"</button></div></Show>
                <Show when=move ||picker.get().is_some()><form class="note-picker" on:submit=move |ev|{
                    ev.prevent_default();let id=picked.get_untracked();let index=note_catalog.get_untracked();let Some(note)=index.iter().find(|n|n.id==id)else{e.status.set("Choose a note first.".into());return;};
                    if picker.get_untracked()==Some("link"){
                        let link=markdown::markdown_link(note);let(start,end)=insertion.get_untracked();let mut body=e.workspace.with_untracked(|w|w.notes[e.active.get_untracked()].body.clone());
                        match markdown::insert_link_at(&mut body,&link,start,end){Ok(())=>{e.snapshot();e.change(|n|n.body=body);},Err(error)=>{e.status.set(error.into());return;}}
                    }else{e.add("note",id,440.,260.);}
                    picker.set(None);
                }><label for="linked-note">"Note"</label><select id="linked-note" required prop:value=move ||picked.get() on:change=move|ev|{let select:web_sys::HtmlSelectElement=event_target(&ev);picked.set(select.value());}><option value="">"Choose a note"</option>{move ||note_catalog.get().into_iter().map(|n|view!{<option value=n.id>{n.title}</option>}).collect_view()}</select><button type="submit" class="primary">{move ||if picker.get()==Some("link"){"Insert link"}else{"Add to canvas"}}</button><button type="button" on:click=move |_|picker.set(None)>"Cancel"</button></form></Show>
                <Show when=move || mode.get()=="read">
                    <section class="reading">
                        <Show when=move ||!source.get().trim().is_empty() fallback=move ||view!{<div class="empty-reading"><p>"This page is yours to fill."</p><button on:click=move |_|mode.set("write")>"Start writing"</button></div>}>
                            <article class="markdown-body" aria-label="Rendered note" inner_html=move ||rendered.get()></article>
                        </Show>
                        <section class="backlinks" aria-label="Linked from"><h2>"Linked from"</h2>{move ||{
                            let links=e.workspace.with(|w|markdown::backlinks(&w.notes,&w.notes[e.active.get()].id));
                            if links.is_empty(){view!{<p>"No other notes link here yet."</p>}.into_any()}else{view!{<ul>{links.into_iter().map(|note|view!{<li><a data-note-id=note.id.clone() href=markdown::note_href(&note.id,"read")>{note.title}</a></li>}).collect_view()}</ul>}.into_any()}
                        }}</section>
                    </section>
                </Show>
                <Show when=move || mode.get()=="write">
                    <section class="writing"><div class="writing-meta"><span>"Markdown"</span><div class="writing-actions"><button on:click=move |_|{picked.set(String::new());let range=document().query_selector(".document").ok().flatten().and_then(|el|el.dyn_into::<web_sys::HtmlTextAreaElement>().ok()).map(|el|(el.selection_start().ok().flatten().unwrap_or(u32::MAX),el.selection_end().ok().flatten().unwrap_or(u32::MAX))).unwrap_or((u32::MAX,u32::MAX));insertion.set(range);picker.set(Some("link"));}>"Link a note"</button><button on:click=move |_|{let id=e.workspace.with_untracked(|w|w.notes[e.active.get_untracked()].id.clone());e.add("note",id,150.,150.);mode.set("canvas");}>"Pin page"</button><button on:click=move |_|{let note=e.workspace.with_untracked(|w|w.notes[e.active.get_untracked()].clone());if let Err(error)=download_text(&note.body,&markdown::markdown_filename(&note.title)){e.status.set(error);}}>"Export .md"</button></div></div><textarea class="document" maxlength="1000000" aria-label="Note content in Markdown" placeholder="Start with a thought…" disabled=move || !e.ready.get() prop:value=move || e.workspace.get().notes[e.active.get()].body.clone() on:focus=move |_| {picker.set(None);e.snapshot();} on:input=move |ev| e.change(|n| n.body=event_target_value(&ev))></textarea><div class="writing-footer">{move || format!("{} words", e.workspace.get().notes[e.active.get()].body.split_whitespace().count())}<span>"Make a little space for an idea."</span></div></section>
                </Show>
                <Show when=move || mode.get()=="canvas">
                    <canvas::Canvas e=e picker=picker picked=picked note_catalog=note_catalog/>
                </Show>
                </Show>
                <Show when=move ||{sync.conflicts.get()>0}><div class="sync-conflict" role="alert"><p>"Some pages have edits from another device. Keep both versions to preserve each copy, then review the pages in your note list."</p><button class="primary" disabled=move ||busy.get()||sync.running.get() on:click=move |_|sync.request(true)>"Keep both versions"</button></div></Show>
                <footer class="statusbar" data-local-state=move || if !e.ready.get() {"opening"} else if e.local_generation.get()==e.local_saved.get() {"saved"} else if e.local_running.get() {"pending"} else {"error"}><span class="status" role="status" aria-live="polite">{move || {let message=e.status.get();if message.is_empty(){let remote=sync.status.get();if remote.is_empty(){e.local_status.get()}else{format!("{} / {}",remote,e.local_status.get())}}else{format!("{} / {}",message,e.local_status.get())}}}</span></footer>
            </main>
        </div>
    }
}
fn main() {
    leptos::mount::mount_to_body(App);
}
