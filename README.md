# starstar
A 3D printer mangment system

See the [documentation index](docs/README.md).

## Usage

Start the API (it listens on `127.0.0.1:8080` and stores printers in `starstar.db`):

```sh
cargo run -p starstar-api
```

Then use the CLI. Set `STARSTAR_API_URL` or pass `--api-url` to point it elsewhere.

```sh
cargo run -p starstar-cli -- printer add mk4 --kind mk4 --host 192.168.1.201 --username maker
cargo run -p starstar-cli -- printer list
cargo run -p starstar-cli -- printer info mk4
cargo run -p starstar-cli -- printer status mk4
cargo run -p starstar-cli -- printer storage mk4
```

`printer add` prompts for the password, or reads it from stdin with `--password-stdin`. The API checks it can reach the printer before saving it.

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

As a backstop, failed sign-ins are also limited per username (20 per 15 minutes) whatever the client IP. Someone who knows a username can therefore lock that account for up to 15 minutes, which is the price of bounding password guessing when the client address cannot be trusted.
