# Folder commands

Folder commands let you attach reusable commands to a folder (or a folder
tree) and run them from Delve's context menu, Quick Spotlight, or a command
set picker. Delve never runs a command set on navigation, on file selection,
or as an unexplained background action.

## Where folder commands appear

- **Folder context menu** — right-click blank space in the file area, or
  right-click selected items. Matching command sets are listed under **Run
  command sets**, together with any assigned quick shortcut.
- **Run multiple sets…** — opens the command-set picker from the context menu,
  the **More actions** menu, Quick Spotlight, or with `Shift+S, 0`.
- **Quick Spotlight** (`Super+Space`, or `Ctrl+Space` fallback) — search for a
  command set by name, description, program, folder, scope, environment key,
  or shortcut.
- **Settings → Commands** — create, edit, enable/disable, run, delete, and
  assign shortcuts to command sets.
- **Run panel or detached run window** — background executions stream output
  and show status, exit codes, stop, restart, and clear controls.

A single run from the context menu or Spotlight opens a confirmation dialog
listing everything that will run. A quick shortcut, or **Run selected** in the
picker, is already an explicit launch and starts immediately.

## Create a command set

Open a folder, right-click blank space, and choose **New command set…**, or open
**Settings → Commands → Add command set…**.

A command set (called a profile internally) contains:

| Field | Meaning |
| --- | --- |
| **Name** | Menu and Spotlight label. Required. |
| **Description** | Optional searchable explanation. |
| **Folder** | Base folder for the set. |
| **Availability** | Where the set is offered (see below). |
| **Working directory** | Current folder, Home, or the first selected folder. |
| **Execution mode** | Sequential, Parallel, or Mixed groups, inside this set. |
| **Launch in** | Background run panel or a separate terminal window. |
| **Stop on failure** | Skip later steps/groups after a non-zero exit. |
| **Enabled** | Hides the set from menus and Spotlight when off. |
| **Environment** | Optional `KEY=value` lines applied to every step. |
| **Quick shortcut** | Optional `Shift+S` slot from 1 to 9. |
| **Commands** | One or more direct program invocations. |

### Availability scopes

- **This folder only** — offered when the current folder is exactly the
  profile's folder.
- **This folder and subfolders** — offered at the base folder and anywhere
  below it. This is the default and matches the original behaviour.
- **Every folder** — offered everywhere. The Folder field is disabled and
  ignored.

A profile with no folder is always treated as **Every folder**. Folder-scoped
profiles require a folder.

## Command sets and multiple selection

Each command set can contain several commands. To run more than one set at
once, use **Run multiple sets…** and check the sets you want. The picker only
lists enabled sets whose availability matches the current folder.

**Run selected** starts every checked set together. Each set still follows its
own execution mode, so a sequential set stays ordered internally while it runs
beside the other selected sets. Batch runs open the detached run window so the
progress of all sets is visible together.

## Quick shortcuts

Assign **Quick shortcut** slots 1–9 to command sets. From the main file window,
press `Shift+S`, then the slot number:

```
Shift+S, 1   run the set assigned to slot 1
Shift+S, 2   run the set assigned to slot 2
Shift+S, 0   open the command-set picker
```

Slots are unique. A slot starts its set immediately, without the single-run
confirmation dialog. The shortcut is ignored while a text field has focus, so
capital S still types normally in search, path, rename, and editor fields.

If the assigned set is disabled or does not apply to the current folder, Delve
shows a notification instead of running it.

## Commands and steps

A command set can run multiple commands. Each row has:

- **Program** — an executable found on `PATH` or an absolute path.
- **Arguments** — split into separate argv entries using shell-like quoting,
  but **no shell is involved**.
- **Group** — used only by Mixed groups.

Delve uses `std::process::Command` with direct argv. Shell syntax such as
pipes, redirection, globbing, and `$VARIABLES` is not interpreted. When you
need shell features, invoke a shell explicitly:

```
Program: sh
Arguments: -c "printf 'hello\n' | wc -c"

Program: bash
Arguments: -lc "source .venv/bin/activate && pytest"
```

## Execution modes

### Sequential

Each command starts after the previous command exits.

```
1. cargo fmt --check
2. cargo test
3. cargo build --release
```

If **Stop on failure** is on, a non-zero exit marks later commands **Skipped**.

### Parallel

All commands start together and run concurrently. Each step has its own run
panel entry; the profile finishes when all children exit.

```
1. cargo check
2. cargo clippy
3. cargo test
```

### Mixed groups

Commands inside the same group run in parallel. Groups run in ascending order.

```
group 1: cargo fmt --check
group 1: cargo clippy
group 2: cargo test
group 2: cargo build
group 3: ./package.sh
```

If a command in a group fails and **Stop on failure** is on, later groups are
skipped.

## Environment variables

Use **Environment** for non-interactive runs. Enter one `KEY=value` per line.
Blank lines and lines starting with `#` are ignored.

```env
RUST_BACKTRACE=1
PROJECT_ENV=development
```

Rules:

- Variable names must not contain whitespace or `=`.
- Values are passed literally; Delve does not expand `$HOME` or other
  variables inside them.
- The values are added to the inherited environment.
- Environment variables are passed to background steps and to the terminal
  process in **Separate terminal window** mode.

## Placeholders

Placeholders are expanded inside arguments only. They are not expanded inside
the program name.

| Placeholder | Expands to |
| --- | --- |
| `{folder}`, `{dir}` | The folder the profile is running against. |
| `{home}` | The current user's home directory. |
| `{name}` | The folder's basename. |
| `{selected}`, `{files}` | One argument per selected item. |

Examples:

```
Program: git
Arguments: -C {folder} status --short

Program: code
Arguments: {folder}

Program: tar
Arguments: -czf {name}.tar.gz {selected}
```

`{selected}`/`{files}` must be a complete argument (not embedded inside other
text), and can expand to zero arguments if nothing is selected. Because
arguments are not shell-split after expansion, spaces in paths are safe.

## Background launch, run panel, and detached window

Background sets appear in the run panel:

- Live combined stdout/stderr output.
- Running/done/failed/stopped status.
- Exit code (for example, **Failed (exit 7)**).
- **Stop** terminates the process and its process group.
- **Restart** starts the same set again.
- **Clear finished** removes completed entries from the panel.
- **Open command runs in a window** moves the panel into a detached output
  window without interrupting running processes.

The window button in the run header toggles between the detached window and
the docked panel. Closing the detached window moves the same runner back into
the main window. Batch runs from the picker open the detached window
automatically.

Delve starts background children in their own process group, so Stop can also
terminate helpers they spawn. Closing the app stops running background
children.

## Separate terminal window

Choose **Launch in → Separate terminal window** for interactive commands,
REPLs, or long-running development servers. Delve detects common terminals and
passes the working directory plus the program/arguments directly.

Important limitation: an external terminal cannot report completion to Delve.
Subsequent steps are launched in order with a short pause, but Delve cannot
wait for the terminal command to finish. The run panel records **Terminal
opened**, not the terminal's exit status. Use background mode when sequencing
depends on completion or exit codes.

If no terminal is detected, choose or write a terminal command in
**Settings → Terminal**, then retry.

## Safety model

- No command set is executed implicitly by navigation or selection.
- Single runs from the context menu and Spotlight show a confirmation dialog.
- Picker launches require the explicit **Run selected** action.
- Quick shortcuts are deliberate launches and start their assigned set
  immediately; slots are unique and ignore text-entry focus.
- Commands are argv arrays, not concatenated shell strings.
- Programs are resolved on `PATH` or used as absolute paths.
- Background processes run in process groups for cleanup.
- A command can be disabled without deleting it.
- Environment values are not shell-expanded.

Delve cannot protect you from a command you deliberately approve. Review
commands before choosing **Run**, especially profiles that delete, publish,
deploy, or modify files outside the folder.

## Troubleshooting

**The command set is not in the context menu**

Check that it is enabled, and confirm its Availability scope and Folder. A
**This folder only** set does not appear in child folders. Right-clicking blank
space and right-clicking a selected item both offer the matching sets.

**Shift+S, 1 does nothing**

Assign slot 1 in the set's editor. Slots are unique, and the shortcut is
ignored while a text field is focused. `Shift+S, 0` always opens the picker.

**“Program was not found on this system”**

Use an absolute path or install the executable and ensure it is on `PATH` for
the session that launched Delve.

**Arguments with spaces or quotes**

The argument field uses shell-style tokenization for convenience, but the
result is executed without a shell. Quote arguments that contain spaces.
When a whole pipeline is needed, explicitly run `sh -c` or `bash -c`.

**The terminal opens but the command is already gone**

The terminal owns the command's lifetime. Use background mode to keep the
process attached to Delve's run panel and to receive an exit code.

**A later step was skipped**

A previous command exited non-zero and **Stop on failure** is enabled. Inspect
the failed entry's output and exit code, then disable that option if later
steps should run regardless.
