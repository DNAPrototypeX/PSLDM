# PSLDM

[![CI](https://github.com/DNAPrototypeX/PSLDM/actions/workflows/ci.yml/badge.svg)](https://github.com/DNAPrototypeX/PSLDM/actions/workflows/ci.yml)

Paul's Screen Locker and Display Manager: one login pane with two front ends.
The greeter logs you in. The locker unlocks a session that already runs. Both
draw the same pane from the same code, so the two screens match.

![The greeter on the left, the locker on the right](docs/comparison.png)

The greeter adds the power buttons, the user picker, and the session list.
Nothing else differs.

## Install

Install the dependencies, then run the script:

```sh
sudo pacman -S --needed rust clang gtk4 gtk4-layer-shell pam accountsservice greetd
git clone https://github.com/DNAPrototypeX/PSLDM.git
cd PSLDM
./install.sh --greeter --wallpaper ~/Pictures/wallpaper.png
```

The script builds the programs, copies them to `/usr/local/bin`, and writes
the PAM file, the wallpaper, your avatar, your font, and the greetd files. It
keeps a copy of every file that it replaces, with the suffix `.psldm-backup`.
Run `./install.sh --help` for the options, and `./install.sh --uninstall` to
remove everything again.

## Lock the screen

1. Test the locker: `psldm-lock --preview`.
2. Add `source = /path/to/PSLDM/packaging/hypr/psldm-lock.conf` to
   `~/.config/hypr/hyprland.conf`.
3. Press SUPER + L.

That file also holds `misc:allow_session_lock_restore`. Hyprland needs this
setting to give the lock back to a new locker after a crash.

## Log in with the greeter

1. Test the greeter: `psldm-greet --preview ~/Pictures/wallpaper.png`.
2. Open a second text console, and log in there.
3. Run `sudo systemctl enable greetd.service`.
4. Restart the computer.

WARNING: greetd replaces your login screen. Keep the second console open
until the greeter works.

## The screen

The screen has two phases, as macOS has.

- The clock alone, over a sharp wallpaper with a dark layer on it.
- The clock at the top, with the avatar, the name, and the password field.

The first key opens the second phase, and the field does not receive that
key. The screen returns to the first phase after 30 seconds without a key,
but only while the field is empty.

## Crates

| Crate | Purpose |
| --- | --- |
| `psldm-ui` | The shared login pane, its state machine, and the stylesheet |
| `psldm-auth` | The greetd backend and the PAM backend, behind one channel pair |
| `psldm-session` | Users, avatars, and sessions, from AccountsService and desktop files |
| `psldm-greet` | The greeter for greetd |
| `psldm-lock` | The locker |

The two programs choose a surface and a backend. The greeter draws on a layer
shell surface and speaks to greetd. The locker draws on an
`ext-session-lock-v1` surface and speaks to PAM.

## Requirements

- A Wayland compositor with the `ext-session-lock-v1` protocol.
- `gtk4` 4.12 or later, and `gtk4-layer-shell` 1.3 or later.
- `accountsservice`, for the user list and the avatars.
- `greetd`, for the greeter only.

## Develop

```sh
cargo build --release
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The workspace holds 9 tests for the state machine and one pixel test. The
pixel test draws both modes and compares every pixel, so a change that only
one mode shows makes it fail. That test needs a display. Without one it
prints the reason and stops, and `PSLDM_REQUIRE_DISPLAY=1` makes the same
condition a failure.

GitHub Actions runs the same three commands in an Arch Linux container, with
Xvfb for the display. See `.github/workflows/ci.yml`.

## License

GPL-3.0-or-later. PSLDM contains code from ReGreet. See `ATTRIBUTION.md`.
