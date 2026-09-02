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
| `--greeter` | The greetd files in `/etc/greetd`, and the monitor settings |
| `--avatar PATH` | The picture of the user. Default `~/.face.icon`, then `~/.face` |
| `--font NAME` | The font family. Default: the font of your desktop |
| `--enable-greetd` | greetd starts at boot. This turns on `--greeter` |
| `--prefix DIR` | Another place for the programs. Default `/usr/local` |
| `--destdir DIR` | Every file under DIR, for a package build |
| `--dry-run` | The steps only, with no change |
| `--uninstall` | Removes the files and restores the copies |

Two of those steps keep the greeter and the locker in step:

- The script copies your avatar to `/var/lib/AccountsService/icons/<user>`.
  The greeter runs as the user `greeter`, and it cannot read a home
  directory with `700` permissions.
- The script writes your desktop font family to `/etc/psldm/font`, and both
  programs read that file. Each program otherwise takes the default font of
  the user that runs it, and those differ. The font must live in a system
  directory, because the greeter cannot read a home directory. The script
  warns you when it does not.
- The greeter runs inside `psldm-greeter-session`. That script erases the
  console when the compositor stops. Without it the screen shows the text
  console between the password and the first frame of the session.
- With `--greeter`, the script writes the modes of the running desktop to
  `/etc/psldm/monitors.conf`, and the greeter session reads that file. Without
  it, a laptop panel starts at its preferred mode, which is often larger than
  the mode the desktop uses, and the greeter looks smaller.

The greeter keeps the last user and the last session in
`/var/lib/psldm/state.toml`, and it offers them again at the next login. Pick
the right session once. A desktop with several session files often needs
this: on Omarchy the plain `Hyprland` entry starts no user services, and the
`Omarchy (Hyprland uwsm)` entry starts them all.

Run the script again after you change a monitor or your avatar.

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

On Arch Linux:

```sh
sudo pacman -S rust gtk4 gtk4-layer-shell greetd accountsservice hyprland
```

| Package | Needed for | Without it |
| --- | --- | --- |
| `gtk4`, `gtk4-layer-shell` 1.1 or later | Both programs. The session-lock API is part of gtk4-layer-shell | Nothing runs |
| `greetd` | The greeter only | The locker still works |
| `accountsservice` | The user list and the avatars | The greeter shows the user who runs it |
| A compositor for the greeter | The greeter only. `hyprland`, `sway`, or `cage` | The greeter has no screen |
| `python3` | `install.sh`, to copy the monitor modes | The greeter uses the preferred mode |

The compositor of your session must support `ext-session-lock-v1`. Hyprland,
Sway, KDE, and River do.

## Other desktops

PSLDM has no code for one distribution or one desktop. Three files hold every
choice, and each one is easy to change:

- `packaging/greetd/greeter-session` names the compositor for the greeter.
  Change one line for `cage` or `sway`.
- `packaging/greetd/hyprland.conf` is the Hyprland configuration for that
  compositor. Another compositor needs its own file.
- `install.sh --greeter` writes `/etc/psldm/monitors.conf` from `hyprctl`.
  Another compositor keeps the preferred mode of each monitor.

The wallpaper, the font, the avatar, and the session memory work the same
everywhere. They live in `/etc/psldm` and `/var/lib/psldm`.

## License

GPL-3.0-or-later. PSLDM contains code from ReGreet. See `ATTRIBUTION.md`.
