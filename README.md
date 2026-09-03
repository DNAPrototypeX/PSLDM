# PSLDM

[![CI](https://github.com/DNAPrototypeX/PSLDM/actions/workflows/ci.yml/badge.svg)](https://github.com/DNAPrototypeX/PSLDM/actions/workflows/ci.yml)

Paul's Screen Locker and Display Manager: one login pane with two front ends.
The greeter logs you in. The locker unlocks a session that already runs. Both
draw the same pane from the same code, so the two screens match. The look
follows macOS Sonoma.

![The greeter and the locker, before and after the first key](docs/comparison.png)

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
2. Bind `pgrep -x psldm-lock || psldm-lock` to a key in your compositor. The
   guard keeps one locker on the screen at a time.
3. Give the same command to your idle daemon.

`packaging/hypr/psldm-lock.conf` holds both lines for Hyprland. It also sets
`misc:allow_session_lock_restore`, which lets a new locker take the lock back
after a crash.

## Log in with the greeter

1. Test the greeter: `psldm-greet --preview ~/Pictures/wallpaper.png`.
2. Open a second text console, and log in there.
3. Run `sudo systemctl enable greetd.service`.
4. Restart the computer.

WARNING: greetd replaces your login screen. Keep the second console open
until the greeter works.

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
one mode shows makes it fail.

GitHub Actions runs the same three commands in an Arch Linux container. See
`.github/workflows/ci.yml`.

This command draws the picture at the top of this page:

```sh
cargo run -p psldm-ui --example comparison -- docs/comparison.png
```

## License

GPL-3.0-or-later. PSLDM contains code from ReGreet. See `ATTRIBUTION.md`.
