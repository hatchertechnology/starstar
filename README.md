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
