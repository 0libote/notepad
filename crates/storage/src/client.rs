use crate::DraftStore;
use notepad_core::{AuthState, Workspace};
use std::sync::Mutex;
pub struct DesktopClient {
    store: Mutex<DraftStore>,
    server: Option<reqwest::Url>,
    client: reqwest::Client,
    session: Mutex<Option<String>>,
}
impl DesktopClient {
    pub fn new(path: impl AsRef<std::path::Path>, server: Option<&str>) -> Result<Self, String> {
        let server = server
            .map(|raw| -> Result<reqwest::Url, String> {
                let mut url = reqwest::Url::parse(raw).map_err(|e| e.to_string())?;
                if !["http", "https"].contains(&url.scheme())
                    || url.host_str().is_none()
                    || !url.username().is_empty()
                    || url.password().is_some()
                    || url.query().is_some()
                    || url.fragment().is_some()
                    || (url.scheme() == "http"
                        && !url.host_str().is_some_and(|h| {
                            h == "localhost"
                                || h.trim_matches(['[', ']'])
                                    .parse::<std::net::IpAddr>()
                                    .is_ok_and(|ip| ip.is_loopback())
                        }))
                {
                    return Err(
                        "Server must be an HTTP(S) base URL without credentials, query or fragment"
                            .into(),
                    );
                }
                url.set_path(&format!("{}/", url.path().trim_end_matches('/')));
                Ok(url)
            })
            .transpose()?;
        let store = DraftStore::open(path)?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            store: Mutex::new(store),
            server,
            client,
            session: Mutex::new(None),
        })
    }
    pub async fn auth_state(&self) -> Result<AuthState, String> {
        let Some(server) = &self.server else {
            return Ok(AuthState {
                configured: true,
                authenticated: true,
            });
        };
        let response = self
            .authorize(
                self.client
                    .get(server.join("api/auth/state").map_err(|e| e.to_string())?),
            )?
            .send()
            .await
            .map_err(|_| "Server unavailable. Local drafts remain available.".to_string())?;
        if !response.status().is_success() {
            return Err("Could not read server sign-in state.".into());
        }
        response
            .json()
            .await
            .map_err(|_| "Invalid sign-in response.".into())
    }
    pub async fn login(&self, password: String) -> Result<(), String> {
        self.authenticate(password, false).await
    }
    pub async fn setup_owner(&self, password: String) -> Result<(), String> {
        self.authenticate(password, true).await
    }
    async fn authenticate(&self, password: String, setup: bool) -> Result<(), String> {
        let server = self
            .server
            .as_ref()
            .ok_or("Local-only notebooks do not need server sign-in")?;
        let response = self
            .client
            .post(
                server
                    .join(if setup {
                        "api/auth/setup"
                    } else {
                        "api/auth/login"
                    })
                    .map_err(|e| e.to_string())?,
            )
            .header("x-notepad-request", "1")
            .header("x-notepad-client", "desktop")
            .json(&serde_json::json!({"password":password}))
            .send()
            .await
            .map_err(|_| "Server unavailable. Try signing in again.".to_string())?;
        if !response.status().is_success() {
            return Err(match response.status().as_u16() {
                401 => "Password not accepted.",
                409 => "This notebook is already set up. Sign in instead.",
                429 => "Too many sign-in attempts. Try again in a minute.",
                503 => "Owner password not configured on the server.",
                _ => "Sign-in failed.",
            }
            .into());
        }
        #[derive(serde::Deserialize)]
        struct Session {
            token: String,
        }
        let session: Session = response
            .json()
            .await
            .map_err(|_| "Invalid server sign-in response.".to_string())?;
        if session.token.len() != 43 {
            return Err("Invalid server session.".into());
        }
        *self.session.lock().map_err(|_| "Session unavailable")? = Some(session.token);
        Ok(())
    }
    pub async fn logout(&self) -> Result<(), String> {
        if let Some(server) = &self.server {
            let response = self
                .authorize(
                    self.client
                        .post(server.join("api/auth/logout").map_err(|e| e.to_string())?)
                        .header("x-notepad-request", "1"),
                )?
                .send()
                .await
                .map_err(|_| "Server unavailable. Sign-out was not completed.".to_string())?;
            if !response.status().is_success() {
                return Err("Could not sign out on the server.".into());
            }
        }
        *self.session.lock().map_err(|_| "Session unavailable")? = None;
        Ok(())
    }
    fn authorize(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder, String> {
        Ok(
            if let Some(token) = self
                .session
                .lock()
                .map_err(|_| "Session unavailable")?
                .as_ref()
            {
                request.bearer_auth(token)
            } else {
                request
            },
        )
    }
    pub fn connection(&self) -> Option<String> {
        self.server.as_ref().map(ToString::to_string)
    }
    pub fn load_draft(&self) -> Result<Option<Workspace>, String> {
        self.store()?.load(self.scope())
    }
    pub fn save_draft(&self, w: &Workspace) -> Result<(), String> {
        self.store()?.save(self.scope(), w)
    }
    pub fn load_sync_base(&self) -> Result<Option<Workspace>, String> {
        self.store()?.load_base(self.scope())
    }
    pub fn save_synced_draft(&self, w: &Workspace, base: Option<&Workspace>) -> Result<(), String> {
        self.store()?.save_pair(self.scope(), w, base)
    }
    pub async fn load_server(&self) -> Result<Workspace, String> {
        self.request(None).await
    }
    pub async fn save_server(&self, w: Workspace) -> Result<Workspace, String> {
        self.request(Some(w)).await
    }

    pub fn scope(&self) -> &str {
        self.server.as_ref().map(|u| u.as_str()).unwrap_or("local")
    }
    fn store(&self) -> Result<std::sync::MutexGuard<'_, DraftStore>, String> {
        self.store
            .lock()
            .map_err(|_| "Local database unavailable. Export your workspace.".into())
    }
    async fn request(&self, workspace: Option<Workspace>) -> Result<Workspace, String> {
        let Some(server) = &self.server else {
            if let Some(w) = workspace {
                self.store()?.save(self.scope(), &w)?;
                return Ok(w);
            }
            return Ok(self.store()?.load(self.scope())?.unwrap_or_default());
        };
        let url = server.join("api/workspace").map_err(|e| e.to_string())?;
        let request = if let Some(w) = workspace {
            w.validate().map_err(str::to_string)?;
            let json = serde_json::to_string(&w).map_err(|e| e.to_string())?;
            if json.len() > 12 * 1024 * 1024 {
                return Err("Workspace exceeds the server's 12 MiB limit. Export a backup.".into());
            }
            self.client
                .put(url)
                .header("content-type", "application/json")
                .body(json)
        } else {
            self.client.get(url)
        };
        let mut response = self
            .authorize(request.header("x-notepad-request", "1"))?
            .send()
            .await
            .map_err(|_| {
                "Server unavailable. Your local draft is kept; retry saving.".to_string()
            })?;
        let status = response.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
            if bytes.len() + chunk.len() > 12 * 1024 * 1024 {
                return Err("Server response too large. Your local draft is kept.".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        if !status.is_success() {
            return Err(format!(
                "Server {}: {}",
                status.as_u16(),
                String::from_utf8_lossy(&bytes)
            ));
        }
        let w: Workspace = serde_json::from_slice(&bytes)
            .map_err(|_| "Server response unreadable. Export before reloading.".to_string())?;
        w.validate().map_err(str::to_string)?;
        Ok(w)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn local_only_and_remote_round_trip_keep_drafts_separate() {
        let path = std::env::temp_dir().join(format!("notepad-client-{}.db", std::process::id()));
        let mut work = Workspace::default();
        work.notes[0].body = "Offline work".into();
        let local = DesktopClient::new(&path, None).unwrap();
        local.save_draft(&work).unwrap();
        assert_eq!(local.load_server().await.unwrap(), work);
        assert_eq!(local.save_server(work.clone()).await.unwrap(), work);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let app = axum::Router::new().route(
            "/api/workspace",
            axum::routing::get(|| async { axum::Json(Workspace::default()) }).put(
                |axum::Json(mut w): axum::Json<Workspace>| async move {
                    w.revision += 1;
                    axum::Json(w)
                },
            ),
        );
        let task = tokio::spawn(async { axum::serve(listener, app).await.unwrap() });
        let remote = DesktopClient::new(&path, Some(&url)).unwrap();
        assert!(remote.load_draft().unwrap().is_none());
        assert_eq!(remote.load_server().await.unwrap(), Workspace::default());
        let saved = remote.save_server(work.clone()).await.unwrap();
        assert_eq!(saved.revision, 1);
        remote.save_draft(&saved).unwrap();
        assert_eq!(remote.load_draft().unwrap().unwrap(), saved);
        task.abort();
        assert!(remote.load_server().await.is_err());
        assert_eq!(remote.load_draft().unwrap().unwrap(), saved);
        assert_eq!(local.load_draft().unwrap().unwrap(), work);
        drop(local);
        drop(remote);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn rejects_ambiguous_or_non_http_server_identities() {
        for url in [
            "file:///tmp/x",
            "https://user:pass@host/",
            "https://host/?query=x",
            "https://host/#fragment",
        ] {
            assert!(DesktopClient::new(":memory:", Some(url)).is_err());
        }
    }
}

#[cfg(test)]
mod session_tests {
    use super::*;
    use axum::{
        Json, Router,
        http::{HeaderMap, StatusCode},
        routing::{get, post},
    };
    const TOKEN: &str = "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
    fn signed_in(headers: &HeaderMap) -> bool {
        headers
            .get("authorization")
            .is_some_and(|v| v == format!("Bearer {TOKEN}").as_str())
    }
    #[tokio::test]
    async fn sessions_live_in_memory_and_authenticate_remote_requests() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new()
            .route(
                "/api/auth/state",
                get(|h: HeaderMap| async move {
                    Json(AuthState {
                        configured: true,
                        authenticated: signed_in(&h),
                    })
                }),
            )
            .route(
                "/api/auth/login",
                post(
                    |h: HeaderMap, Json(body): Json<serde_json::Value>| async move {
                        if h.get("x-notepad-client").is_none_or(|v| v != "desktop")
                            || body["password"] != "test-only-password"
                        {
                            return Err(StatusCode::UNAUTHORIZED);
                        }
                        Ok(Json(serde_json::json!({"token":TOKEN})))
                    },
                ),
            )
            .route(
                "/api/auth/logout",
                post(|h: HeaderMap| async move {
                    if signed_in(&h) {
                        StatusCode::NO_CONTENT
                    } else {
                        StatusCode::UNAUTHORIZED
                    }
                }),
            )
            .route(
                "/api/workspace",
                get(|h: HeaderMap| async move {
                    if signed_in(&h) {
                        Ok(Json(Workspace::default()))
                    } else {
                        Err(StatusCode::UNAUTHORIZED)
                    }
                }),
            );
        let task = tokio::spawn(async {
            axum::serve(listener, app).await.unwrap();
        });
        let client = DesktopClient::new(":memory:", Some(&url)).unwrap();
        assert!(!client.auth_state().await.unwrap().authenticated);
        assert!(client.load_server().await.is_err());
        assert!(client.login("wrong".into()).await.is_err());
        client.login("test-only-password".into()).await.unwrap();
        assert!(client.auth_state().await.unwrap().authenticated);
        assert_eq!(client.load_server().await.unwrap(), Workspace::default());
        client.save_draft(&Workspace::default()).unwrap();
        client.logout().await.unwrap();
        assert!(client.session.lock().unwrap().is_none());
        assert!(client.load_server().await.is_err());
        assert!(client.load_draft().unwrap().is_some());
        task.abort();
    }
}
