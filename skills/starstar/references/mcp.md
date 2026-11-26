# starstar MCP server

The API serves MCP over streamable HTTP at `<public_url>/mcp` (`STARSTAR_PUBLIC_URL` on the server, default `http://127.0.0.1:8080`).

## Connect

```sh
claude mcp add --transport http starstar <public_url>/mcp
```

Then authenticate from the client (in Claude Code, `/mcp`). A browser opens for the OAuth sign-in and consent page; the client gets a token limited to the role you pick. Choose **operator** for an agent. `tools/list` only shows tools the token's role may call, and a call above the role fails.

## Tools

| Role | Tools |
|---|---|
| viewer | `list_printers`, `get_printer`, `printer_info`, `printer_status`, `list_storage`, `get_job`, `get_transfer`, `list_files`, `file_state`, `list_cameras`, `get_camera`, `camera_snapshot` |
| operator (adds) | `upload_file`, `start_print`, `pause_job`, `resume_job`, `continue_job`, `stop_job`, `cancel_transfer`, `take_snapshot` |
| admin (adds) | `register_printer`, `remove_printer`, `delete_file`, `order_cameras`, `register_camera`, `remove_camera`, `configure_camera`, `reset_camera_config`, `connect_camera`, `disconnect_camera`, `check_update`, `apply_update` |

`upload_file` takes base64 content and is capped (`STARSTAR_MCP_MAX_UPLOAD_MB`, default 25 MB); for bigger files use the CLI.

## Safety

Same rules as the CLI: call `printer_status` and `get_job` before `upload_file` or `start_print`, and call `start_print`, `stop_job`, `delete_file`, `remove_printer` and `apply_update` only on the user's go-ahead, and `pause_job`, `resume_job` and `cancel_transfer` only when asked. Destructive tools are annotated `destructive`; clients may prompt, but do not rely on that.
