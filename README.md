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

1. Run `sudo cp target/release/psldm-lock target/release/psldm-greet /usr/bin/`.
2. Run `sudo cp packaging/pam.d/psldm /etc/pam.d/psldm`.
3. Run `sudo mkdir -p /etc/psldm`, then link your wallpaper to
   `/etc/psldm/wallpaper`.
4. For the locker, source `packaging/hypr/psldm-lock.conf` from your Hyprland
   configuration.
5. For the greeter, install `greetd`, copy both files from
   `packaging/greetd/` to `/etc/greetd/`, then run
   `sudo systemctl enable --now greetd.service`.

Keep the old display manager until the greeter works. A greeter that fails on
virtual terminal 1 leaves you with a text login.

## The screen

The screen has two phases, as macOS has.

- The clock only. The wallpaper is sharp, with a dark layer over it.
- The clock at the top, with the avatar, the name, and the field. The first
  key opens this phase, and the field does not receive that key.

The screen returns to the first phase after 30 seconds without a key, but only
while the field is empty.

## Surfaces

| Program | Surface | Backend |
| --- | --- | --- |
| `psldm-greet` | `wlr-layer-shell` overlay, one for each monitor | greetd |
| `psldm-lock` | `ext-session-lock-v1`, one for each monitor | PAM |

The compositor keeps the lock surface even if `psldm-lock` stops, so a crash
does not open the session.

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
