# starstar command reference

Generated from `starstar --help` and each subcommand's `--help`. Every command also takes `--api-url <URL>` (env `STARSTAR_API_URL`) and `--json`.

```

=== status
Check that the API is reachable
Usage: starstar status [OPTIONS]

=== login
Sign in with your browser. On a machine without a browser, use --no-browser
Usage: starstar login [OPTIONS]
      --no-browser         Print a URL and ask for the code it shows, instead of opening a browser. Use this on a machine without a browser
      --role <ROLE>        Highest role this sign-in may use: viewer, operator or admin [default: operator]

=== logout
Sign out and revoke this sign-in on the server
Usage: starstar logout [OPTIONS]

=== whoami
Show who you are signed in as
Usage: starstar whoami [OPTIONS]

=== setup
Create the first admin account with the setup code the API printed
Usage: starstar setup [OPTIONS] --username <USERNAME>
      --username <USERNAME>  Username for the new admin
      --setup-code-stdin     Read the setup code from stdin (one line) instead of prompting
      --password-stdin       Read the password from stdin (one line, after the setup code) instead of prompting

=== job
Show and control the running print job
Usage: starstar job [OPTIONS] <COMMAND>
Commands:
  show      Show the current job, if any
  pause     Pause the current job
  resume    Resume a paused job
  continue  Continue a job that is waiting for the user (for example after a filament change)
  stop      Stop (cancel) the current job. Needs --yes without a terminal

=== job show
Show the current job, if any
Usage: starstar job show [OPTIONS] <PRINTER>
Arguments:
  <PRINTER>  Printer name

=== job pause
Pause the current job
Usage: starstar job pause [OPTIONS] <PRINTER>
Arguments:
  <PRINTER>  Printer name
      --id <ID>            Job id. Without it, the current job is looked up first

=== job resume
Resume a paused job
Usage: starstar job resume [OPTIONS] <PRINTER>
Arguments:
  <PRINTER>  Printer name
      --id <ID>            Job id. Without it, the current job is looked up first

=== job continue
Continue a job that is waiting for the user (for example after a filament change)
Usage: starstar job continue [OPTIONS] <PRINTER>
Arguments:
  <PRINTER>  Printer name
      --id <ID>            Job id. Without it, the current job is looked up first

=== job stop
Stop (cancel) the current job. Needs --yes without a terminal
Usage: starstar job stop [OPTIONS] <PRINTER>
Arguments:
  <PRINTER>  Printer name
      --id <ID>            Job id. Without it, the current job is looked up first
  -y, --yes                Do not ask for confirmation. Required when there is no terminal

=== transfer
Show and cancel a file transfer to the printer
Usage: starstar transfer [OPTIONS] <COMMAND>
Commands:
  show    Show the file transfer in progress, if any
  cancel  Cancel the file transfer in progress

=== transfer show
Show the file transfer in progress, if any
Usage: starstar transfer show [OPTIONS] <PRINTER>
Arguments:
  <PRINTER>  Printer name

=== transfer cancel
Cancel the file transfer in progress
Usage: starstar transfer cancel [OPTIONS] <PRINTER>
Arguments:
  <PRINTER>  Printer name
      --id <ID>            Transfer id. Without it, the current transfer is looked up first

=== part
Manage parts and their files
Usage: starstar part [OPTIONS] <COMMAND>
Commands: ls, new, info, rm, rev, setup, upload, download
Identify <PART> by name or ID. Use name:X or id:N to disambiguate a numeric name that differs from the matching ID.

=== part ls
Usage: starstar part ls [OPTIONS]
      --search <SEARCH>
      --sort <SORT>        Possible values: newest, name

=== part new
Usage: starstar part new [OPTIONS] <NAME>
      --notes <NOTES>

=== part info
Usage: starstar part info [OPTIONS] <PART>
      --all                Include replaced slices

=== part rm
Usage: starstar part rm [OPTIONS] <PART>
  -y, --yes                Do not ask for confirmation; required without a terminal

=== part rev
Usage: starstar part rev [OPTIONS] <PART>
      --note <NOTE>
      --carry <CARRY>      May be supplied more than once

=== part setup
Usage: starstar part setup [OPTIONS] <PART> <NAME>

=== part setup-rev
Usage: starstar part setup-rev [OPTIONS] <PART> <SETUP>
      --model-rev <N>  Model revision number (defaults to latest)
      --note <TEXT>

=== part edit
Usage: starstar part edit [OPTIONS] <PART>
      --name <NAME>
      --notes <NOTES>

=== part label
Usage: starstar part label <FILE_ID> <PROFILE>
An empty profile clears the label. Only slices have profiles.

=== part upload
Usage: starstar part upload [OPTIONS] <PART> <LOCAL>
      --role <ROLE>        Possible values: source, render, project_3mf, slice, other
      --setup <SETUP>
      --setup-rev <SETUP_REV>
      --model-rev <MODEL_REV>
      --profile <PROFILE>
      --replace
      --name <NAME>
When --role is omitted, the role is inferred from the file name extension (using --name if supplied); an unrecognized extension needs --role. Slices and .3mf projects need --setup; --setup-rev requires --setup. Other files without --model-rev stay at part level. Setup names match case-insensitively. The result includes the server file and whether it was created or already existed. Profiles on non-slice uploads are ignored.

CLI deletion of individual files, setups and revisions is not available; use the web UI or API. The API rejects deletion of a part’s last model revision. Part and setup names are trimmed and cannot be empty.

=== part download
Usage: starstar part download [OPTIONS] <ID>
  -o, --output <OUTPUT>
      --force

=== files
List, inspect, upload, print and delete files on a printer
Usage: starstar files [OPTIONS] <COMMAND>
Commands:
  ls      List a folder: `<storage>` or `<storage>/<path>`
  info    Show a file's details and whether it is read-only or printing
  upload  Upload a local file to the printer
  print   Start printing a file on the printer
  rm      Delete a file or folder. Needs --yes without a terminal

=== files ls
List a folder: `<storage>` or `<storage>/<path>`
Usage: starstar files ls [OPTIONS] <PRINTER> <REMOTE>
Arguments:
  <PRINTER>  Printer name
  <REMOTE>   Storage and folder, e.g. `usb` or `usb/parts`

=== files info
Show a file's details and whether it is read-only or printing
Usage: starstar files info [OPTIONS] <PRINTER> <REMOTE>
Arguments:
  <PRINTER>  Printer name
  <REMOTE>   File, e.g. `usb/cube.bgcode`

=== files upload
Upload a local file to the printer
Usage: starstar files upload [OPTIONS] <PRINTER> <REMOTE> <LOCAL>
Arguments:
  <PRINTER>  Printer name
  <REMOTE>   Destination: a folder (`usb` or `usb/parts/`) or a file path (`usb/cube.bgcode`)
  <LOCAL>    Local file to upload
      --overwrite          Replace a file of the same name
      --print              Start printing as soon as the upload finishes

=== files print
Start printing a file on the printer
Usage: starstar files print [OPTIONS] <PRINTER> <REMOTE>
Arguments:
  <PRINTER>  Printer name
  <REMOTE>   File, e.g. `usb/cube.bgcode`

=== files rm
Delete a file or folder. Needs --yes without a terminal
Usage: starstar files rm [OPTIONS] <PRINTER> <REMOTE>
Arguments:
  <PRINTER>  Printer name
  <REMOTE>   File or folder, e.g. `usb/cube.bgcode`
      --force              Delete a folder even if it is not empty
  -y, --yes                Do not ask for confirmation. Required when there is no terminal

=== camera
List, show, configure and take pictures with the printer's cameras
Usage: starstar camera [OPTIONS] <COMMAND>
Commands:
  ls          List the printer's cameras
  show        Show one camera's configuration
  snap        Save a camera picture to a file (or `-` for stdout)
  order       Set the order of the cameras
  register    Register a camera with the printer, optionally with settings
  config      Change a camera's settings (give at least one)
  remove      Remove a camera from the printer
  reset       Reset a camera's settings to the defaults
  connect     Connect a camera
  disconnect  Disconnect a camera

=== camera ls
List the printer's cameras
Usage: starstar camera ls [OPTIONS] <PRINTER>
Arguments:
  <PRINTER>  Printer name

=== camera show
Show one camera's configuration
Usage: starstar camera show [OPTIONS] <PRINTER> <ID>
Arguments:
  <PRINTER>  Printer name
  <ID>       Camera id

=== camera snap
Save a camera picture to a file (or `-` for stdout)
Usage: starstar camera snap [OPTIONS] --output <OUTPUT> <PRINTER> [ID]
Arguments:
  <PRINTER>  Printer name
  [ID]       Camera id. Without it, the printer's default camera is used
  -o, --output <OUTPUT>    Where to save the picture; `-` writes it to stdout
      --take               Take a new picture now instead of fetching the last one (needs a camera id)

=== camera order
Set the order of the cameras
Usage: starstar camera order [OPTIONS] <PRINTER> <IDS>...
Arguments:
  <PRINTER>  Printer name
  <IDS>...   Camera ids in the new order

=== camera register
Register a camera with the printer, optionally with settings
Usage: starstar camera register [OPTIONS] <PRINTER> <ID>
Arguments:
  <PRINTER>  Printer name
  <ID>       Camera id
      --name <NAME>                          Camera name
      --trigger <TRIGGER>                    When to take pictures [possible values: ten-sec, thirty-sec, sixty-sec, each-layer, fifth-layer, manual]
      --resolution <RESOLUTION>              Resolution as WIDTHxHEIGHT
      --rotation <ROTATION>                  Rotation in degrees
      --focus <FOCUS>                        Focus
      --exposure <EXPOSURE>                  Exposure
      --send-to-connect <SEND_TO_CONNECT>    Send pictures to Prusa Connect: true or false [possible values: true, false]

=== camera config
Change a camera's settings (give at least one)
Usage: starstar camera config [OPTIONS] <PRINTER> <ID>
Arguments:
  <PRINTER>  Printer name
  <ID>       Camera id
      --name <NAME>                          Camera name
      --trigger <TRIGGER>                    When to take pictures [possible values: ten-sec, thirty-sec, sixty-sec, each-layer, fifth-layer, manual]
      --resolution <RESOLUTION>              Resolution as WIDTHxHEIGHT
      --rotation <ROTATION>                  Rotation in degrees
      --focus <FOCUS>                        Focus
      --exposure <EXPOSURE>                  Exposure
      --send-to-connect <SEND_TO_CONNECT>    Send pictures to Prusa Connect: true or false [possible values: true, false]

=== camera remove
Remove a camera from the printer
Usage: starstar camera remove [OPTIONS] <PRINTER> <ID>
Arguments:
  <PRINTER>  Printer name
  <ID>       Camera id

=== camera reset
Reset a camera's settings to the defaults
Usage: starstar camera reset [OPTIONS] <PRINTER> <ID>
Arguments:
  <PRINTER>  Printer name
  <ID>       Camera id

=== camera connect
Connect a camera
Usage: starstar camera connect [OPTIONS] <PRINTER> <ID>
Arguments:
  <PRINTER>  Printer name
  <ID>       Camera id

=== camera disconnect
Disconnect a camera
Usage: starstar camera disconnect [OPTIONS] <PRINTER> <ID>
Arguments:
  <PRINTER>  Printer name
  <ID>       Camera id

=== update
Check for and apply printer firmware updates
Usage: starstar update [OPTIONS] <COMMAND>
Commands:
  check  Check whether an update is available
  apply  Apply an available update. Always needs --yes, even in a terminal

=== update check
Check whether an update is available
Usage: starstar update check [OPTIONS] <PRINTER>
Arguments:
  <PRINTER>  Printer name
      --env <ENV>          What to update. Only `prusalink` is supported [default: prusalink] [possible values: prusalink]

=== update apply
Apply an available update. Always needs --yes, even in a terminal
Usage: starstar update apply [OPTIONS] <PRINTER>
Arguments:
  <PRINTER>  Printer name
      --env <ENV>          What to update. Only `prusalink` is supported [default: prusalink] [possible values: prusalink]
  -y, --yes                Confirm the update. Required

=== user
Manage users (admin) and your own password
Usage: starstar user [OPTIONS] <COMMAND>
Commands:
  add       Create a user
  list      List users
  set-role  Change a user's role
  disable   Disable a user so they cannot sign in
  enable    Enable a disabled user
  remove    Delete a user. Needs --yes without a terminal
  passwd    Change your own password. This signs out all your other sessions and apps

=== user add
Create a user
Usage: starstar user add [OPTIONS] --role <ROLE> <USERNAME>
Arguments:
  <USERNAME>  New username: lowercase letters, digits, '.', '_' or '-'
      --role <ROLE>        Role: viewer, operator or admin
      --password-stdin     Read the password from stdin (one line) instead of prompting

=== user list
List users
Usage: starstar user list [OPTIONS]

=== user set-role
Change a user's role
Usage: starstar user set-role [OPTIONS] <USERNAME> <ROLE>
Arguments:
  <USERNAME>  Username
  <ROLE>      New role: viewer, operator or admin

=== user disable
Disable a user so they cannot sign in
Usage: starstar user disable [OPTIONS] <USERNAME>
Arguments:
  <USERNAME>  Username

=== user enable
Enable a disabled user
Usage: starstar user enable [OPTIONS] <USERNAME>
Arguments:
  <USERNAME>  Username

=== user remove
Delete a user. Needs --yes without a terminal
Usage: starstar user remove [OPTIONS] <USERNAME>
Arguments:
  <USERNAME>  Username
  -y, --yes                Do not ask for confirmation. Required when there is no terminal

=== user passwd
Change your own password. This signs out all your other sessions and apps
Usage: starstar user passwd [OPTIONS]
      --password-stdin     Read the current and then the new password from stdin (two lines)

=== apps
List and revoke connected apps (OAuth sign-ins)
Usage: starstar apps [OPTIONS] <COMMAND>
Commands:
  list    List connected apps
  revoke  Revoke a connected app's access

=== apps list
List connected apps
Usage: starstar apps list [OPTIONS]
      --all                Show everyone's apps (admin only)

=== apps revoke
Revoke a connected app's access
Usage: starstar apps revoke [OPTIONS] <ID>
Arguments:
  <ID>  App id, from `apps list`

=== printer
Register, list, inspect and remove printers
Usage: starstar printer [OPTIONS] <COMMAND>
Commands:
  add      Register a printer. The API checks it can reach the printer first
  remove   Remove a printer from starstar. The printer itself is not changed. Needs --yes without a terminal
  list     List registered printers
  show     Show a registered printer's connection details
  info     Show firmware and hardware details from the printer
  status   Show the printer's state and temperatures
  storage  List storage devices on the printer

=== printer add
Register a printer. The API checks it can reach the printer first
Usage: starstar printer add [OPTIONS] --kind <KIND> --host <HOST> --username <USERNAME> <NAME>
Arguments:
  <NAME>  Unique name for the printer, e.g. `office-mk4`
      --kind <KIND>          Printer model: `mk4` or `mini`
      --host <HOST>          Hostname or IP address, optionally with a port
      --username <USERNAME>  PrusaLink username
      --password-stdin       Read the password from stdin instead of prompting for it

=== printer remove
Remove a printer from starstar. The printer itself is not changed. Needs --yes without a terminal
Usage: starstar printer remove [OPTIONS] <NAME>
Arguments:
  <NAME>  Printer name
  -y, --yes                Do not ask for confirmation. Required when there is no terminal

=== printer list
List registered printers
Usage: starstar printer list [OPTIONS]

=== printer show
Show a registered printer's connection details
Usage: starstar printer show [OPTIONS] <NAME>
Arguments:
  <NAME>  Printer name

=== printer info
Show firmware and hardware details from the printer
Usage: starstar printer info [OPTIONS] <NAME>
Arguments:
  <NAME>  Printer name

=== printer status
Show the printer's state and temperatures
Usage: starstar printer status [OPTIONS] <NAME>
Arguments:
  <NAME>  Printer name

=== printer storage
List storage devices on the printer
Usage: starstar printer storage [OPTIONS] <NAME>
Arguments:
  <NAME>  Printer name
```

Exit codes: 0 success; 1 API, I/O, cancellation, or other failure (including HTTP 409); 2 bad arguments or client-side validation; 3 unauthenticated or HTTP 401; 4 HTTP 403; 5 confirmation required without a terminal. `--json` also makes errors machine-readable.
