# Building Notepad

Notepad is a personal notebook with a spatial working surface. Keep the first product small enough that writing, collecting, drawing, and retrieving feel immediate. This is an early working foundation, not yet a secure public service or a complete Obsidian replacement.

## Current implementation

- `notepad-core`: versioned, serializable Rust workspace model and input validation. Notes own Markdown text, movable/resizable cards, anchored card connections, and vector strokes. Email cards add optional metadata and a base64-encoded original message; format-1 notebooks, drafts and backups upgrade to format 2 while preserving their content. Network writes require the current format so older clients cannot drop new fields.
- `notepad-web`: Leptos client-side rendering compiled to WebAssembly. Browser APIs are called through Rust's `web-sys`; no handwritten JavaScript application. HTML/CSS and generated WASM loader JavaScript are still necessary for the web. Bun only runs development commands.
- `notepad-storage`: native SQLite drafts scoped to a server identity, plus a Rust HTTP client. The portable backend is tested without a GUI.
- `notepad-desktop`: Tauri shell, packaged web assets and narrowly scoped IPC commands; native build/launch remains unverified.
- `notepad-server`: Axum API and static asset hosting. SQLite in WAL mode holds one workspace. Transactional revisions reject stale writes with HTTP 409.
- Browser IndexedDB stores ordered, coalesced draft snapshots. Existing localStorage notes migrate after the new transaction commits. A pending/error status and before-unload prompt protect uncommitted browser edits. A draft and its last-known server baseline commit atomically. Automatic sync uses a page-level three-way merge: independent page changes combine, and conflicting page edits pause until Keep both versions preserves the server page plus a device copy. Upload responses advance the revision and baseline while preserving edits made in flight. Open clients poll about every 15 seconds. JSON export/import provides a portable backup.

The canvas is a bounded 2600 × 1800 world with scroll and zoom, not yet an infinite GPU canvas. Card positions and strokes are world coordinates. Cards are DOM objects; drawings and connections are SVG paths. Optional dimensions preserve legacy automatic card layouts; connections refer to card IDs and recalculate their border anchors after movement, resize, or content layout changes. See [canvas controls](CANVAS.md). This makes the first interactions accessible to inspection without prematurely introducing a rendering engine.

The same note has a page (Markdown source in Write, rendered content in Read) and a canvas. Note cards reference a page by persistent ID and show a live preview; text/sticky cards remain independent. Arbitrary block/range references are not yet supported. See [notebook workflows](NOTEBOOK.md).

## Why these tools

[Leptos CSR](https://book.leptos.dev/getting_started/index.html) runs Rust UI code as WASM, with Trunk handling the generated browser bridge. It preserves a DOM-based text editing path. Using an all-canvas UI would make selection, IME composition, screen readers, and mobile text editing much harder.

[Axum](https://docs.rs/axum/latest/axum/) offers a small typed Rust HTTP layer. SQLite lets a personal Docker install persist into one volume with no database service to administer. Do not add Postgres until shared hosting or concurrent workload warrants it.

[Tauri 2](https://v2.tauri.app/start/frontend/) accepts static assets including WASM and uses the operating system webview. It is the intended desktop shell for this same UI. The desktop shell, SQLite draft path and Rust server bridge are implemented; native build/launch and OS integration are still unverified. See [desktop instructions](DESKTOP.md). A shell alone does not make editing faster: the benefit must come from a local database, local assets, background synchronization, and measured startup time.

[Yrs](https://docs.rs/yrs/latest/yrs/) implements Yjs-compatible CRDT shared types in Rust. It is a candidate for future per-note collaboration and offline merging, not a dependency today. Whole-workspace snapshots with optimistic revisions are much simpler for this milestone, and now merge changes to different pages. Same-page edits require recovery copies; a CRDT would allow finer-grained merging.

Study the document and spatial workflows of [Obsidian Canvas](https://obsidian.md/canvas) and [Excalidraw](https://github.com/excalidraw/excalidraw). Reuse suitable open file formats and design lessons. Their JavaScript/TypeScript editing code is not a Rust-native engine. License review is necessary before copying code or vendoring dependencies; no source from either is copied here.

## Email import

[mailparse](https://docs.rs/mailparse/latest/mailparse/) handles encoded headers, transfer encodings, and character sets. [html2text](https://docs.rs/html2text/latest/html2text/) converts HTML bodies to inert text entirely in Rust. A multipart alternative prefers the plain-text representation; mixed inline parts are retained. Attached text and attached messages are kept out of the body display. There are no email iframes, HTML injection, image fetches, or remote link previews.

The browser reads raw ArrayBuffer bytes, preserving non-UTF-8 originals. Async imports retain their destination note ID so switching notes while a file is read cannot misfile a capture. Original-source download uses an explicit user action and a fixed `.eml` filename. Source, metadata, and body travel together through cache, server snapshots and JSON backups.

## Next milestones, in order

1. **Trust the notebook.** Separate content-addressed attachment storage, schema migrations, recovery history, multi-selection, richer pen input and shapes. IndexedDB removes the small localStorage ceiling and coalesces edits, but large snapshots and inline attachments must still be replaced before heavy use. Add browser tests for text composition, reload recovery and interrupted saves.
2. **Desktop and offline.** Verify and distribute the Tauri package and its SQLite/server bridge. Add separate attachment files, an in-app server setting, native close safeguards, external-link/download integration and single-instance handling. The durable draft holds pending changes while offline; use the same note IDs and coordinate model in both clients. Measure cold startup and large boards before claiming a speed advantage.
3. **Conflict-safe sync.** Move from one workspace snapshot to per-note operations; evaluate Yrs for text and spatial properties. Page-level merge, explicit conflict recovery, and automatic sync are implemented. Extend the tested offline, lost-response and in-flight-edit cases to per-note operations and collaboration. The single-owner server now authenticates sessions, rejects cross-origin writes and throttles login; verify the TLS/container deployment before claiming public-hosting readiness. Multi-user isolation remains separate future work.
4. **Collection and retrieval.** Full-text indexing via SQLite FTS5, folders/tags, bulk vault import and richer retrieval. Markdown reading, wikilinks, stable ID links, backlinks and individual Markdown capture/export are now implemented. Add website metadata retrieval only behind SSRF-safe egress rules. Extend local .eml capture with attachment inspection and better previews, keeping email HTML inert. Arbitrary mailbox integration requires a separate authorization and provider strategy.
5. **Scale the canvas.** Spatial indexing, viewport culling, simplified strokes, and profiling with large boards. Only adopt a GPU renderer if DOM/SVG profiling proves it necessary. Keep text editing in DOM overlays.

## Current operational limits

Single owner and workspace, with Argon2id password verification, revocable sessions, origin guards and login throttling. No multi-user isolation. Compose binds localhost intentionally. Remote browser access requires the configured HTTPS origin and a reverse proxy; see [hosting](HOSTING.md). Container/TLS deployment is still unverified. The IndexedDB browser draft is per origin, unencrypted, and is not a backup. An exclusive Web Lock allows one editing tab per browser profile to protect the shared draft; other profiles/devices can edit concurrently. Private browsing, clearing site data, changing URL/port, or quota failures affect it. The server rejects stale revisions. The client merges independent page changes and pauses same-page conflicts for Keep both versions recovery. Keep JSON exports as independent backups.

Images are currently inline base64 with a 4 MB input-file limit, six-million-character per-image model limit, and a 12 MiB server request limit. Only raster image types are accepted. Website drops create outbound links, not embedded live websites; text drops create plain text cards. RFC 822 .eml email files under 2 MB are decoded locally with mailparse and html2text, rendered as escaped plain text, and retained byte-for-byte as base64 for original-file download. Attachments remain in the original file; mailbox accounts and Outlook .msg imports are not implemented. Zoom and pan use a scrollable canvas. There are no remote embeds, arbitrary HTML rendering, mailbox access, CRDT sync, verified desktop installers, or fully offline web app-shell installation in this build.
