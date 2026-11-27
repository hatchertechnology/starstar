# starstar MCP server

The configured MCP endpoint for this deployment is `http://starstar/mcp` over streamable HTTP.

Thor’s configured public origin gives the endpoint `https://starstar.tailb56d83.ts.net/mcp`.

The server uses `<public_url>/mcp`, with `STARSTAR_PUBLIC_URL` defaulting to `http://127.0.0.1:8080`. The CLI API URL separately defaults to `http://starstar`.

## Connect

```sh
claude mcp add --transport http starstar http://starstar/mcp
```

Then authenticate from the client (in Claude Code, `/mcp`). A browser opens for the OAuth sign-in and consent page; the client gets a token limited to the role you pick. Choose **operator** for an agent. `tools/list` only shows tools the token's role may call, and a call above the role fails.

## Tools

| Role | Tools |
|---|---|
| viewer | `list_printers`, `get_printer`, `printer_info`, `printer_status`, `list_storage`, `get_job`, `get_transfer`, `list_files`, `file_state`, `list_cameras`, `get_camera`, `camera_snapshot`, `part_list`, `part_info` |
| operator (adds) | `upload_file`, `start_print`, `pause_job`, `resume_job`, `continue_job`, `stop_job`, `cancel_transfer`, `take_snapshot`, `part_match` |
| admin (adds) | `register_printer`, `remove_printer`, `delete_file`, `order_cameras`, `register_camera`, `remove_camera`, `configure_camera`, `reset_camera_config`, `connect_camera`, `disconnect_camera`, `check_update`, `apply_update` |

Parts tool arguments:

- `part_list`: optional `query` (search part names, file names and profiles) and optional `sort` (`newest` by default, or `name`).
- `part_info`: required `id` (part ID).
- `part_match`: required `id` (part ID), optional `setup_id` (defaults to the part's default setup), and optional `profile` (defaults to the setup's default profile).

`upload_file` takes base64 content and is capped (`STARSTAR_MCP_MAX_UPLOAD_MB`, default 25 MB); for bigger files use the CLI.

## Safety

Same rules as the CLI: call `printer_status` and `get_job` before `upload_file` or `start_print`, and call `start_print`, `stop_job`, `delete_file`, `remove_printer` and `apply_update` only on the user's go-ahead, and `pause_job`, `resume_job` and `cancel_transfer` only when asked. Destructive tools are annotated `destructive`; clients may prompt, but do not rely on that.
