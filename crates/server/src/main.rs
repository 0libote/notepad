use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    routing::get,
};
use notepad_core::Workspace;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use tower_http::services::{ServeDir, ServeFile};
mod auth;
#[derive(Clone)]
struct AppState {
    db: Db,
    auth: Arc<auth::Config>,
}
type Db = Arc<Mutex<Connection>>;
type ApiError = (StatusCode, String);
fn database(path: &str) -> Result<Connection, Box<dyn std::error::Error>> {
    #[cfg(unix)]
    if path != ":memory:" {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path)?;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    let db = Connection::open(path)?;
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS workspace (id INTEGER PRIMARY KEY CHECK(id=1), json TEXT NOT NULL);")?;
    auth::schema(&db)?;
    db.execute(
        "INSERT OR IGNORE INTO workspace VALUES (1, ?1)",
        [serde_json::to_string(&Workspace::default()).unwrap()],
    )?;
    let tx = db.unchecked_transaction()?;
    let json: String = tx.query_row("SELECT json FROM workspace WHERE id=1", [], |r| r.get(0))?;
    let stored: Workspace = serde_json::from_str(&json)?;
    let old_schema = stored.schema;
    let upgraded = stored.upgrade().map_err(std::io::Error::other)?;
    if old_schema != upgraded.schema {
        tx.execute(
            "UPDATE workspace SET json=?1 WHERE id=1",
            [serde_json::to_string(&upgraded)?],
        )?;
    }
    tx.commit()?;
    Ok(db)
}
fn internal(e: impl std::fmt::Display) -> ApiError {
    eprintln!("Storage error: {e}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "Storage unavailable. Local drafts have not been replaced; retry saving.".into(),
    )
}
async fn load(State(state): State<AppState>) -> Result<Json<Workspace>, ApiError> {
    let db = state.db.lock().map_err(internal)?;
    let json: String = db
        .query_row("SELECT json FROM workspace WHERE id=1", [], |r| r.get(0))
        .map_err(internal)?;
    Ok(Json(serde_json::from_str(&json).map_err(internal)?))
}
async fn save(
    State(state): State<AppState>,
    Json(mut w): Json<Workspace>,
) -> Result<Json<Workspace>, ApiError> {
    w.validate()
        .map_err(|e| (StatusCode::UNPROCESSABLE_ENTITY, e.into()))?;
    let mut db = state.db.lock().map_err(internal)?;
    let tx = db.transaction().map_err(internal)?;
    let json: String = tx
        .query_row("SELECT json FROM workspace WHERE id=1", [], |r| r.get(0))
        .map_err(internal)?;
    let previous: Workspace = serde_json::from_str(&json).map_err(internal)?;
    if previous.revision != w.revision {
        return Err((
            StatusCode::CONFLICT,
            "Server changed during sync. Your local draft is kept. Try syncing again.".into(),
        ));
    }
    w.revision += 1;
    tx.execute(
        "UPDATE workspace SET json=?1 WHERE id=1",
        [serde_json::to_string(&w).map_err(internal)?],
    )
    .map_err(internal)?;
    tx.commit().map_err(internal)?;
    Ok(Json(w))
}
fn app(state: AppState, dist: &str) -> Router {
    let api = Router::new()
        .route("/api/workspace", get(load).put(save))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::require_session,
        ))
        .route("/api/auth/state", get(auth::status))
        .route(
            "/api/auth/login",
            axum::routing::post(auth::login).layer(DefaultBodyLimit::max(4096)),
        )
        .route(
            "/api/auth/setup",
            axum::routing::post(auth::setup).layer(DefaultBodyLimit::max(4096)),
        )
        .route("/api/auth/logout", axum::routing::post(auth::logout))
        .layer(axum::middleware::from_fn(auth::no_cache));
    Router::new()
        .merge(api)
        .route("/api/health", get(|| async { "ok" }))
        .fallback_service(
            ServeDir::new(dist).not_found_service(ServeFile::new(format!("{dist}/index.html"))),
        )
        .layer(DefaultBodyLimit::max(12 * 1024 * 1024))
        .layer(
            tower_http::set_header::SetResponseHeaderLayer::if_not_present(
                axum::http::header::X_CONTENT_TYPE_OPTIONS,
                axum::http::HeaderValue::from_static("nosniff"),
            ),
        )
        .layer(
            tower_http::set_header::SetResponseHeaderLayer::if_not_present(
                axum::http::header::CONTENT_SECURITY_POLICY,
                axum::http::HeaderValue::from_static(
                    "frame-ancestors 'none'; object-src 'none'; base-uri 'self'",
                ),
            ),
        )
        .with_state(state)
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::env::var("NOTEPAD_DATA").unwrap_or_else(|_| "data".into());
    std::fs::create_dir_all(&data)?;
    let mut database = database(&format!("{data}/notepad.db"))?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "setup") {
        let password = if args.len() == 3 && args[1] == "--password-file" {
            let input = std::fs::read_to_string(&args[2])?;
            input.trim_end_matches(['\r', '\n']).to_string()
        } else if args.len() == 1 {
            let password = rpassword::prompt_password("New owner password: ")?;
            if password != rpassword::prompt_password("Repeat password: ")? {
                return Err("Passwords did not match".into());
            }
            password
        } else {
            return Err("Usage: notepad-server setup [--password-file PATH]".into());
        };
        auth::set_password(&mut database, &password).map_err(std::io::Error::other)?;
        println!("Owner password configured. Existing sessions revoked; notes preserved.");
        return Ok(());
    }
    if !args.is_empty() {
        return Err("Usage: notepad-server [setup [--password-file PATH]]".into());
    }
    let db = Arc::new(Mutex::new(database));
    let public =
        std::env::var("NOTEPAD_PUBLIC_URL").unwrap_or_else(|_| "http://127.0.0.1:3000".into());
    let state = AppState {
        db,
        auth: Arc::new(auth::Config::new(&public).map_err(std::io::Error::other)?),
    };
    let addr = std::env::var("NOTEPAD_BIND").unwrap_or_else(|_| "127.0.0.1:3000".into());
    let dist = std::env::var("NOTEPAD_DIST").unwrap_or_else(|_| "dist".into());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("Notepad listening on http://{addr}");
    axum::serve(listener, app(state, &dist))
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    #[test]
    fn database_upgrades_without_changing_content_or_revision_and_rejects_future_data() {
        let path =
            std::env::temp_dir().join(format!("notepad-migration-{}.db", std::process::id()));
        let mut old = Workspace::default();
        old.schema = 1;
        old.revision = 17;
        old.notes[0].body = "Preserve this notebook".into();
        {
            let db = database(path.to_str().unwrap()).unwrap();
            db.execute(
                "UPDATE workspace SET json=?1",
                [serde_json::to_string(&old).unwrap()],
            )
            .unwrap();
        }
        {
            let db = database(path.to_str().unwrap()).unwrap();
            let json: String = db
                .query_row("SELECT json FROM workspace", [], |r| r.get(0))
                .unwrap();
            let saved: Workspace = serde_json::from_str(&json).unwrap();
            assert_eq!(saved.schema, notepad_core::SCHEMA);
            assert_eq!(saved.revision, 17);
            assert_eq!(saved.notes, old.notes);
            let mut future = saved;
            future.schema = 999;
            db.execute(
                "UPDATE workspace SET json=?1",
                [serde_json::to_string(&future).unwrap()],
            )
            .unwrap();
        }
        assert!(database(path.to_str().unwrap()).is_err());
        {
            let db = Connection::open(&path).unwrap();
            let json: String = db
                .query_row("SELECT json FROM workspace", [], |r| r.get(0))
                .unwrap();
            assert_eq!(
                serde_json::from_str::<Workspace>(&json).unwrap().schema,
                999
            );
        }
        std::fs::remove_file(path).unwrap();
    }
    #[tokio::test]
    async fn revisions_prevent_lost_updates_and_survive_reopen() {
        let path = std::env::temp_dir().join(format!("notepad-test-{}.db", std::process::id()));
        let db = Arc::new(Mutex::new(database(path.to_str().unwrap()).unwrap()));
        auth::set_password(&mut db.lock().unwrap(), "test-only-long-password").unwrap();
        let state = AppState {
            db: db.clone(),
            auth: Arc::new(auth::Config::new("http://127.0.0.1:3000").unwrap()),
        };
        let app = app(state, "missing");
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/auth/login")
                    .header("content-type", "application/json")
                    .header("x-notepad-request", "1")
                    .body(Body::from(r#"{"password":"test-only-long-password"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        let cookie = response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        let mut w = Workspace::default();
        w.notes[0].title = "Persistent".into();
        for expected in [StatusCode::OK, StatusCode::CONFLICT] {
            let r = app
                .clone()
                .oneshot(
                    Request::put("/api/workspace")
                        .header("cookie", &cookie)
                        .header("x-notepad-request", "1")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&w).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(r.status(), expected);
        }
        drop(app);
        drop(db);
        let db = database(path.to_str().unwrap()).unwrap();
        let json: String = db
            .query_row("SELECT json FROM workspace", [], |r| r.get(0))
            .unwrap();
        let saved: Workspace = serde_json::from_str(&json).unwrap();
        assert_eq!(saved.revision, 1);
        assert_eq!(saved.notes[0].title, "Persistent");
        drop(db);
        std::fs::remove_file(path).unwrap();
    }
}
