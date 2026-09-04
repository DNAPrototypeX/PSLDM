# PSLDM

[![CI](https://github.com/DNAPrototypeX/PSLDM/actions/workflows/ci.yml/badge.svg)](https://github.com/DNAPrototypeX/PSLDM/actions/workflows/ci.yml)

Paul's Screen Locker and Display Manager: one login pane with two front ends.
The greeter logs you in. The locker unlocks a session that already runs. Both
draw the same pane from the same code, so the two screens match. The look
follows macOS Sonoma.

![The greeter and the locker, before and after the first key](docs/comparison.png)

## Install
Arch Linux:

```sh
sudo pacman -S rust clang gtk4 gtk4-layer-shell greetd accountsservice hyprland
git clone https://github.com/DNAPrototypeX/PSLDM && cd PSLDM
./install.sh --greeter --wallpaper ~/path/to/wallpaper.jpg
```

That builds both programs, puts them in `/usr/local/bin`, and writes five
things that keep the two screens the same:

| File | Content |
| --- | --- |
| `/etc/pam.d/psldm` | The PAM stack for the locker |
| `/etc/psldm/wallpaper` | Your image, readable by the greeter user |
| `/etc/psldm/font` | The font family of your desktop |
| `/etc/psldm/monitors.lua` | The monitor modes of your desktop |
| `/var/lib/AccountsService/icons/<user>` | Your avatar |

The script calls `sudo` only for the steps outside the repository, and it
keeps a copy of every file it replaces.

| Option | What it adds |
| --- | --- |
| `--greeter` | The greetd files in `/etc/greetd`, and the monitor modes |
| `--enable-greetd` | greetd at boot. This turns on `--greeter` |
| `--wallpaper PATH`, `--avatar PATH`, `--font NAME` | Your own choices |
| `--prefix DIR`, `--destdir DIR` | Another place, or a package build |
| `--dry-run`, `--uninstall` | The steps only, or remove everything |
The greeter adds the power buttons, the user picker, and the session list.
Nothing else differs.

## Install

Install the dependencies, then run the script:

```sh
sudo systemctl disable sddm.service      # or your display manager
sudo systemctl enable greetd.service
```

Keep the old display manager installed. If the greeter fails at boot, press
Ctrl+Alt+F2 for a text login and turn it back on.

## Make it your screen locker

Bind a key, and point your idle daemon at the same command:

```lua
hl.bind("SUPER + L", hl.dsp.exec_cmd("pgrep -x psldm-lock || psldm-lock"))
```

`packaging/hypr/psldm-lock.lua` holds that line for Hyprland. An idle daemon
needs the full path, because it often runs with a short PATH.
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

The screen has two phases. Before the first key it shows the picker only.
The first key slides the password field in under the name over 260
milliseconds, and the picker rises as the field grows. That key also starts
the password, so no character is lost.

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

The test needs a Wayland or an X11 display, and it opens two windows for a
moment. Without a display it reports the reason and stops. Set
`PSLDM_TEST_DUMP` to a directory to save the drawings as PPM files.

## Requirements

| Package | Needed for | Without it |
| --- | --- | --- |
| `gtk4`, `gtk4-layer-shell` 1.1 or later | Both programs. The session-lock API is part of gtk4-layer-shell | Nothing runs |
| `greetd` | The greeter only | The locker still works |
| `accountsservice` | The user list and the avatars | The greeter shows the user who runs it |
| A compositor for the greeter | `hyprland` 0.56 or later, `sway`, or `cage` | The greeter has no screen |
| `python3` | `install.sh`, to copy the monitor modes | The greeter uses the preferred mode |
| `clang` | The build only. The `pam-sys` crate runs `bindgen`, which loads `libclang` | The build stops at `clang-sys` |

The compositor of your session must support `ext-session-lock-v1`. Hyprland,
Sway, KDE, and River do.

The two Hyprland files use the Lua format, which Hyprland 0.56 reads. Hyprland
still reads the older `.conf` format, and it plans to drop it.

## Other desktops

PSLDM has no code for one distribution or one desktop. Three files hold every
choice, and each one is easy to change:

- `packaging/greetd/greeter-session` names the compositor for the greeter.
  Change one line for `cage` or `sway`.
- `packaging/greetd/hyprland.lua` configures that compositor. Another
  compositor needs its own file.
- `install.sh --greeter` writes `/etc/psldm/monitors.lua` with `hyprctl`.
  Another compositor keeps the preferred mode of each monitor.

The wallpaper, the font, the avatar, and the session memory work the same
everywhere.

## License

GPL-3.0-or-later. PSLDM contains code from ReGreet. See `ATTRIBUTION.md`.
