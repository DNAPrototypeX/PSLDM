# PSLDM

Paul's Screen Locker and Display Manager.

PSLDM is one user interface with two front ends. The greeter logs a user in.
The locker unlocks a session that already runs. Both draw the same login pane,
so the two screens match. The locker hides the power menu and the user picker.

## Crates

| Crate | Purpose |
| --- | --- |
| `psldm-ui` | The shared login pane and its state machine |
| `psldm-auth` | The greetd backend and the PAM backend, behind one channel pair |
| `psldm-session` | Users, avatars, and sessions, from AccountsService and desktop files |
| `psldm-greet` | The greeter binary for greetd |
| `psldm-lock` | The locker binary |

## Build

```sh
cargo build --release
cargo test --workspace
```

## Run it

| Command | What it does |
| --- | --- |
| `psldm-lock` | Locks the session with `ext-session-lock-v1` |
| `psldm-greet` | Shows the greeter. greetd must start it |
| `psldm-lock --preview [WALLPAPER]` | The pane in a normal window, demo password `pass` |
| `psldm-greet --preview [WALLPAPER]` | The same, with the greeter parts |
| `psldm-lock --check [USER]` | The real PAM stack, on the terminal |
| `psldm-greet --users` | The users and the sessions that it finds |

Two more commands test the real surfaces inside a nested compositor:
`psldm-lock --preview-lock` and `psldm-greet --preview-layer`. Both use the
demo backend.

## Install

```sh
./install.sh --wallpaper ~/.config/omarchy/current/background
```

That builds the release programs, puts them in `/usr/local/bin`, installs
`/etc/pam.d/psldm`, and copies the wallpaper to `/etc/psldm/wallpaper`. The
script calls `sudo` only for the steps that write outside the repository, and
it keeps a copy of every file that it replaces.

| Option | What it adds |
| --- | --- |
| `--greeter` | The greetd files in `/etc/greetd` |
| `--enable-greetd` | greetd starts at boot. This turns on `--greeter` |
| `--prefix DIR` | Another place for the programs. Default `/usr/local` |
| `--destdir DIR` | Every file under DIR, for a package build |
| `--dry-run` | The steps only, with no change |
| `--uninstall` | Removes the files and restores the copies |

Keep the old display manager until the greeter works. A greeter that fails on
virtual terminal 1 leaves you with a text login. Test it first with
`psldm-greet --preview`.

## Surfaces

| Program | Surface | Backend |
| --- | --- | --- |
| `psldm-greet` | `wlr-layer-shell` overlay, one for each monitor | greetd |
| `psldm-lock` | `ext-session-lock-v1`, one for each monitor | PAM |

The compositor keeps the lock surface even if `psldm-lock` stops, so a crash
does not open the session.

## Tests

```sh
cargo test --workspace
```

The state machine tests need nothing. One test, `same_pane`, draws both modes
and compares every pixel:

1. It draws the locker pane twice and requires the same pixels, so that the
   comparison means something.
2. It requires the greeter to draw more than the locker.
3. It hides the power buttons, the user row, and the session list, then
   requires the two drawings to match exactly.

A 3 pixel margin that only the greeter uses makes step 3 fail with about 4400
different pixels.

The test needs a Wayland or an X11 display, and it opens two windows for a
moment. Without a display it reports the reason and stops. Set
`PSLDM_TEST_DUMP` to a directory to save the drawings as PPM files.

## Crate layout

`psldm-ui` holds the pane, the state machine, and `assets/style.css`. The two
binaries choose a surface and a backend. Nothing else differs:

- The greeter uses greetd and shows the power buttons, the user row, and the
  session list.
- The locker uses PAM and hides all three.

## Requirements

- A Wayland compositor with the `ext-session-lock-v1` protocol.
- `greetd`, for the greeter.
- `accountsservice`, for the user list and the avatars.

## License

GPL-3.0-or-later. PSLDM contains code from ReGreet. See `ATTRIBUTION.md`.
