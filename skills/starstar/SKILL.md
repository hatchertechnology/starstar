---
name: starstar
description: Use when the user wants to 3D print something, send a sliced file (G-code, .bgcode) to a printer or over PrusaLink, or check printer status, the current print job, file transfers, storage or cameras on Prusa MK4 or MINI printers managed by starstar. Also use for the starstar CLI or MCP server, and for signing in to starstar.
---

# starstar: control Prusa printers

starstar is a server plus a CLI (`starstar`) that drives Prusa MK4 and MINI printers through PrusaLink. This skill is how an agent uses the CLI (or the MCP server) to upload G-code, start prints and watch them. Nothing here talks to a printer directly; every call goes through the starstar API.

## Safety rules

Printers heat and move. A wrong command wastes filament or damages hardware.

- **Printer busy, or no one to ask:** if the printer is not idle, or you cannot get the user's go-ahead (no terminal, unattended, or they only said "just print it" without naming a printer), stop. Do not upload with `--print`, do not stop or pause the running job to make room, and do not add `--yes`. "Just print it" authorises starting a print only on an idle printer you have confirmed; it does not authorise interrupting another job. Report the state and ask.
- Before uploading or printing, run `starstar printer status <name> --json` and `starstar job show <name> --json`. Only continue if the printer is idle (not printing, paused, or waiting for the user) and the right printer name is confirmed.
- Start a print only when the user has said to print it, on that printer. Uploading without `--print` is the safe default; ask before `files print` or `--print`. If you cannot ask, upload without `--print` (when the printer is idle) and tell the user it is waiting.
- Never pass `--yes` to `job stop`, `files rm`, `printer remove`, `user remove` or `update apply` unless the user told you to do that exact action. Without a terminal these commands fail with exit code 5 until `--yes` is given; that failure is the safety net, so do not "fix" it by adding the flag.
- Never use `files rm --force` (it deletes a non-empty folder and everything in it) without explicit instruction.
- Agents sign in as **operator**, never admin. `login` defaults to `--role operator`; leave it. Admin is for registering printers, deleting files, camera settings, firmware and users, which the user does themselves.
- Slice for the right printer: an MK4 file will not run on an MK4S (it checks the model and nozzle), and the MINI needs its own file. starstar registers printers only as `mk4` or `mini`, so run `starstar printer info <name>` to see the real model before choosing the MK4 or MK4S file.
- Do not guess printer names. Run `starstar printer list`.

## Install the CLI

```sh
cargo install --git https://github.com/hatchertechnology/starstar starstar-cli --locked
# or, from a checkout:
cargo install --path crates/cli --locked
```

The binary is `starstar`. Point it at the server with `--api-url` or `STARSTAR_API_URL` (default `http://127.0.0.1:8080`). `starstar status` checks the API is reachable.

## Sign in

```sh
starstar login               # opens a browser, signs in as operator
starstar login --no-browser  # prints a URL; paste back the code it shows
starstar whoami
```

Use `--no-browser` on a headless machine or when you cannot open a browser; ask the user to open the URL and paste the code. For unattended use, set `STARSTAR_TOKEN` to a bearer token; it is used as-is and never refreshed. `starstar logout` revokes the sign-in. On a fresh server, the admin runs `starstar setup --username <name>` with the setup code the API printed.

## Print loop: model → slice → upload

1. Model with the `cad-skills:cadquery` skill (or have the user supply STL/STEP).
2. Slice with `cad-skills:prusaslicer` to binary G-code (`.bgcode`) for the target printer (MK4, MK4S or MINI), with the right filament profile. Use that skill only to slice: upload and print with `starstar files upload`, never its PrusaLink/curl step, which bypasses starstar's roles and audit.
3. Check the printer is idle (safety rules above).
4. Upload:

```sh
starstar files upload mk4 usb/ prints/cube-12mm/cube-12mm.MK4.bgcode
```

5. Show the user the file, estimate and printer, get a go-ahead, then print it:

```sh
starstar files print mk4 usb/cube-12mm.MK4.bgcode
# or upload and start in one step:
starstar files upload mk4 usb/ cube.bgcode --print
```

`--overwrite` replaces a file of the same name. `<remote>` is `<storage>/<path>`; list storages with `starstar printer storage <name>`.

**First test print:** `prints/cube-12mm/` in a starstar checkout (https://github.com/hatchertechnology/starstar/tree/main/prints/cube-12mm) has a 12 mm cube sliced for the MK4 (`cube-12mm.MK4.bgcode`), MK4S and MINI (`cube-12mm.MINI.bgcode`) with Prusament PLA at 0.20 mm: about 9 minutes on an MK4. Use the file matching the printer.

## Monitor

```sh
starstar printer status mk4 --json   # state and temperatures
starstar job show mk4 --json         # progress, time left, file
starstar camera snap mk4 -o /tmp/mk4.jpg   # latest camera picture
starstar transfer show mk4           # upload in progress
```

Poll `job show --json` every minute or so, not in a tight loop. `job pause`, `job resume`, `job continue` (after a filament change) and `transfer cancel` (aborts an upload) are operator actions; use them only when the user asks. `job stop` cancels the print and needs the user's go-ahead.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | success |
| 1 | API unreachable, user said no, or other API error (including 409 conflicts) |
| 2 | bad arguments |
| 3 | not signed in, or sign-in expired: run `starstar login` |
| 4 | role too low (403): ask the user, do not try to escalate |
| 5 | destructive command needs `--yes` and there is no terminal |

`--json` makes results and errors machine-readable; use it when parsing.

## Use MCP instead

If the agent supports MCP, connect the server rather than the CLI:

```sh
claude mcp add --transport http starstar <public_url>/mcp
```

The OAuth sign-in happens in the browser; tools are listed per role. See [references/mcp.md](references/mcp.md). The same safety rules apply: `start_print`, `stop_job`, `delete_file`, `remove_printer` and `apply_update` need the user's go-ahead, and `pause_job`, `resume_job`, `cancel_transfer` only when asked.

## Reference

Every command and flag: [references/commands.md](references/commands.md).
