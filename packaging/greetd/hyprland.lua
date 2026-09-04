-- Hyprland configuration for the PSLDM greeter.
-- Install it at /etc/greetd/hyprland.lua.
--
-- The compositor exists only to hold the greeter. It stops as soon as the
-- greeter stops, and greetd then starts the user session.

-- The fallback, for a monitor that the next file does not name.
hl.monitor({
    output   = "",
    mode     = "preferred",
    position = "auto",
    scale    = 1,
})

-- The modes of the desktop session. install.sh writes this file, so that the
-- greeter and the session show the pane at the same size. The file is absent
-- until install.sh --greeter runs on a desktop that has hyprctl, so read it
-- with pcall.
pcall(dofile, "/etc/psldm/monitors.lua")

hl.config({
    misc = {
        disable_hyprland_logo    = true,
        disable_splash_rendering = true,
        force_default_wallpaper  = 0,
    },

    animations = {
        enabled = false,
    },

    -- Set the same keyboard layout as the session, or the password fails.
    input = {
        kb_layout = "us",
    },
})

hl.on("hyprland.start", function()
    hl.exec_cmd([[sh -c '/usr/bin/psldm-greet; hyprctl dispatch exit']])
end)
