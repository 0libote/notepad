# Validation

2026-10-29, Linux development environment.

- Shared Rust model: serialization round trip, default validation, invalid links/duplicate identifiers, unsupported schema, nonfinite geometry.
- Axum router and SQLite: first save succeeds, stale revision returns conflict, updated content survives closing and reopening the database.
- WASM: client target type check and Trunk bundle build.
- Chromium: isolated browser context and disposable server/database; card creation, pointer movement, undo, freehand stroke, website drop, raster image drop, text editing, server save, local reload recovery, bundled font loading and mobile viewport containment. Script: `scripts/smoke.mjs`.
- Rust formatting and Clippy for core/server.
- Hallmark: static CSS checks and rendered checks at desktop and phone widths. Review recorded in `.hallmark/review.md`.

Docker is not installed in this environment. Dockerfile and Compose are supplied but an image build/start was not exercised. No desktop binary, real pen/touch hardware, browser compatibility matrix, long-running storage stress test, multi-device merge, or authenticated deployment was tested.

Final optimized release bundle built successfully with `trunk build --release`. The smoke test, including pointer and keyboard movement, passed against the release assets with no browser errors. Final Hallmark static/render sweep: 0 FAIL, 0 REFLEX, 0 WARN. Core/server Clippy and Rust formatting passed.

## Email capture

Added Rust MIME tests for encoded display names/subjects, ISO-8859-1 quoted-printable text, base64 bodies, preferred plain alternatives, mixed inline HTML/text, attachment exclusion, HTML-only conversion, malformed input, oversized messages and exact original-byte preservation. Workspace tests cover older cards without email metadata, new email serialization and invalid/missing source metadata. Nine core tests plus the server persistence/concurrency test pass.

The release build and extended browser smoke test pass. The browser test verifies `.eml` drag/drop, decoded metadata, inert HTML-only display, no remote tracking-image request, exact downloaded `.eml` bytes, invalid-file rejection without content changes, persistence after a server save and reload, and retaining the destination note during a deliberately delayed file read. Final Hallmark sweep is clean at desktop and phone sizes. Core/server Clippy and formatting pass. Docker and native desktop remain unverified.

## Durable drafts and desktop foundation

Browser drafts now use IndexedDB with explicit transaction commits. A single writer coalesces changes and keeps newer snapshots behind any in-flight save. Existing localStorage content migrates only after validation and a successful IndexedDB commit. Persistence state is separate from remote save status; pending browser writes register a before-unload guard.

The persistence browser test passes for migration, rapid-edit coalescing, an unavailable host API, injected quota failures that retain the previous good draft, retry after failure, a valid workspace larger than 5 MB, corrupt legacy-copy protection, and ordered desktop IPC saves with an edit during an in-flight write. The desktop IPC path is mocked in Chromium and is not a native WebView test. Portable SQLite/client tests cover reopening drafts, server namespace isolation, invalid-server rejection, manual HTTP round trips and retained drafts after disconnection.

A Tauri shell, generated icons, narrow command capabilities, packaged-asset CSP and environment-configured server bridge are present. Linux prerequisites are absent (GTK/WebKit development packages). A Windows cross-check reached the missing `x86_64-w64-mingw32-gcc` requirement. No native installer or WebView launch is claimed. Native close handling, link opening, file download dialogs and single-instance behavior remain to be verified. CI includes a native Linux check with the required packages.

Final checks for this increment: optimized WASM build; 13 Rust tests; both Chromium suites against the final release assets; Clippy for core/server/storage and the WASM frontend; formatting for both workspaces; and the Hallmark rendered/static sweep all pass. The sweep reports 0 FAIL, 0 REFLEX, 0 WARN.

## Authenticated hosting

The single-owner server now blocks workspace reads/writes before setup or without a valid session. Argon2id hashes, random session issuance, hashed session storage, expiry, logout, password-reset revocation, secure cookie flags, origin/custom-header mutation guards, login throttling, and uncached private API responses have focused Rust coverage. Password reset preserves the notebook. Unix database permissions are restricted to the owner.

The browser authentication test covers failed login and cleared password inputs, HttpOnly cookies, session invalidation through the setup CLI, unsynced local-draft recovery after reauthentication, successful authenticated save, explicit local deletion at logout, notification/removal in another tab, and reloading the last server version after sign-in. Portable desktop tests verify in-memory tokens and authenticated HTTP requests without putting tokens in SQLite. The normal editor and persistence tests now start configured owner fixtures rather than bypassing authentication.

The local service has been restarted with the protected API. Before owner setup, `/api/auth/state` reports unconfigured and unauthenticated, and `/api/workspace` returns HTTP 503 instead of note content. No default owner password has been generated. The optional Caddy/Compose HTTPS deployment has not been exercised because Docker is unavailable. Native installation and OS integration remain unverified.

Final authentication checks: 19 Rust tests pass; the normal editor, persistence and authentication Chromium suites pass against the optimized release; core/server/storage and WASM Clippy pass; formatting and diff checks pass. The rendered/static Hallmark sweep reports 0 FAIL, 0 REFLEX, 0 WARN. The running SQLite database is mode 0600. The private owner-password input timed out, so the real instance remains unconfigured and fail-closed; only isolated test instances have test passwords.

The final sign-in review includes desktop and phone captures, a regression assertion for visible phone persistence status, and a browser assertion that private/authentication GETs use `no-store`. All three browser suites pass against the final release assets after these changes. The final design sweep remains clean.

## Notebook reading and references

Added Rust Markdown rendering, wiki/ID link resolution, rename retargeting, backlinks, captured-content search, Markdown source import/export, URL navigation, and live canvas references to pages. The parser minimum is pulldown-cmark 0.13.5; an event-count regression covers the patched empty-pipe wikilink case. Raw HTML is escaped, external image URLs remain explicit links, unsupported link schemes are inert, and document sizes are bounded. Note/card identifiers must round-trip through DOM attributes and URLs, so empty, overlong and control-character identifiers are rejected.

The notebook browser suite exercises semantic Markdown, blocked active HTML/remote image loads, aliases, missing-note creation, backlinks, renaming without broken references, browser history and reload, live note previews on the canvas, Unicode selection insertion, exact source export, Markdown drops, captured-content search, invalid UTF-8 rejection, authenticated server saves and mobile containment. It also imports a backup reusing a card ID for another target and verifies the view updates and Undo restores the original target.

Final notebook increment: 28 Rust tests pass. The notebook, editor/capture, persistence and authentication browser suites all pass against the final optimized assets. Core/server/storage and WASM Clippy, both workspace formatting checks, and the design sweep pass. The sweep reports 0 FAIL, 0 REFLEX, 0 WARN. Desktop/phone reading and linked-canvas captures were inspected. The documented Bun build entrypoint also succeeds after installing the WASM target for the separate Rust toolchain selected by this environment's Bun shim. The running Rust server has been updated; owner setup remains an operator action, and Docker/TLS/native package execution is still unverified.

## Automatic sync and conflict recovery

Added a Rust page-level three-way merge and shared WASM sync coordinator. Browser IndexedDB and native SQLite save the draft and last-known server baseline atomically. Open, authenticated clients debounce uploads and poll for remote changes. Independent page changes merge; competing page changes pause for an explicit Keep both versions action. In-flight edits survive upload responses. An exclusive Web Lock prevents two tabs from overwriting one browser cache with divergent offline drafts; an unsupported browser stays paused.

The new two-profile Chromium suite verifies offline reload with its baseline, automatic independent-page merge, same-page conflict persistence across reload, preservation of both versions, continued typing during an upload, a committed request whose response is lost, idle remote polling, simultaneous stale PUT rejection and merge on retry, baseline-write transaction rollback with upload prevention, recovery after storage becomes writable, and cache-lock release after closing the editing tab. The persistence suite also checks a browser without Web Locks. The authentication suite checks cross-tab logout with the second tab paused by the lock.

Final sync increment: 32 Rust tests pass (21 core, 6 server, 5 storage). All five Chromium suites pass against the optimized release assets. Rust formatting, core/server/storage Clippy, WASM Clippy, and diff checks pass. The static/render Hallmark sweep reports 0 FAIL, 0 REFLEX, 0 WARN. The phone conflict notice was inspected. The current server is running the updated build and remains unconfigured/fail-closed pending operator password setup. Docker/TLS deployment and native package execution remain unverified. Same-page automatic text merging, background sync while closed, separate attachment storage and native OS integration remain outside this increment.

## Canvas editing and format safety

Cards now support optional persisted dimensions, and boards own connections anchored to card IDs. The Rust geometry tests cover frame validation, world bounds, endpoint placement, legacy layout compatibility, connection validation, and removal of attached connections. The canvas is a separate Rust component; keyed cards and SVG objects remain stable during movement instead of cloning/remounting every card payload.

The new Chromium canvas suite exercises pointer and keyboard resizing, movement, gesture Undo, anchored connections following movement and size changes, stable DOM nodes, selection/removal of cards and ink (including dots), cascading connection removal and restoration, duplication, text-field Backspace protection, cancellation, pointer ownership with a second synthetic touch, zoom/scroll coordinates without focus jumps, all capture-card types, reload, authenticated two-profile geometry sync, invalid imports and phone containment. Desktop and phone screenshots were inspected. Physical pen/touch hardware has not been tested.

Workspace format 2 prevents older clients from silently dropping sizes and connections. Stored format-1 server data upgrades in a transaction without changing content or revision. Browser IndexedDB/localStorage drafts, sync baselines, native SQLite drafts and JSON imports upgrade on read. Unknown/invalid versions fail closed. Rust tests cover server/draft migration and preservation of unsupported data. Browser tests exercise old draft/baseline and backup migration, plus an authenticated format-1 write at the correct revision being rejected with HTTP 422 while the server notebook remains unchanged.

Final canvas increment: 39 Rust tests pass (26 core, 7 server, 6 storage). All six browser suites pass against the optimized format-2 build. Core/server/storage and WASM Clippy, workspace/desktop formatting, and diff checks pass. The static/render Hallmark sweep reports 0 FAIL, 0 REFLEX, 0 WARN. The updated service is healthy and remains unconfigured/fail-closed pending owner setup. Docker/TLS execution, native packages and OS integration remain unverified. Redo, multi-selection, richer drawing tools, infinite-canvas rendering and separate attachment storage remain follow-up work.

## Public repository and HomeServer deployment

Created the public `0libote/notepad` repository and pushed the implementation as 0libote. GitHub's Rust, Linux desktop compilation and Docker-build jobs pass. Desktop compilation now has native-system-library coverage; installer generation and WebView execution remain unverified.

Built the Docker image from application source on HomeServer and deployed the LAN HTTPS profile with persistent notebook and Caddy volumes. TLS validates with the copied public CA certificate, including direct-IP clients without SNI. The app runs as UID 10001 and its database has mode 0600. The production instance remains deliberately unconfigured and returns HTTP 503 for private notebook reads because the private owner-password request was declined.

A disposable deployment using the same image, separate volumes and a test-only owner password passed browser checks for HTTPS secure contexts, Web Locks, Secure/HttpOnly session cookies, anchored canvas edits, server saves, reload, offline recovery, cross-origin mutation rejection and phone layout. Fresh authenticated HTTP reads after a container restart verified that note text, dimensions and connections persisted. All 39 Rust tests also passed inside a Docker builder container on HomeServer. The temporary test deployment and its volumes were removed afterward. Public-DNS deployment and native desktop execution remain unverified; production owner setup and client certificate trust are operator actions.

ZimaOS dashboard registration: the Docker-only deployment initially appeared as an unmanaged Compose app. Added LAN profile metadata and registered a managed manifest under `/var/lib/casaos/apps/notepad`, retaining the existing external notebook and Caddy volumes. ZimaOS's compose-app API reports `notepad` as running and managed, with the correct Notepad title, icon and HTTPS URL. Restart through the ZimaOS CLI succeeds and the CA-verified health endpoint remains healthy. No notebook contents or owner credentials were changed.

Cloudflare opening fix: the public hostname returned HTTP 400 because the tunnel sent HTTP to a TLS listener. Aligned the existing origin endpoint with the tunnel's HTTP transport, changed the backend's allowed origin to the public HTTPS URL, and updated dashboard launch metadata. Public health checks now return HTTP 200. A fresh Chromium context with normal certificate verification loads the owner-setup UI at the exact public URL, with a secure context, Web Locks, and no JavaScript exceptions. Notebook data and owner credentials were preserved; private access remains locked until owner setup.

## First-load owner setup

Added a browser first-run form and native-client bridge for choosing the owner password. There are no password length or complexity checks; short and empty choices are accepted. Setup stores an Argon2id hash and creates a session in the same transaction. A concurrent or later setup request receives HTTP 409, so it cannot replace an established owner. Host-side password reset remains available without strength rules. Mutation-origin checks, request-size bounds, hashing concurrency limits and login throttling remain in place.

Local verification: 41 Rust tests pass. The onboarding suite covers one-character/empty password choices, immediate sign-in, reload, sign-out/sign-in, later-device sign-in and owner replacement protection. The other six browser suites pass. The notebook regression now waits for the backup control to be enabled before upload, avoiding a race with automatic sync. Clippy, formatting and the clean design sweep pass. Desktop and phone setup captures were inspected.

HomeServer verification: rebuilt and deployed the image, then verified the public first-run form over Cloudflare HTTPS with normal certificate checks. The real notebook remains unconfigured so its owner can choose the password. A disposable Docker container on HomeServer passed first-run setup with a one-character password, immediate authenticated access, reload, refusal of later setup attempts, sign-out and sign-in. The test now waits for the device-save indicator before reloading, respecting the existing unsaved-work navigation guard. Temporary test storage and the SSH forward were removed afterward.

## Drawing editor (2026-10-31)

Workspace format 3 preserves existing cards, connectors and legacy strokes while adding styled shapes and ink. Core tests cover shape geometry, bounds, styling, deterministic sketch paths and exact floating-point JSON round trips. Canvas regression checks exercise mouse focus inside cards, anchored connections, keyboard/pointer resizing, undo, coordinate conversion under pan/zoom, and long-decimal geometry syncing without drift. The drawing suite covers creation, styles, text, resize, selection, ink, erasing, multi-selection, redo, cursor zoom, save/reload and phone containment. Rotation, grouping, shape-bound arrows and real pen hardware remain outside this pass.
