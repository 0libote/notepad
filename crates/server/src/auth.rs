use crate::{ApiError, AppState, internal};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use axum::{
    Json,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use notepad_core::AuthState;
use rand_core::{OsRng, RngCore};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const SESSION_SECONDS: u64 = 7 * 24 * 60 * 60;
pub struct Config {
    origin: String,
    secure: bool,
    attempts: Mutex<VecDeque<Instant>>,
    verification: Arc<tokio::sync::Semaphore>,
}
impl Config {
    pub fn new(public_url: &str) -> Result<Self, String> {
        let url = url::Url::parse(public_url).map_err(|e| e.to_string())?;
        let loopback = match url.host() {
            Some(url::Host::Domain("localhost")) => true,
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            _ => false,
        };
        if !["http", "https"].contains(&url.scheme())
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
            || (url.scheme() == "http" && !loopback)
        {
            return Err("NOTEPAD_PUBLIC_URL must be an HTTPS origin, or a loopback HTTP origin for local use".into());
        }
        Ok(Self {
            origin: url.origin().ascii_serialization(),
            secure: url.scheme() == "https",
            attempts: Mutex::new(VecDeque::new()),
            verification: Arc::new(tokio::sync::Semaphore::new(2)),
        })
    }
    fn cookie_name(&self) -> &str {
        if self.secure {
            "__Host-notepad_session"
        } else {
            "notepad_session"
        }
    }
    fn cookie(&self, token: &str, age: u64) -> HeaderValue {
        HeaderValue::from_str(&format!(
            "{}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={age}{}",
            self.cookie_name(),
            if self.secure { "; Secure" } else { "" }
        ))
        .expect("Generated cookie is valid")
    }
    fn mutation_allowed(&self, headers: &HeaderMap) -> bool {
        if let Some(origin) = headers.get(header::ORIGIN) {
            return origin.to_str().is_ok_and(|o| o == self.origin);
        }
        // The custom header cannot be set by cross-site forms. No CORS origins are allowed.
        headers.get("x-notepad-request").is_some_and(|v| v == "1")
            && !headers
                .get("sec-fetch-site")
                .is_some_and(|v| v != "same-origin" && v != "none")
    }
}
pub fn schema(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS account (id INTEGER PRIMARY KEY CHECK(id=1), password_hash TEXT NOT NULL); CREATE TABLE IF NOT EXISTS sessions (digest BLOB PRIMARY KEY, expires INTEGER NOT NULL);")
}
fn hash_password(password: &str) -> Result<String, String> {
    Argon2::default()
        .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
        .map(|hash| hash.to_string())
        .map_err(|e| e.to_string())
}
pub fn set_password(db: &mut Connection, password: &str) -> Result<(), String> {
    let hash = hash_password(password)?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    tx.execute("INSERT INTO account VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET password_hash=excluded.password_hash",[hash]).map_err(|e|e.to_string())?;
    tx.execute("DELETE FROM sessions", [])
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn password_hash(db: &Connection) -> rusqlite::Result<Option<String>> {
    db.query_row("SELECT password_hash FROM account WHERE id=1", [], |r| {
        r.get(0)
    })
    .optional()
}
fn token(headers: &HeaderMap, config: &Config) -> Option<String> {
    if let Some(auth) = headers.get(header::AUTHORIZATION) {
        return auth
            .to_str()
            .ok()?
            .strip_prefix("Bearer ")
            .map(str::to_string);
    }
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|c| {
            c.trim()
                .strip_prefix(&format!("{}=", config.cookie_name()))
                .map(str::to_string)
        })
}
fn digest(token: &str) -> Option<Vec<u8>> {
    let bytes = URL_SAFE_NO_PAD.decode(token).ok()?;
    if bytes.len() != 32 {
        return None;
    }
    Some(Sha256::digest(token.as_bytes()).to_vec())
}
fn authenticated(db: &Connection, headers: &HeaderMap, config: &Config) -> rusqlite::Result<bool> {
    let Some(hash) = token(headers, config).and_then(|t| digest(&t)) else {
        return Ok(false);
    };
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sessions WHERE digest=?1 AND expires>?2)",
        rusqlite::params![hash, now()],
        |r| r.get(0),
    )
}
pub async fn status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuthState>, ApiError> {
    let db = state.db.lock().map_err(internal)?;
    Ok(Json(AuthState {
        configured: password_hash(&db).map_err(internal)?.is_some(),
        authenticated: authenticated(&db, &headers, &state.auth).map_err(internal)?,
    }))
}
#[derive(Deserialize)]
pub struct Login {
    password: String,
}
#[derive(Serialize)]
pub struct LoginResult {
    #[serde(flatten)]
    state: AuthState,
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<String>,
}
fn auth_permit(config: &Config) -> Result<tokio::sync::OwnedSemaphorePermit, ApiError> {
    {
        let mut attempts = config.attempts.lock().map_err(internal)?;
        let time = Instant::now();
        while attempts
            .front()
            .is_some_and(|t| time.duration_since(*t) > Duration::from_secs(60))
        {
            attempts.pop_front();
        }
        if attempts.len() >= 10 {
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                "Too many sign-in attempts. Try again in a minute.".into(),
            ));
        }
        attempts.push_back(time);
    }
    let permit = config
        .verification
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            (
                StatusCode::TOO_MANY_REQUESTS,
                "Sign-in busy. Try again shortly.".into(),
            )
        })?;
    Ok(permit)
}
pub async fn setup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Login>,
) -> Result<Response, ApiError> {
    if !state.auth.mutation_allowed(&headers) {
        return Err((
            StatusCode::FORBIDDEN,
            "Open setup from the configured app URL.".into(),
        ));
    }
    if password_hash(&*state.db.lock().map_err(internal)?)
        .map_err(internal)?
        .is_some()
    {
        return Err((
            StatusCode::CONFLICT,
            "This notebook is already set up. Sign in instead.".into(),
        ));
    }
    let permit = auth_permit(&state.auth)?;
    let hash = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        hash_password(&input.password)
    })
    .await
    .map_err(internal)?
    .map_err(internal)?;
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let token = URL_SAFE_NO_PAD.encode(bytes);
    {
        let mut db = state.db.lock().map_err(internal)?;
        let tx = db.transaction().map_err(internal)?;
        if password_hash(&tx).map_err(internal)?.is_some() {
            return Err((
                StatusCode::CONFLICT,
                "This notebook is already set up. Sign in instead.".into(),
            ));
        }
        tx.execute("INSERT INTO account VALUES(1,?1)", [hash])
            .map_err(internal)?;
        tx.execute(
            "INSERT INTO sessions VALUES(?1,?2)",
            rusqlite::params![digest(&token).unwrap(), now() + SESSION_SECONDS],
        )
        .map_err(internal)?;
        tx.commit().map_err(internal)?;
    }
    session_response(&state.auth, &headers, token)
}
pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(login): Json<Login>,
) -> Result<Response, ApiError> {
    if !state.auth.mutation_allowed(&headers) {
        return Err((
            StatusCode::FORBIDDEN,
            "Sign in from the configured app URL.".into(),
        ));
    }
    let hash = {
        let db = state.db.lock().map_err(internal)?;
        password_hash(&db).map_err(internal)?
    }
    .ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Choose your notebook password on the first-run setup screen.".into(),
    ))?;
    let permit = auth_permit(&state.auth)?;
    let check_hash = hash.clone();
    let valid = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        PasswordHash::new(&check_hash).is_ok_and(|h| {
            Argon2::default()
                .verify_password(login.password.as_bytes(), &h)
                .is_ok()
        })
    })
    .await
    .map_err(internal)?;
    if !valid {
        return Err((StatusCode::UNAUTHORIZED, "Password not accepted.".into()));
    }
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let token = URL_SAFE_NO_PAD.encode(bytes);
    {
        let mut db = state.db.lock().map_err(internal)?;
        let tx = db.transaction().map_err(internal)?;
        if password_hash(&tx).map_err(internal)?.as_ref() != Some(&hash) {
            return Err((
                StatusCode::UNAUTHORIZED,
                "Password changed. Sign in again.".into(),
            ));
        }
        tx.execute("DELETE FROM sessions WHERE expires<=?1", [now()])
            .map_err(internal)?;
        tx.execute("DELETE FROM sessions WHERE digest IN (SELECT digest FROM sessions ORDER BY expires DESC LIMIT -1 OFFSET 99)",[]).map_err(internal)?;
        tx.execute(
            "INSERT INTO sessions VALUES(?1,?2)",
            rusqlite::params![digest(&token).unwrap(), now() + SESSION_SECONDS],
        )
        .map_err(internal)?;
        tx.commit().map_err(internal)?;
    }
    session_response(&state.auth, &headers, token)
}
fn session_response(
    config: &Config,
    headers: &HeaderMap,
    token: String,
) -> Result<Response, ApiError> {
    let desktop = headers
        .get("x-notepad-client")
        .is_some_and(|v| v == "desktop")
        && !headers.contains_key(header::ORIGIN);
    let mut response = Json(LoginResult {
        state: AuthState {
            configured: true,
            authenticated: true,
        },
        token: desktop.then(|| token.clone()),
    })
    .into_response();
    if !desktop {
        response
            .headers_mut()
            .insert(header::SET_COOKIE, config.cookie(&token, SESSION_SECONDS));
    }
    Ok(response)
}
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if !state.auth.mutation_allowed(&headers) {
        return Err((
            StatusCode::FORBIDDEN,
            "Sign out from the configured app URL.".into(),
        ));
    }
    if let Some(hash) = token(&headers, &state.auth).and_then(|t| digest(&t)) {
        state
            .db
            .lock()
            .map_err(internal)?
            .execute("DELETE FROM sessions WHERE digest=?1", [hash])
            .map_err(internal)?;
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, state.auth.cookie("", 0));
    Ok(response)
}
pub async fn require_session(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    {
        let db = state.db.lock().map_err(internal)?;
        if password_hash(&db).map_err(internal)?.is_none() {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "Choose your notebook password on the first-run setup screen.".into(),
            ));
        }
        if !authenticated(&db, request.headers(), &state.auth).map_err(internal)? {
            return Err((
                StatusCode::UNAUTHORIZED,
                "Sign in to access the server. Your local draft is kept.".into(),
            ));
        }
    }
    if request.method() != Method::GET && !state.auth.mutation_allowed(request.headers()) {
        return Err((StatusCode::FORBIDDEN, "Request origin not allowed.".into()));
    }
    Ok(next.run(request).await)
}
pub async fn no_cache(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app, database};
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    const PASSWORD: &str = "test-only-owner-password";
    fn fixture(public: &str, configured: bool) -> (Router, AppState) {
        let mut db = database(":memory:").unwrap();
        if configured {
            set_password(&mut db, PASSWORD).unwrap();
        }
        let state = AppState {
            db: Arc::new(Mutex::new(db)),
            auth: Arc::new(Config::new(public).unwrap()),
        };
        (app(state.clone(), "missing"), state)
    }
    async fn send(
        app: &Router,
        method: &str,
        path: &str,
        body: &str,
        cookie: Option<&str>,
        origin: Option<&str>,
    ) -> Response {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .header("x-notepad-request", "1");
        if let Some(c) = cookie {
            req = req.header("cookie", c);
        }
        if let Some(o) = origin {
            req = req.header("origin", o);
        }
        app.clone()
            .oneshot(req.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap()
    }
    async fn sign_in(app: &Router) -> String {
        let response = send(
            app,
            "POST",
            "/api/auth/login",
            &format!(r#"{{"password":"{PASSWORD}"}}"#),
            None,
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .into()
    }
    #[tokio::test]
    async fn browser_setup_accepts_weak_and_empty_passwords_and_cannot_replace_an_owner() {
        for password in ["", "x", " ", "🔓"] {
            let (app, state) = fixture("http://127.0.0.1:3000", false);
            let body = serde_json::json!({"password":password}).to_string();
            assert_eq!(
                send(
                    &app,
                    "POST",
                    "/api/auth/setup",
                    &body,
                    None,
                    Some("https://attacker.invalid")
                )
                .await
                .status(),
                StatusCode::FORBIDDEN
            );
            assert!(password_hash(&state.db.lock().unwrap()).unwrap().is_none());
            let response = send(&app, "POST", "/api/auth/setup", &body, None, None).await;
            assert_eq!(response.status(), StatusCode::OK);
            let cookie = response.headers()["set-cookie"]
                .to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .to_string();
            assert_eq!(
                send(&app, "GET", "/api/workspace", "", Some(&cookie), None)
                    .await
                    .status(),
                StatusCode::OK
            );
            assert_eq!(
                send(
                    &app,
                    "POST",
                    "/api/auth/setup",
                    r#"{"password":"replacement"}"#,
                    None,
                    None
                )
                .await
                .status(),
                StatusCode::CONFLICT
            );
            assert_eq!(
                send(&app, "POST", "/api/auth/login", &body, None, None)
                    .await
                    .status(),
                StatusCode::OK
            );
            assert_eq!(
                send(
                    &app,
                    "POST",
                    "/api/auth/login",
                    r#"{"password":"replacement"}"#,
                    None,
                    None
                )
                .await
                .status(),
                StatusCode::UNAUTHORIZED
            );
        }
    }
    #[tokio::test]
    async fn simultaneous_setup_has_only_one_winner() {
        let (app, state) = fixture("http://127.0.0.1:3000", false);
        let (a, b) = tokio::join!(
            send(
                &app,
                "POST",
                "/api/auth/setup",
                r#"{"password":"a"}"#,
                None,
                None
            ),
            send(
                &app,
                "POST",
                "/api/auth/setup",
                r#"{"password":"b"}"#,
                None,
                None
            )
        );
        let statuses = [a.status(), b.status()];
        assert_eq!(statuses.iter().filter(|&&s| s == StatusCode::OK).count(), 1);
        assert_eq!(
            statuses
                .iter()
                .filter(|&&s| s == StatusCode::CONFLICT)
                .count(),
            1
        );
        let db = state.db.lock().unwrap();
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM account", [], |r| r.get::<_, u64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get::<_, u64>(0))
                .unwrap(),
            1
        );
    }
    #[tokio::test]
    async fn cookie_sessions_are_required_expire_and_revoke() {
        let (app, state) = fixture("http://127.0.0.1:3000", true);
        let response = send(&app, "GET", "/api/workspace", "", None, None).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(
            send(
                &app,
                "POST",
                "/api/auth/login",
                r#"{"password":"incorrect"}"#,
                None,
                None
            )
            .await
            .status(),
            StatusCode::UNAUTHORIZED
        );
        let cookie = sign_in(&app).await;
        assert_eq!(
            send(&app, "GET", "/api/workspace", "", Some(&cookie), None)
                .await
                .status(),
            StatusCode::OK
        );
        let stored: Vec<u8> = state
            .db
            .lock()
            .unwrap()
            .query_row("SELECT digest FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(stored.len(), 32);
        assert_ne!(stored, cookie.as_bytes());
        state
            .db
            .lock()
            .unwrap()
            .execute("UPDATE sessions SET expires=0", [])
            .unwrap();
        assert_eq!(
            send(&app, "GET", "/api/workspace", "", Some(&cookie), None)
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let cookie = sign_in(&app).await;
        let response = send(&app, "POST", "/api/auth/logout", "", Some(&cookie), None).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(
            response.headers()["set-cookie"]
                .to_str()
                .unwrap()
                .contains("Max-Age=0")
        );
        assert_eq!(
            send(&app, "GET", "/api/workspace", "", Some(&cookie), None)
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    #[tokio::test]
    async fn setup_is_fail_closed_and_password_reset_keeps_notes() {
        let (app, state) = fixture("http://127.0.0.1:3000", false);
        assert_eq!(
            send(&app, "GET", "/api/workspace", "", None, None)
                .await
                .status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            send(
                &app,
                "POST",
                "/api/auth/login",
                r#"{"password":"test-only-owner-password"}"#,
                None,
                None
            )
            .await
            .status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        set_password(&mut state.db.lock().unwrap(), PASSWORD).unwrap();
        let cookie = sign_in(&app).await;
        let before: String = state
            .db
            .lock()
            .unwrap()
            .query_row("SELECT json FROM workspace", [], |r| r.get(0))
            .unwrap();
        set_password(
            &mut state.db.lock().unwrap(),
            "replacement-test-only-password",
        )
        .unwrap();
        let after: String = state
            .db
            .lock()
            .unwrap()
            .query_row("SELECT json FROM workspace", [], |r| r.get(0))
            .unwrap();
        assert_eq!(before, after);
        assert_eq!(
            send(&app, "GET", "/api/workspace", "", Some(&cookie), None)
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert!(set_password(&mut state.db.lock().unwrap(), "short").is_ok());
    }
    #[tokio::test]
    async fn secure_cookies_origin_guards_and_login_limit() {
        let (app, state) = fixture("https://notes.example", true);
        let body = format!(r#"{{"password":"{PASSWORD}"}}"#);
        assert_eq!(
            send(
                &app,
                "POST",
                "/api/auth/login",
                &body,
                None,
                Some("https://attacker.example")
            )
            .await
            .status(),
            StatusCode::FORBIDDEN
        );
        let response = send(
            &app,
            "POST",
            "/api/auth/login",
            &body,
            None,
            Some("https://notes.example"),
        )
        .await;
        let header = response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .to_owned();
        for flag in [
            "__Host-notepad_session=",
            "Secure",
            "HttpOnly",
            "SameSite=Strict",
            "Path=/",
        ] {
            assert!(header.contains(flag));
        }
        let json: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
        assert!(json.get("token").is_none());
        let cookie = header.split(';').next().unwrap();
        let workspace = serde_json::to_string(&notepad_core::Workspace::default()).unwrap();
        assert_eq!(
            send(
                &app,
                "PUT",
                "/api/workspace",
                &workspace,
                Some(cookie),
                Some("https://attacker.example")
            )
            .await
            .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            send(
                &app,
                "POST",
                "/api/auth/logout",
                "",
                Some(cookie),
                Some("null")
            )
            .await
            .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            send(
                &app,
                "PUT",
                "/api/workspace",
                &workspace,
                Some(cookie),
                Some("https://notes.example")
            )
            .await
            .status(),
            StatusCode::OK
        );
        let req = Request::post("/api/auth/logout")
            .header("cookie", cookie)
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            app.clone().oneshot(req).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
        *state.auth.attempts.lock().unwrap() = std::iter::repeat_n(Instant::now(), 10).collect();
        assert_eq!(
            send(
                &app,
                "POST",
                "/api/auth/login",
                &body,
                None,
                Some("https://notes.example")
            )
            .await
            .status(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }
    #[tokio::test]
    async fn desktop_token_is_explicit_and_never_put_in_cookie() {
        let (app, _) = fixture("https://notes.example", true);
        let req = Request::post("/api/auth/login")
            .header("content-type", "application/json")
            .header("x-notepad-request", "1")
            .header("x-notepad-client", "desktop")
            .body(Body::from(format!(r#"{{"password":"{PASSWORD}"}}"#)))
            .unwrap();
        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().get("set-cookie").is_none());
        let value: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
        let token = value["token"].as_str().unwrap();
        assert_eq!(token.len(), 43);
        let req = Request::get("/api/workspace")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            app.clone().oneshot(req).await.unwrap().status(),
            StatusCode::OK
        );
    }
    #[test]
    fn https_is_required_for_remote_origins() {
        assert!(Config::new("http://notes.example").is_err());
        assert!(Config::new("https://user:pass@notes.example").is_err());
        assert!(Config::new("https://notes.example/path").is_err());
        assert!(Config::new("http://[::1]:3000").is_ok());
    }
}
