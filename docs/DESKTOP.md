# Desktop client

The Tauri client packages the Rust/WASM workspace and saves drafts through Rust IPC into SQLite in the operating system's application-data directory. The window uses the existing writing/canvas UI. No Node server or separate web server is needed for local-only use.

**Status:** implementation and portable backend tests are present. A native installer and WebView launch have not yet been verified in the current environment. Linux GTK/WebKit development packages and the Windows MinGW cross compiler are missing. Browser IPC tests verify the UI contract but do not substitute for native testing. The Linux desktop `cargo check` now passes in GitHub CI with GTK/WebKit installed; native linking, installers and WebView launch remain unverified.

## Build

Install the platform dependencies from [Tauri's prerequisites](https://v2.tauri.app/start/prerequisites/), plus Rust, Bun, and Trunk. Then, from the repo root:

```sh
bun install
rustup target add wasm32-unknown-unknown
cargo install --locked trunk --version 0.21.14
bun run desktop:dev
```

The desktop development command starts Trunk for live reload. It does not require the Axum server if you are using a local-only notebook. For a packaged build:

```sh
bun run desktop:build
```

The CLI builds the web assets first and places native bundles under `crates/desktop/target/release/bundle`. Build on each target OS; cross compilation is not a substitute for installation and WebView testing.

## Connect to your own server

The current connection setting is an environment variable read when the desktop client starts:

```sh
NOTEPAD_SERVER_URL=http://127.0.0.1:3000 bun run desktop:dev
```

Use your own server's base URL instead of localhost for another host. No variable means a local-only notebook; the primary action says Save locally. With a server configured, Save to server and Load server copy use a Rust HTTP client. Browser CORS is not involved. While the app is open and signed in, the shared WASM coordinator syncs automatically through that bridge. Drafts and sync baselines commit together in SQLite. The portable storage/HTTP components are tested; native execution of the coordinator is not yet verified.

Local drafts are scoped to the normalized server URL. Starting with a different URL opens a separate draft namespace so revisions from two different servers cannot overwrite one another. Switching between local-only mode and a server also opens separate namespaces. Export a workspace and import it to transfer content deliberately.

The SQLite database is `drafts.db` under Tauri's `app_data_dir` for `io.notepad.workspace`. Keep application-data backups. The frontend shows pending/saved/error persistence states and coalesces changes with one draft write in flight. Closing or killing the app while a save is pending can still lose the most recent edits. Wait for Saved on this device before quitting. Native close handling, simultaneous app instances, and save dialogs need native validation.

Connected clients now sign in through the same password form. Session tokens stay in Rust memory, not in SQLite or the WebView cache. Sign out disconnects the server and keeps local drafts; restarting requires sign-in again. Remote server URLs require HTTPS, with HTTP allowed only on loopback. See [hosting](HOSTING.md) for owner setup and TLS configuration. URL editing inside the app, persistent OS-keychain sessions, synchronization while the app is closed, OS link opening and download handling, pen/touch hardware, and signed installers remain follow-up work. Large attachments are still part of the draft snapshot; separate attachment storage is not implemented.
