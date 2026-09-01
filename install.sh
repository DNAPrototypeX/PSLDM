#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Paul Moore
#
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Install PSLDM. Run it from the repository, as your own user. The script
# calls sudo for the steps that write to /etc and to the prefix.

set -euo pipefail

readonly REPO="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly BACKUP_SUFFIX=".psldm-backup"

PREFIX="/usr/local"
DESTDIR="${DESTDIR:-}"
WALLPAPER=""
AVATAR=""
FONT=""
WITH_GREETER=0
ENABLE_GREETD=0
BUILD=1
UNINSTALL=0
DRY_RUN=0

usage() {
    cat <<'USAGE'
Usage: ./install.sh [OPTION]...

Options:
  --prefix DIR       Install the programs in DIR/bin. Default: /usr/local
  --wallpaper PATH   Copy this image to /etc/psldm/wallpaper
  --avatar PATH      Copy this image to the AccountsService icon of the user.
                     Without the option the script looks for ~/.face.icon and
                     ~/.face. The greeter cannot read a private home
                     directory, so it needs this copy
  --font NAME        Use this font family for the pane. Without the option
                     the script reads the font of your desktop
  --greeter          Also install the greetd files in /etc/greetd
  --enable-greetd    Start greetd at boot. This turns on --greeter
  --no-build         Use the programs in target/release as they are
  --uninstall        Remove every file that this script installs
  --destdir DIR      Write every file under DIR. For a package build
  --dry-run          Print the steps, and change nothing
  -h, --help         Print this text

The script keeps a copy of every file that it replaces. The copy has the
suffix .psldm-backup.

WARNING: --enable-greetd changes how you log in. Test the greeter first with
`psldm-greet --preview`, and keep a second way to reach the computer.
USAGE
}

say() {
    printf '%s\n' "$*"
}

fail() {
    printf 'Error: %s\n' "$*" >&2
    exit 1
}

# The command that raises privileges. Root needs none, and a test can set
# PSLDM_SUDO to an empty value.
if [[ "$(id -u)" -eq 0 ]]; then
    SUDO=""
else
    SUDO="${PSLDM_SUDO-sudo}"
fi

# Run one privileged command, or print it in a dry run.
run() {
    if [[ "$DRY_RUN" -eq 1 ]]; then
        printf '  would run: %s%s\n' "${SUDO:+$SUDO }" "$*"
        return 0
    fi
    if [[ -n "$SUDO" ]]; then
        "$SUDO" "$@"
    else
        "$@"
    fi
}

# A path under the destination directory.
target() {
    printf '%s%s' "$DESTDIR" "$1"
}

# Keep a copy of a file before the script replaces it.
back_up() {
    local path="$1"
    [[ -e "$path" && ! -e "$path$BACKUP_SUFFIX" ]] || return 0
    say "  keeping a copy of $path"
    run cp -a "$path" "$path$BACKUP_SUFFIX"
}

parse_arguments() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --prefix) PREFIX="${2:?--prefix needs a directory}"; shift 2 ;;
            --wallpaper) WALLPAPER="${2:?--wallpaper needs a path}"; shift 2 ;;
            --avatar) AVATAR="${2:?--avatar needs a path}"; shift 2 ;;
            --font) FONT="${2:?--font needs a family name}"; shift 2 ;;
            --greeter) WITH_GREETER=1; shift ;;
            --enable-greetd) ENABLE_GREETD=1; WITH_GREETER=1; shift ;;
            --no-build) BUILD=0; shift ;;
            --uninstall) UNINSTALL=1; shift ;;
            --destdir) DESTDIR="${2:?--destdir needs a directory}"; shift 2 ;;
            --dry-run) DRY_RUN=1; shift ;;
            -h|--help) usage; exit 0 ;;
            *) usage >&2; fail "unknown option $1" ;;
        esac
    done
}

check_tools() {
    [[ "$BUILD" -eq 0 ]] || command -v cargo >/dev/null || fail "cargo is missing"
    if [[ -n "$SUDO" ]]; then
        command -v "$SUDO" >/dev/null || fail "$SUDO is missing"
    fi

    # A package build has no greetd on the build host, so only a real install
    # needs the program.
    if [[ "$WITH_GREETER" -eq 1 && -z "$DESTDIR" ]] && ! command -v greetd >/dev/null; then
        fail "greetd is missing. Install it, then run this script again"
    fi
    if [[ -n "$WALLPAPER" && ! -f "$WALLPAPER" ]]; then
        fail "no image at $WALLPAPER"
    fi
    if [[ -n "$AVATAR" && ! -f "$AVATAR" ]]; then
        fail "no image at $AVATAR"
    fi
}

build() {
    [[ "$BUILD" -eq 1 ]] || return 0
    say "Building the release programs"
    if [[ "$DRY_RUN" -eq 1 ]]; then
        say "  would run: cargo build --release"
        return 0
    fi
    (cd "$REPO" && cargo build --release)
}

install_programs() {
    say "Installing the programs in $PREFIX/bin"
    local program
    for program in psldm-lock psldm-greet; do
        local source="$REPO/target/release/$program"
        if [[ "$DRY_RUN" -eq 0 && ! -x "$source" ]]; then
            fail "no program at $source. Run the script without --no-build"
        fi
        run install -Dm755 "$source" "$(target "$PREFIX/bin/$program")"
    done
}

install_pam() {
    say "Installing the PAM file /etc/pam.d/psldm"
    back_up "$(target /etc/pam.d/psldm)"
    run install -Dm644 "$REPO/packaging/pam.d/psldm" "$(target /etc/pam.d/psldm)"
}

install_wallpaper() {
    [[ -n "$WALLPAPER" ]] || return 0
    say "Copying the wallpaper to /etc/psldm/wallpaper"
    # The greeter runs as another user, so the file must be a readable copy.
    # A link into your home directory does not work.
    back_up "$(target /etc/psldm/wallpaper)"
    run install -Dm644 "$WALLPAPER" "$(target /etc/psldm/wallpaper)"
}

# The name of the person who runs the script, not root.
current_user() {
    printf '%s' "${SUDO_USER:-${USER:-$(id -un)}}"
}

# The font family of the desktop, without the size.
#
# gsettings holds the name that most desktops use. The GTK settings file and
# fontconfig are the fallbacks.
desktop_font() {
    local name=""

    if command -v gsettings >/dev/null; then
        name="$(gsettings get org.gnome.desktop.interface font-name 2>/dev/null || true)"
        name="${name#\'}"
        name="${name%\'}"
    fi
    if [[ -z "$name" && -r "$HOME/.config/gtk-4.0/settings.ini" ]]; then
        name="$(sed -n 's/^gtk-font-name[[:space:]]*=[[:space:]]*//p' \
            "$HOME/.config/gtk-4.0/settings.ini" | head -1)"
    fi
    if [[ -z "$name" ]] && command -v fc-match >/dev/null; then
        name="$(fc-match --format '%{family[0]}' sans 2>/dev/null || true)"
        printf '%s' "$name"
        return 0
    fi

    # A font name ends with a size and a style, such as "Adwaita Sans 11".
    # The pane sets its own sizes, so keep the family only.
    printf '%s' "$name" | sed -E 's/[[:space:]]+[0-9]+([.][0-9]+)?$//; s/[[:space:]]+(Bold|Italic|Oblique|Light|Medium|Regular|Semi-Bold|SemiBold)$//'
}

install_font() {
    local family="$FONT"
    [[ -n "$family" ]] || family="$(desktop_font)"
    if [[ -z "$family" ]]; then
        say "No font found. The pane will use the default sans font"
        return 0
    fi

    say "Copying the font family to /etc/psldm/font: $family"
    warn_about_font "$family"

    local temporary
    temporary="$(mktemp)"
    printf '%s\n' "$family" > "$temporary"
    run install -Dm644 "$temporary" "$(target /etc/psldm/font)"
    rm -f "$temporary"
}

# The greeter cannot read a home directory, so the font must be in a system
# directory.
warn_about_font() {
    command -v fc-list >/dev/null || return 0
    local files
    files="$(fc-list : file family 2>/dev/null | grep -F "$1" || true)"
    if [[ -z "$files" ]]; then
        say "  WARNING: no font named $1 is installed"
        return 0
    fi
    if ! printf '%s' "$files" | grep -qE '^/(usr|etc)/'; then
        say "  WARNING: $1 is not in a system directory."
        say "  The greeter runs as another user and cannot read it."
    fi
}

install_avatar() {
    local source="$AVATAR"
    if [[ -z "$source" ]]; then
        local candidate
        for candidate in "$HOME/.face.icon" "$HOME/.face"; do
            [[ -f "$candidate" ]] || continue
            source="$candidate"
            break
        done
    fi
    [[ -n "$source" ]] || return 0

    local user
    user="$(current_user)"
    say "Copying the avatar to /var/lib/AccountsService/icons/$user"
    # A home directory is often private, and the greeter runs as another
    # user. Only this copy is readable for both programs.
    back_up "$(target "/var/lib/AccountsService/icons/$user")"
    run install -Dm644 "$source" "$(target "/var/lib/AccountsService/icons/$user")"
}

# Write the monitors of the running desktop, so that the greeter uses the
# same modes. Without this file the greeter uses the preferred mode of each
# monitor, and a laptop panel often prefers a larger mode than the desktop
# uses. Everything then looks smaller in the greeter.
install_monitors() {
    [[ "$WITH_GREETER" -eq 1 ]] || return 0

    if ! command -v hyprctl >/dev/null; then
        say "No hyprctl. The greeter will use the preferred mode of each monitor"
        return 0
    fi

    local temporary
    temporary="$(mktemp)"
    if ! hyprctl monitors -j 2>/dev/null | monitor_lines > "$temporary"; then
        say "Cannot read the monitors. The greeter will use the preferred mode"
        rm -f "$temporary"
        return 0
    fi

    say "Copying the monitor settings to /etc/psldm/monitors.conf"
    if [[ "$DRY_RUN" -eq 1 ]]; then
        sed 's/^/    /' "$temporary"
    fi
    run install -Dm644 "$temporary" "$(target /etc/psldm/monitors.conf)"
    rm -f "$temporary"
}

# Turn the JSON of hyprctl into Hyprland monitor lines.
monitor_lines() {
    python3 -c '
import json, sys

monitors = json.load(sys.stdin)
if not monitors:
    raise SystemExit(1)

print("# Written by install.sh from the running desktop. Do not edit.")
for monitor in monitors:
    mode = f"{monitor["width"]}x{monitor["height"]}@{monitor["refreshRate"]:.2f}"
    position = f"{monitor["x"]}x{monitor["y"]}"
    line = f"monitor = {monitor["name"]}, {mode}, {position}, {monitor["scale"]}"
    if monitor.get("transform"):
        line += f", transform, {monitor["transform"]}"
    print(line)
'
}

install_greeter() {
    [[ "$WITH_GREETER" -eq 1 ]] || return 0
    say "Installing the greetd files in /etc/greetd"

    back_up "$(target /etc/greetd/config.toml)"
    back_up "$(target /etc/greetd/hyprland.conf)"
    run install -Dm644 "$REPO/packaging/greetd/config.toml" \
        "$(target /etc/greetd/config.toml)"

    # The greeter session must call the program where this script put it.
    local temporary
    temporary="$(mktemp)"
    sed "s|/usr/bin/psldm-greet|$PREFIX/bin/psldm-greet|" \
        "$REPO/packaging/greetd/hyprland.conf" > "$temporary"
    run install -Dm644 "$temporary" "$(target /etc/greetd/hyprland.conf)"
    rm -f "$temporary"
}

enable_greetd() {
    [[ "$ENABLE_GREETD" -eq 1 ]] || return 0
    if [[ -n "$DESTDIR" ]]; then
        say "Not starting greetd, because --destdir is set"
        return 0
    fi
    say "Starting greetd at boot"
    run systemctl enable greetd.service
}

report() {
    say ""
    say "Done."
    say ""
    say "Test the locker now:"
    say "  $PREFIX/bin/psldm-lock --preview"
    say ""
    say "Add this line to ~/.config/hypr/hyprland.conf to lock with SUPER + L:"
    say "  bind = SUPER, L, exec, pgrep -x psldm-lock || $PREFIX/bin/psldm-lock"

    if [[ "$WITH_GREETER" -eq 1 && "$ENABLE_GREETD" -eq 0 ]]; then
        say ""
        say "The greetd files are in place. Start greetd at boot with:"
        say "  sudo systemctl enable greetd.service"
    fi
    if [[ "$ENABLE_GREETD" -eq 1 ]]; then
        say ""
        say "greetd starts at the next boot. Keep a second way to reach the"
        say "computer until the greeter works."
    fi
}

uninstall() {
    say "Removing PSLDM"
    local path
    for path in "$(target "$PREFIX/bin/psldm-lock")" \
                "$(target "$PREFIX/bin/psldm-greet")" \
                "$(target /etc/pam.d/psldm)" \
                "$(target /etc/psldm/wallpaper)" \
                "$(target /etc/psldm/monitors.conf)" \
                "$(target /etc/psldm/font)" \
                "$(target "/var/lib/AccountsService/icons/$(current_user)")" \
                "$(target /etc/greetd/hyprland.conf)" \
                "$(target /etc/greetd/config.toml)"; do
        [[ -e "$path" ]] || continue
        say "  removing $path"
        run rm -f "$path"
        if [[ -e "$path$BACKUP_SUFFIX" ]]; then
            say "  restoring $path from the copy"
            run mv "$path$BACKUP_SUFFIX" "$path"
        fi
    done

    # The wallpaper is the only file in this directory.
    run rmdir --ignore-fail-on-non-empty "$(target /etc/psldm)" 2>/dev/null || true

    say ""
    say "greetd still starts at boot. Turn it off with:"
    say "  sudo systemctl disable greetd.service"
}

main() {
    parse_arguments "$@"
    check_tools

    if [[ "$DRY_RUN" -eq 1 ]]; then
        say "Dry run. The script changes nothing."
    fi

    if [[ "$UNINSTALL" -eq 1 ]]; then
        uninstall
        exit 0
    fi

    build
    install_programs
    install_pam
    install_wallpaper
    install_avatar
    install_font
    install_monitors
    install_greeter
    enable_greetd
    report
}

main "$@"
