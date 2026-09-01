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

## Test the current build

The lock surface and the greeter surface arrive in milestone 4. Until then,
both binaries open the pane in a normal window.

1. Run `cargo run -p psldm-lock -- --preview ~/.config/omarchy/current/background`.
2. Press any key. The pane appears, and that first key does not reach the
   field.
3. Type `pass` and press Enter. The preview backend accepts only that word.
4. Run `cargo run -p psldm-greet -- --preview <WALLPAPER>` to see the power
   buttons, the user row, and the session list.
5. Run `cargo run -p psldm-lock -- --check` to test the real PAM stack on the
   terminal. Copy `packaging/pam.d/psldm` to `/etc/pam.d/psldm` first.

## The screen

The screen has two phases, as macOS has.

- The clock only. The wallpaper is sharp, with a dark layer over it.
- The clock at the top, with the avatar, the name, and the field. The first
  key opens this phase, and the field does not receive that key.

The screen returns to the first phase after 30 seconds without a key, but only
while the field is empty.

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
