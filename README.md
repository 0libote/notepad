<img src="assets/icon.svg" width="64" height="64" alt="">

# Notepad

A Rust notebook with room to think. Write and read linked Markdown notes, then switch to a whiteboard to arrange text, sticky notes, links, images, emails, and freehand drawings.

An early, working single-user foundation for a self-hosted web app and a desktop client in development. See [architecture and roadmap](docs/ARCHITECTURE.md) and [design system](DESIGN.md).

## Run locally

Install Rust and Bun, then:

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked trunk --version 0.21.14
bun run build
cargo run -p notepad-server -- setup
bun run server
```

Open **http://127.0.0.1:3000**. Rust builds the browser UI to WebAssembly; Bun is only a command runner. Keep Cargo and Bun on the same Rust toolchain, or install the WASM target for the toolchain selected by your toolchain manager. The server creates `data/notepad.db` automatically.

For live reload, run `NOTEPAD_PUBLIC_URL=http://127.0.0.1:8080 bun run server` in one terminal and `bun run dev` in another. Open http://127.0.0.1:8080; Trunk proxies `/api` to the server. The two URLs have independent browser caches.

## Docker

```sh
docker compose build
docker compose run --rm -it notepad notepad-server setup
docker compose up -d
```

Open http://127.0.0.1:3000. The named `notepad-data` volume holds SQLite. Stop with `docker compose down`; do not add `-v` unless you intend to delete your server data. For a consistent live backup use SQLite's backup API/tooling, or stop the container before copying the volume; copying only the main DB while WAL is active can miss writes.

The server requires an owner password and signs users in with revocable sessions. The published port is bound to localhost. For a remote URL, configure the exact HTTPS origin and a TLS reverse proxy; see [hosting and password setup](docs/HOSTING.md). The Docker/HTTPS deployment remains unverified here. Connected clients share automatic sync logic; native execution still needs verification.

For LAN HTTPS deployment, see [Docker deployment](docs/DEPLOYMENT.md).

## Using it

- **Write / Read / Canvas** share one note. Write edits Markdown source, Read renders it, and Canvas arranges captures and linked pages.
- Use `[[Title]]` links, or **Link a note** to insert a stable reference. Read shows backlinks; **Pin page** puts a live reference on the board. See [notebook workflows](docs/NOTEBOOK.md).
- Import UTF-8 `.md` files as pages, or drop them on a canvas to create a page and reference card. **Export .md** downloads the current source.
- Drag a card's header to move it, or select it and drag its corner to resize. **Connect cards** creates an anchored arrow between two cards. Select a card, drawing or connection to delete it; Undo restores the change. See [canvas controls](docs/CANVAS.md).
- Drop raster images, `.eml` emails, web links, or plain text onto the board. The image tool opens a file chooser. Website cards open the source in a new tab.
- The email tool imports RFC 822 `.eml` files under 2 MB. Encoded headers and MIME bodies are decoded in Rust; HTML-only messages become plain text, without scripts or remote images. Email cards show the sender, recipient, date, and body. **Original .eml** downloads the complete original message, including attachments. Attachments are not opened or extracted inside the app.
- Changes save on this device and sync automatically while signed in. **Save to server** checks immediately. The footer distinguishes device saves from server sync.
- **Export** downloads the whole workspace as JSON. **Import backup** restores one after confirmation. **Load server copy** replaces local content after confirmation; export unsaved work first.
- Different page edits merge automatically. If two devices change one page, sync pauses; **Keep both versions** retains the server page and a device copy. See [sync behavior and limits](docs/NOTEBOOK.md#sync-between-devices).

Browser drafts use IndexedDB rather than localStorage. Existing localStorage drafts migrate after a successful IndexedDB commit. Saves are coalesced and ordered; the footer reports pending and failed writes. Wait for the saved message before closing. Browser storage can still be evicted or fill up, especially with images. Keep exports and save to server. This version does not yet connect to mailboxes, import Outlook `.msg` files, provide an infinite canvas, merge competing edits within one page, or ship a verified native desktop installer.

## Desktop

A Tauri shell and SQLite-backed desktop draft path now share the Rust/WASM UI. Use `bun run desktop:dev` or `bun run desktop:build` after installing the native prerequisites. `NOTEPAD_SERVER_URL` optionally points the client at your own server; no setting opens a local-only notebook. See [desktop setup and current verification limits](docs/DESKTOP.md). Native installation has not been verified here.

## Checks

```sh
bun run test
bun run check
cargo fmt --all -- --check
```

For the browser smoke test (requires Chromium):

```sh
bun install
cargo build -p notepad-server
bun run smoke
bun run smoke:persistence
bun run smoke:auth
bun run smoke:notebook
bun run smoke:sync
bun run smoke:canvas
```

The smoke tests launch isolated temporary servers on ports 3137–3140 and delete their test databases. It exercises cards, dragging, undo, drawing, link/image/email drops, original email download, safe email rendering, writing, server save, reload recovery, bundled fonts, and the mobile shell.

Fonts are bundled locally under their included SIL Open Font Licenses; the application makes no external font requests.

The notebook test covers Markdown safety, wiki/ID links, backlinks, title changes, browser history, live canvas references, UTF-16 cursor insertion, Markdown files, search and persistence. The authentication test covers protected startup, failed login, session reset, local draft recovery, cross-tab sign-out and local removal. The persistence test covers legacy migration, quota failure, offline API access, large drafts and ordered desktop IPC saves. `notepad-storage` tests SQLite draft survival and scoped server access.

`notepad-core` tests model validation and serialization. Server tests exercise SQLite persistence and optimistic concurrency through the HTTP router.
