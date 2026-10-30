# Docker deployment

For a LAN server, use `compose.lan.yaml`. It runs the Rust service behind Caddy on HTTPS port 8443, with separate persistent notebook and certificate volumes. It does not publish the app's HTTP port. Set `NOTEPAD_HOST` to the server's LAN IP or hostname in a local `.env` file (ignored by Git).

```sh
printf 'NOTEPAD_HOST=YOUR_LAN_ADDRESS\n' > .env
docker compose -p notepad -f compose.lan.yaml up -d --build
docker compose -p notepad -f compose.lan.yaml exec notepad notepad-server setup
```

The setup command prompts privately for an owner password of at least 12 characters. There is no default password. Before setup, the sign-in state reports unconfigured and private notebook requests return HTTP 503. Do not use the SSH account password as an app default.

Caddy issues a local HTTPS certificate. Copy its **public root certificate** and install it as a trusted certificate on each client device:

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
