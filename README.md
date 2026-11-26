# starstar
A 3D printer mangment system

See the [documentation index](docs/README.md).

## Overview

starstar manages Prusa MK4 and MINI printers through PrusaLink. It has:

- an **API** (Rust, `starstar-api`) with built-in users, roles (viewer, operator, admin) and an OAuth 2.1 server;
- a **web UI** (Nuxt, in `web/`) served by the API;
- a **CLI** (`starstar`) for printers, jobs, files, cameras and firmware;
- an **MCP endpoint** at `/mcp` for AI agents;
- an **agent skill** that teaches agents to use the CLI.

## Running the API and web UI

Build the web UI, then start the API. It listens on `127.0.0.1:8080`, stores data in `starstar.db` and serves `web/.output/public` (set `STARSTAR_WEB_DIR` to change it).

```sh
(cd web && npm ci && npx nuxt generate)
cargo run -p starstar-api
```

Useful settings: `STARSTAR_BIND`, `STARSTAR_PUBLIC_URL` (the URL clients use, needed for OAuth and MCP), `STARSTAR_DATABASE`. Set `STARSTAR_ALLOW_INSECURE_HTTP=1` to bind a non-loopback address with an `http://` public URL (for example on a LAN).

### First admin

With no users, the API prints a setup code at startup (or uses `STARSTAR_SETUP_TOKEN`). Create the first admin at `<public_url>/setup` in the browser, or:

```sh
cargo run -q -p starstar-cli -- setup --username admin
```

## CLI

```sh
cargo install --path crates/cli --locked   # installs `starstar`
starstar login --role admin                # opens a browser; add --no-browser on a headless machine
starstar printer add mk4 --kind mk4 --host 192.168.1.201 --username maker
starstar printer status mk4
starstar job show mk4 --json
starstar files upload mk4 usb/ prints/cube-12mm/cube-12mm.MK4.bgcode
```

`login` defaults to the operator role, which cannot register printers; use `--role admin` for `printer add` and other admin commands, and plain `starstar login` for day-to-day printing. Point the CLI elsewhere with `--api-url` or `STARSTAR_API_URL`. For scripts, set `STARSTAR_TOKEN`. `printer add` prompts for the PrusaLink password, or reads it with `--password-stdin`; the API checks it can reach the printer before saving it. Run `starstar --help` for every command.

## MCP

```sh
claude mcp add --transport http starstar <public_url>/mcp
```

Authenticate in the client; it opens a browser for sign-in. Tools are listed per role. See [the tool list](skills/starstar/references/mcp.md).

## Agent skill

```sh
npx skills add kerryhatcher/starstar
```

The skill at [`skills/starstar`](skills/starstar/SKILL.md) covers the CLI, the model, slice and print loop, monitoring, and the safety rules agents follow (agents get the operator role, not admin).

## Test print

[`prints/cube-12mm`](prints/cube-12mm/README.md) is a 12 mm test cube, already sliced for the MK4, MK4S and MINI (about 9 minutes on an MK4). Nothing in it has been sent to a printer.

## Deploying behind a proxy

The API rate-limits sign-ins by client IP. It takes that IP from the socket peer, and from `X-Forwarded-For` only when the peer is in `STARSTAR_TRUSTED_PROXIES`. When `STARSTAR_BIND` is a loopback address (the default) and the variable is unset, `127.0.0.1/32` and `::1/128` are trusted. On any other bind address nothing is trusted by default. The effective list is printed at startup.

Every trusted hop must **overwrite or append** the real client address to `X-Forwarded-For`. The API believes the rightmost entry that is not a trusted proxy. For nginx:

```nginx
location / {
    proxy_pass http://127.0.0.1:8080;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
}
```

Caddy and Traefik do this by default. Do not use TCP-level forwarders (`socat`, `ssh -L`/`-R`, `stunnel`, HAProxy `mode tcp`, `kubectl port-forward`, `tailscale serve --tcp`) with the loopback default. They add no header, so every client appears as `127.0.0.1` and shares one rate-limit key. A hop that passes a client's own `X-Forwarded-For` through unchanged lets that client choose its key. The API logs a warning once when a loopback peer sends no `X-Forwarded-For`.

As a backstop, failed sign-ins are also limited per username (20 per 15 minutes) whatever the client IP. A persistent attacker who knows a username can therefore keep that account locked for as long as they keep failing, in 15-minute windows, which is the price of bounding password guessing when the client address cannot be trusted.
