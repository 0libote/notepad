use indexed_db_futures::{database::Database, prelude::*, transaction::TransactionMode};
use notepad_core::Workspace;
use wasm_bindgen::{JsCast, JsValue};

const LEGACY: &str = "notepad.workspace.v1";
const STORE: &str = "drafts";

pub fn is_desktop() -> bool {
    js_sys::Reflect::has(&web_sys::window().unwrap(), &"__TAURI__".into()).unwrap_or(false)
}
pub async fn invoke<T: serde::de::DeserializeOwned>(
    command: &str,
    args: &impl serde::Serialize,
) -> Result<T, String> {
    let window = web_sys::window().ok_or("Window unavailable")?;
    let tauri = js_sys::Reflect::get(&window, &"__TAURI__".into()).map_err(js_error)?;
    let core = js_sys::Reflect::get(&tauri, &"core".into()).map_err(js_error)?;
    let invoke: js_sys::Function = js_sys::Reflect::get(&core, &"invoke".into())
        .map_err(js_error)?
        .dyn_into()
        .map_err(js_error)?;
    let args = args
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| e.to_string())?;
    let promise: js_sys::Promise = invoke
        .call2(&core, &command.into(), &args)
        .map_err(js_error)?
        .dyn_into()
        .map_err(js_error)?;
    let value = wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(js_error)?;
    serde_wasm_bindgen::from_value(value).map_err(|e| e.to_string())
}
fn js_error(e: JsValue) -> String {
    e.as_string().unwrap_or_else(|| format!("{e:?}"))
}
fn db_error(e: impl std::fmt::Display) -> String {
    format!("Local storage unavailable: {e}. Export a backup before closing.")
}
async fn database() -> Result<Database, String> {
    Database::open("notepad-local")
        .with_version(1u8)
        .with_on_upgrade_needed(|_, db| {
            db.create_object_store(STORE).build()?;
            Ok(())
        })
        .await
        .map_err(db_error)
}
pub async fn load() -> Result<Option<Workspace>, String> {
    if is_desktop() {
        return invoke("load_draft", &serde_json::json!({})).await;
    }
    let db = database().await?;
    let tx = db.transaction(STORE).build().map_err(db_error)?;
    let store = tx.object_store(STORE).map_err(db_error)?;
    let stored: Option<String> = store.get("workspace").await.map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    db.close();
    if let Some(json) = stored {
        return decode(&json).map(Some);
    }
    // Keep the legacy copy until the IndexedDB write has actually committed.
    let legacy = match web_sys::window()
        .ok_or("Window unavailable")?
        .local_storage()
        .map_err(js_error)?
    {
        Some(storage) => storage.get_item(LEGACY).map_err(js_error)?,
        None => None,
    };
    if let Some(json) = legacy {
        let workspace = decode(&json)?;
        save(&workspace, None).await?;
        return Ok(Some(workspace));
    }
    Ok(None)
}
fn decode(json: &str) -> Result<Workspace, String> {
    let w: Workspace = serde_json::from_str(json).map_err(|_|"The local draft could not be read. It has been kept; import a backup or explicitly load the server copy.".to_string())?;
    w.upgrade()
        .map_err(|e| format!("Local draft: {e}. It has not been overwritten."))
}
pub async fn save(w: &Workspace, base: Option<&Workspace>) -> Result<(), String> {
    w.validate()
        .map_err(|e| format!("Local draft not saved: {e}. Export a backup."))?;
    if let Some(b) = base {
        b.validate().map_err(str::to_string)?;
        if b.revision != w.revision {
            return Err("Sync baseline revision differs from the draft".into());
        }
    }
    if is_desktop() {
        return invoke(
            "save_draft",
            &serde_json::json!({"workspace":w,"base":base}),
        )
        .await;
    }
    let json = serde_json::to_string(w).map_err(db_error)?;
    let db = database().await?;
    let tx = db
        .transaction(STORE)
        .with_mode(TransactionMode::Readwrite)
        .build()
        .map_err(db_error)?;
    let store = tx.object_store(STORE).map_err(db_error)?;
    store
        .put(json)
        .with_key("workspace")
        .await
        .map_err(db_error)?;
    if let Some(base) = base {
        store
            .put(serde_json::to_string(base).map_err(db_error)?)
            .with_key("sync-base")
            .await
            .map_err(db_error)?;
    } else {
        store.delete("sync-base").await.map_err(db_error)?;
    }
    tx.commit().await.map_err(db_error)?;
    db.close();
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        storage.remove_item(LEGACY).ok();
    }
    Ok(())
}
pub async fn load_sync_base() -> Result<Option<Workspace>, String> {
    if is_desktop() {
        return invoke("load_sync_base", &serde_json::json!({})).await;
    }
    let db = database().await?;
    let tx = db.transaction(STORE).build().map_err(db_error)?;
    let stored: Option<String> = tx
        .object_store(STORE)
        .map_err(db_error)?
        .get("sync-base")
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    db.close();
    stored.map(|s| decode(&s)).transpose()
}
fn request_timeout() -> Result<(web_sys::AbortSignal, gloo_timers::callback::Timeout), String> {
    let controller = web_sys::AbortController::new().map_err(js_error)?;
    let signal = controller.signal();
    let timer = gloo_timers::callback::Timeout::new(20_000, move || controller.abort());
    Ok((signal, timer))
}
pub async fn load_server() -> Result<Workspace, String> {
    if is_desktop() {
        return invoke("load_server", &serde_json::json!({})).await;
    }
    let (signal, _timeout) = request_timeout()?;
    let r = gloo_net::http::Request::get("/api/workspace")
        .abort_signal(Some(&signal))
        .cache(web_sys::RequestCache::NoStore)
        .send()
        .await
        .map_err(|_| "Server unavailable. Your local draft is kept.".to_string())?;
    if !r.ok() {
        return Err(r
            .text()
            .await
            .unwrap_or_else(|_| "Could not load the server copy.".into()));
    }
    let w: Workspace = r
        .json()
        .await
        .map_err(|_| "Server data could not be read.".to_string())?;
    w.validate().map_err(str::to_string)?;
    Ok(w)
}
pub async fn save_server(w: &Workspace) -> Result<Workspace, String> {
    if is_desktop() {
        return invoke("save_server", &serde_json::json!({"workspace":w})).await;
    }
    let (signal, _timeout) = request_timeout()?;
    let r = gloo_net::http::Request::put("/api/workspace")
        .abort_signal(Some(&signal))
        .header("x-notepad-request", "1")
        .json(w)
        .map_err(db_error)?
        .send()
        .await
        .map_err(|_| "Server unavailable. Your local draft is kept; retry saving.".to_string())?;
    if !r.ok() {
        return Err(r
            .text()
            .await
            .unwrap_or_else(|_| "Server save failed.".into()));
    }
    let saved: Workspace = r
        .json()
        .await
        .map_err(|_| "Save response unreadable. Export before reloading.".to_string())?;
    saved.validate().map_err(str::to_string)?;
    Ok(saved)
}

pub async fn auth_state() -> Result<notepad_core::AuthState, String> {
    if is_desktop() {
        return invoke("auth_state", &serde_json::json!({})).await;
    }
    let (signal, _timeout) = request_timeout()?;
    let response = gloo_net::http::Request::get("/api/auth/state")
        .abort_signal(Some(&signal))
        .cache(web_sys::RequestCache::NoStore)
        .send()
        .await
        .map_err(|_| "Server unavailable. Local drafts remain available.".to_string())?;
    if !response.ok() {
        return Err("Could not read server sign-in state.".into());
    }
    response
        .json()
        .await
        .map_err(|_| "Invalid sign-in response.".into())
}
pub async fn login(password: String) -> Result<(), String> {
    if is_desktop() {
        return invoke("login", &serde_json::json!({"password":password})).await;
    }
    let (signal, _timeout) = request_timeout()?;
    let response = gloo_net::http::Request::post("/api/auth/login")
        .abort_signal(Some(&signal))
        .header("x-notepad-request", "1")
        .json(&serde_json::json!({"password":password}))
        .map_err(db_error)?
        .send()
        .await
        .map_err(|_| "Server unavailable. Try signing in again.".to_string())?;
    if !response.ok() {
        return Err(response
            .text()
            .await
            .unwrap_or_else(|_| "Sign-in failed.".into()));
    }
    Ok(())
}
pub async fn logout() -> Result<(), String> {
    if is_desktop() {
        return invoke("logout", &serde_json::json!({})).await;
    }
    let (signal, _timeout) = request_timeout()?;
    let response = gloo_net::http::Request::post("/api/auth/logout")
        .abort_signal(Some(&signal))
        .header("x-notepad-request", "1")
        .send()
        .await
        .map_err(|_| "Server unavailable. Sign-out was not completed.".to_string())?;
    if !response.ok() {
        return Err("Could not sign out on the server.".into());
    }
    Ok(())
}
pub async fn clear_browser_draft() -> Result<(), String> {
    let db = database().await?;
    let tx = db
        .transaction(STORE)
        .with_mode(TransactionMode::Readwrite)
        .build()
        .map_err(db_error)?;
    tx.object_store(STORE)
        .map_err(db_error)?
        .delete("workspace")
        .await
        .map_err(db_error)?;
    tx.object_store(STORE)
        .map_err(db_error)?
        .delete("sync-base")
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    db.close();
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        storage.remove_item(LEGACY).map_err(js_error)?;
    }
    Ok(())
}

/// Hold an origin-wide Web Lock until this document closes. A second tab cannot
/// overwrite the shared cache with a different offline draft.
pub async fn acquire_editor() -> Result<bool, String> {
    if is_desktop() {
        return Ok(true);
    }
    use std::{cell::RefCell, rc::Rc};
    let window = web_sys::window().ok_or("Window unavailable")?;
    let navigator = js_sys::Reflect::get(&window, &"navigator".into()).map_err(js_error)?;
    let locks = js_sys::Reflect::get(&navigator, &"locks".into()).map_err(js_error)?;
    if locks.is_undefined() || locks.is_null() {
        return Err("This browser needs Web Locks to protect drafts. Use HTTPS (or localhost) and a current browser.".into());
    }
    let request: js_sys::Function = js_sys::Reflect::get(&locks, &"request".into())
        .map_err(js_error)?
        .dyn_into()
        .map_err(js_error)?;
    let options = js_sys::Object::new();
    js_sys::Reflect::set(&options, &"ifAvailable".into(), &true.into()).map_err(js_error)?;
    let result = Rc::new(RefCell::new(None));
    let acquired = result.clone();
    let callback = wasm_bindgen::closure::Closure::<dyn FnMut(JsValue) -> js_sys::Promise>::new(
        move |lock: JsValue| {
            let available = !lock.is_null() && !lock.is_undefined();
            *acquired.borrow_mut() = Some(Ok(available));
            if available {
                js_sys::Promise::new(&mut |_, _| {})
            } else {
                js_sys::Promise::resolve(&JsValue::UNDEFINED)
            }
        },
    );
    let pending: js_sys::Promise = request
        .call3(
            &locks,
            &"notepad-editor".into(),
            &options,
            callback.as_ref(),
        )
        .map_err(js_error)?
        .dyn_into()
        .map_err(js_error)?;
    callback.forget();
    let failure = result.clone();
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(error) = wasm_bindgen_futures::JsFuture::from(pending).await {
            *failure.borrow_mut() = Some(Err(js_error(error)));
        }
    });
    loop {
        if let Some(outcome) = result.borrow_mut().take() {
            return outcome;
        }
        gloo_timers::future::TimeoutFuture::new(20).await;
    }
}
