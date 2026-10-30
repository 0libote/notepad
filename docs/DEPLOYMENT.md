# Docker deployment

For a LAN server, use `compose.lan.yaml`. It runs the Rust service behind Caddy on HTTPS port 8443, with separate persistent notebook and certificate volumes. It does not publish the app's HTTP port. Set `NOTEPAD_HOST` to the server's LAN IP or hostname in a local `.env` file (ignored by Git).

```sh
printf 'NOTEPAD_HOST=YOUR_LAN_ADDRESS\n' > .env
docker compose -p notepad -f compose.lan.yaml up -d --build
docker compose -p notepad -f compose.lan.yaml exec notepad notepad-server setup
```

The setup command prompts privately for an owner password of at least 12 characters. There is no default password. Before setup, the sign-in state reports unconfigured and private notebook requests return HTTP 503. Do not use the SSH account password as an app default.

Caddy issues a local HTTPS certificate. The LAN configuration sets a [default SNI](https://caddyserver.com/docs/caddyfile/options#default-sni) so clients connecting directly to an IP address can select that certificate. Copy its **public root certificate** and install it as a trusted certificate on each client device:

```sh
docker compose -p notepad -f compose.lan.yaml cp caddy:/data/caddy/pki/authorities/local/root.crt ./notepad-lan-root.crt
```

Only copy `root.crt`; leave the private CA key in the Caddy volume. After trusting the root, open `https://YOUR_LAN_ADDRESS:8443`. Browser draft protection requires a secure context. For a public DNS name and publicly trusted HTTPS, use the separate `compose.public.yaml` instructions in [HOSTING.md](HOSTING.md).

## Operations

```sh
docker compose -p notepad -f compose.lan.yaml ps
docker compose -p notepad -f compose.lan.yaml logs --tail=100
docker compose -p notepad -f compose.lan.yaml restart notepad
```

The notebook volume is `notepad_notepad-data`; certificate volumes are `notepad_caddy-data` and `notepad_caddy-config`. Stop the app before taking a filesystem-level SQLite backup, or use SQLite's backup API. Keep independent JSON exports. `down` keeps named volumes; `down -v` deletes them and must not be used for routine updates.

For updates, back up, fetch the desired Git revision, stop the old service, rebuild and start the matched backend/web bundle. Reload browser clients afterward. Format-1 data upgrades transactionally; older clients cannot write over the newer canvas fields. Native desktop packaging is a separate deliverable.

## Deployment verification

The LAN profile has been tested on HomeServer with the source-built Docker image. Checks covered TLS validation with the copied CA certificate, an unconfigured production notebook rejecting private reads, and a separate disposable instance exercising secure cookies, Web Locks, canvas editing, server saves, reload, offline recovery, origin rejection, phone layout, and persistence after a container restart. The temporary instance uses separate project volumes and is removed afterward.

`bun run smoke:deployment` targets a **disposable** HTTPS instance configured with the test-only fixture password from `scripts/test-server.mjs`. Set `NOTEPAD_TEST_URL` to that instance. It deliberately changes notebook content and must not target a personal notebook. Its temporary Chromium context accepts the local test certificate; production TLS is verified independently with `curl --cacert`. Install the public CA certificate on real client devices rather than disabling certificate verification.

HomeServer operation files live under `/DATA/AppData/notepad`, with the Git checkout in `repository`. The deployment has no default owner password: if setup was not completed privately, run the setup command from an interactive SSH terminal. Docker permissions on that host require `sudo`.

## ZimaOS dashboard

A Docker-only Compose launch does not automatically create a managed ZimaOS dashboard tile. The LAN profile includes `x-casaos` metadata for the Notepad title, icon and HTTPS launch URL. On HomeServer, the managed manifest is `/var/lib/casaos/apps/notepad/docker-compose.yml`; it references the original notebook and certificate volumes as external volumes. ZimaOS's app API lists `notepad` as managed and running, and its restart control has been verified.

For this source-built installation, rebuild from the Git checkout, then recreate through the managed manifest so the dashboard registration remains attached:

```sh
cd /DATA/AppData/notepad/repository
sudo docker compose -p notepad -f compose.lan.yaml build
sudo docker compose -p notepad -f /var/lib/casaos/apps/notepad/docker-compose.yml up -d --no-build --pull never --force-recreate
```

The image is local (`notepad-notepad:latest`), so rebuilding supplies updates rather than an app-store image pull. The dashboard tile opens the existing HTTPS address; certificate trust and app owner setup still apply.

## Cloudflare Tunnel

For an existing installation behind a host-networked Cloudflare connector, `compose.cloudflared.yaml` reuses the notebook and certificate volumes. Set `NOTEPAD_PUBLIC_HOST` to the public hostname (without `https://`). The profile defaults to an HTTP origin bound only to localhost on port 3087; the tunnel's service should be `http://127.0.0.1:3087`. Cloudflare provides publicly trusted HTTPS at the external URL. Do not send HTTP to the LAN profile's HTTPS port.

The public hostname must also match `NOTEPAD_PUBLIC_URL` so browser sign-in and save requests pass the origin guard. The dashboard metadata uses the public hostname and HTTPS port 443. Do not put a complete URL into the hostname field, since the dashboard adds the scheme itself.

HomeServer's existing tunnel pointed to HTTP on its original origin port. Its managed manifest now serves HTTP on that port for the tunnel's local hop and sets the public origin to `https://notepad.0liman.uk`. The dashboard opens that public URL; the former LAN HTTPS URL is no longer its entry point. This deployment was verified in Chromium without disabling certificate checks: the app loads over trusted public TLS, Web Locks are available, and owner setup is still required before notebook access.
