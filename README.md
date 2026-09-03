# PSLDM

Paul's Screen Locker and Display Manager: one login screen for both jobs.

The greeter logs you in. The locker unlocks the session you already have.
Both draw the same pane from the same code, so the two screens match. The
greeter adds a user row, two power buttons, and a session list. Nothing else
differs, and a test compares the two pane by pane.

![The locker and the greeter, side by side](docs/comparison.png)

## Install

Arch Linux:

```sh
sudo pacman -S rust gtk4 gtk4-layer-shell greetd accountsservice hyprland
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
| `/etc/psldm/monitors.conf` | The monitor modes of your desktop |
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

Run the script again after you change a monitor, a font, or an avatar.

## Try it before you switch

```sh
psldm-lock --preview ~/path/to/wallpaper.jpg
```

Press any key, then type `pass`. The preview backend accepts only that word,
and it needs no password of yours. `psldm-greet --preview` shows the greeter
parts as well.

## Make it your login screen

Test the greeter on a spare virtual terminal first. Your own login does not
change while you do.

1. Set `vt = 2` in a copy of `/etc/greetd/config.toml`.
2. Run `sudo greetd --config /etc/greetd/psldm-test.toml`, then press
   Ctrl+Alt+F2.
3. Log in. Pick your session in the list at the bottom. The greeter keeps
   that choice in `/var/lib/psldm/state.toml`.

When that works twice:

```sh
sudo systemctl disable sddm.service      # or your display manager
sudo systemctl enable greetd.service
```

Keep the old display manager installed. If the greeter fails at boot, press
Ctrl+Alt+F2 for a text login and turn it back on.

## Make it your screen locker

Bind a key, and point your idle daemon at the same command:

```
bind = SUPER, L, exec, pgrep -x psldm-lock || psldm-lock
```

`packaging/hypr/psldm-lock.conf` holds that line for Hyprland. An idle daemon
needs the full path, because it often runs with a short PATH.

## The screen

The layout follows macOS Sonoma.

- The clock and the date sit at the top.
- The avatar and the name sit near the bottom.
- The greeter puts two small power buttons at the bottom left, and the
  session list at the bottom right.

The screen has two phases. Before the first key it shows the picker only.
The first key slides the password field in under the name over 260
milliseconds, and the picker rises as the field grows. That key does not
reach the field.

The screen returns to the first phase after 30 seconds without a key, but
only while the field is empty. A row of other users appears in the middle of
the bottom bar when the computer has more than one.

The bottom bar floats over the pane, so the height of the power buttons and
the user row cannot move the avatar. A test compares the two modes above the
bar and requires an exact match.

## How it works

| Crate | Purpose |
| --- | --- |
| `psldm-ui` | The pane, its state machine, and `assets/style.css` |
| `psldm-auth` | The greetd backend and the PAM backend, behind one channel pair |
| `psldm-session` | Users, avatars, sessions, and the settings in `/etc/psldm` |
| `psldm-greet` | The greeter binary for greetd |
| `psldm-lock` | The locker binary |

| Program | Surface | Backend |
| --- | --- | --- |
| `psldm-greet` | `wlr-layer-shell` overlay, one for each monitor | greetd |
| `psldm-lock` | `ext-session-lock-v1`, one for each monitor | PAM |

The compositor keeps the lock surface even if `psldm-lock` stops, so a crash
does not open the session.

One value, `Mode`, decides what the greeter adds. Everything else is shared.

## Commands

| Command | What it does |
| --- | --- |
| `psldm-lock` | Locks the session |
| `psldm-greet` | Shows the greeter. greetd starts it |
| `psldm-lock --preview [WALLPAPER]` | The pane in a window, demo password `pass` |
| `psldm-greet --preview [WALLPAPER]` | The same, with the greeter parts |
| `psldm-lock --check [USER]` | The real PAM stack, on the terminal |
| `psldm-greet --users` | The users and the sessions that it finds |

`psldm-lock --preview-lock` and `psldm-greet --preview-layer` test the real
surfaces inside a nested compositor, with the demo backend.

## Tests

```sh
cargo test --workspace
```

The state machine tests need nothing. One test, `same_pane`, draws both modes
and compares every pixel:

1. It draws one pane twice and requires the same pixels, so that the
   comparison means something.
2. It requires a new pane to draw the idle phase, with no field.
3. It requires the greeter to draw more than the locker.
4. It requires the two modes to match above the bottom bar, with the greeter
   parts on the screen. The bar once stood under the picker and moved the
   avatar up in the greeter alone.
5. It hides the three greeter parts, then requires an exact match everywhere.

A 3 pixel margin that only the greeter uses makes step 5 fail with about 3900
different pixels.

The test needs a Wayland or an X11 display, and it opens two windows for a
moment. Without a display it reports the reason and stops. Set
`PSLDM_TEST_DUMP` to a directory to save the drawings as PPM files.

## Requirements

| Package | Needed for | Without it |
| --- | --- | --- |
| `gtk4`, `gtk4-layer-shell` 1.1 or later | Both programs. The session-lock API is part of gtk4-layer-shell | Nothing runs |
| `greetd` | The greeter only | The locker still works |
| `accountsservice` | The user list and the avatars | The greeter shows the user who runs it |
| A compositor for the greeter | `hyprland`, `sway`, or `cage` | The greeter has no screen |
| `python3` | `install.sh`, to copy the monitor modes | The greeter uses the preferred mode |

The compositor of your session must support `ext-session-lock-v1`. Hyprland,
Sway, KDE, and River do.

## Other desktops

PSLDM has no code for one distribution or one desktop. Three files hold every
choice, and each one is easy to change:

- `packaging/greetd/greeter-session` names the compositor for the greeter.
  Change one line for `cage` or `sway`.
- `packaging/greetd/hyprland.conf` configures that compositor. Another
  compositor needs its own file.
- `install.sh --greeter` writes `/etc/psldm/monitors.conf` with `hyprctl`.
  Another compositor keeps the preferred mode of each monitor.

The wallpaper, the font, the avatar, and the session memory work the same
everywhere.

## License

GPL-3.0-or-later. PSLDM contains code from ReGreet. See `ATTRIBUTION.md`.
