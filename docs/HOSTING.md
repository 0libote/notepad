# Hosting your notebook

Notepad currently supports one owner and one workspace per server. The server refuses workspace reads and writes until an owner password is configured. Static app assets and health/sign-in state remain public; they contain no notes.

## First run

Locally:

```sh
cargo run -p notepad-server -- setup
cargo run -p notepad-server
```

The setup command prompts twice without echoing the password. Choose at least 12 characters. It writes an Argon2id hash to SQLite, never a plaintext password. Then open http://127.0.0.1:3000 and sign in. Keep the same `NOTEPAD_DATA` directory for both commands. Existing notes survive setup.

With Docker:

```sh
docker compose build
docker compose run --rm -it notepad notepad-server setup
docker compose up -d
```

The setup container and service share the named data volume. The normal published port remains bound to localhost. No default password is provided.

For an automation-managed secret, the setup command also accepts `setup --password-file PATH`. Mount the secret read-only and ensure the container user can read it. Do not put the password in a command-line argument or commit a secret file. The file's final line break is ignored; spaces are preserved.

## Public HTTPS URL

`NOTEPAD_PUBLIC_URL` must be the exact origin users open, such as `https://notes.example.com`. Remote HTTP origins are rejected. Loopback HTTP is allowed for local development. No URL path prefix is supported.

The repository includes an optional Caddy example:

```sh
export NOTEPAD_DOMAIN=notes.example.com
docker compose -f compose.yaml -f compose.public.yaml build
docker compose -f compose.yaml -f compose.public.yaml run --rm -it notepad notepad-server setup
docker compose -f compose.yaml -f compose.public.yaml up -d
```

Replace the example hostname with your own. Its DNS must point to your host and ports 80/443 must reach Caddy. The public override sets the app origin and proxies traffic to the internal Rust service. Caddy manages certificates and redirects HTTP to HTTPS according to its [HTTPS documentation](https://caddyserver.com/docs/quick-starts/https). Keep the Caddy volumes so certificate state persists. The base service's localhost port remains available for host diagnostics, but open the configured HTTPS URL to sign in.

With another reverse proxy, preserve `Origin` and proxy the entire origin, including `/api`. Set `NOTEPAD_PUBLIC_URL` explicitly. The app checks that state-changing browser requests come from that origin; it does not trust forwarded headers to decide whether a cookie is secure.

The LAN Docker/Caddy deployment has been exercised on HomeServer with a trusted local CA, secure-cookie authentication, browser editing, offline recovery and container restart persistence. See [LAN deployment](DEPLOYMENT.md). The public-DNS configuration remains unverified. Native package installation and WebView launch are also outstanding.

## Sessions and local copies

Browser sessions use HttpOnly, SameSite=Strict cookies. HTTPS uses a Secure `__Host-` cookie. Session tokens contain 32 random bytes; only their SHA-256 digests are stored server-side. Sessions have a seven-day absolute expiry. Login verification is capped and rate-limited. Cross-origin writes and cross-site forms are rejected. API responses use `Cache-Control: no-store`.

A session expiring does not erase a local draft. Sign in again to resume automatic sync. Existing local edits are compared with the server; a same-page conflict pauses sync and offers Keep both versions. Load server copy remains an explicit replacement operation. This is a local-cache application: server authentication is not encryption or an OS-level lock on IndexedDB/SQLite. Device users with access to the profile may read cached notes.

Browser Sign out asks before deleting the local copy, because unsynced changes would be lost. Export or save first. Successful sign-out revokes the session, removes the browser draft and signals other open tabs to do the same. Desktop Sign out revokes the server session but keeps local SQLite drafts, as its confirmation explains. Desktop session tokens stay in Rust memory and are not written to SQLite or browser storage; restarting a connected desktop client requires sign-in again.

## Reset the password

Run the same setup command against the existing data directory/volume. It replaces the password hash and revokes all existing sessions while preserving notes. Password recovery currently requires host access; there is no email recovery or public registration.

## Development

Trunk's proxy runs on a different browser origin. Start the Rust server with:

```sh
NOTEPAD_PUBLIC_URL=http://127.0.0.1:8080 bun run server
```

Then run `bun run dev` and open http://127.0.0.1:8080. Use that origin consistently; direct port-3000 browser mutations are intentionally rejected in this configuration. Browser caches are per origin.
