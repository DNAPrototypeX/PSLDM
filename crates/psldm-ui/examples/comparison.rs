// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Draw the picture that the README shows.
//!
//! The example builds four panes: the greeter and the locker, each one in
//! the clock phase and in the password phase. It draws them in one grid and
//! writes a PNG file.
//!
//! ```sh
//! cargo run -p psldm-ui --example comparison -- docs/comparison.png
//! cargo run -p psldm-ui --example comparison -- out.png wall.jpg Inter
//! ```
//!
//! Without a wallpaper file the example draws a gradient, so that the
//! picture needs no photograph in the repository.
//!
//! The example needs a display. Without one, run it under Xvfb:
//!
//! ```sh
//! xvfb-run -a -s "-screen 0 2560x1600x24" cargo run -p psldm-ui \
//!     --example comparison -- docs/comparison.png
//! ```

use std::path::PathBuf;

use gtk::prelude::*;
use psldm_ui::{LoginPane, Mode, Phase, PowerAction, UiConfig, UserInfo};

/// The width of one pane in the picture, in pixels.
///
/// The pane draws at the size of a real screen. A smaller picture would show
/// the clock and the avatar larger than a user ever sees them.
const WIDTH: i32 = 1920;

/// The height of one pane in the picture, in pixels.
const HEIGHT: i32 = 1200;

/// The time that every pane shows.
const CLOCK_TIME: &str = "9:41";

/// The date that every pane shows.
const CLOCK_DATE: &str = "Monday, June 1";

/// The limit on main loop turns while the example waits for the first frame.
const MAX_TURNS: usize = 4000;

/// The width of the gradient wallpaper, in pixels.
///
/// A wallpaper asks for the size of its file, and a widget never gets less
/// than it asks for. The gradient therefore has the size of one pane.
const WALLPAPER_WIDTH: usize = WIDTH as usize;

/// The height of the gradient wallpaper, in pixels.
const WALLPAPER_HEIGHT: usize = HEIGHT as usize;

/// The three colors of the gradient wallpaper, from the top left corner.
const WALLPAPER_STOPS: [(f32, [f32; 3]); 3] = [
    (0.0, [26.0, 32.0, 84.0]),
    (0.5, [94.0, 47.0, 122.0]),
    (1.0, [214.0, 106.0, 74.0]),
];

/// The style of the page around the four panes.
const PAGE_STYLE: &str = "
.shot-page { background: #101014; padding: 40px; }
.shot-caption {
    color: #f5f5f7;
    font-size: 30px;
    font-weight: 600;
    margin-bottom: 16px;
}
.shot-row-caption {
    color: #a1a1a6;
    font-size: 24px;
    margin-right: 16px;
}
.shot-pane { border-radius: 12px; }
";

fn main() {
    let mut args = std::env::args().skip(1);
    let output = args.next().unwrap_or_else(|| "docs/comparison.png".into());
    let wallpaper = args
        .next()
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
    let font = args.next();

    gtk::init().expect("GTK cannot open the display");
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_cursor_blink(false);
        settings.set_gtk_enable_animations(false);
    }
    psldm_ui::load_style(font.as_deref());
    add_page_style();

    let config = UiConfig {
        wallpaper: Some(wallpaper.unwrap_or_else(gradient_wallpaper)),
        ..UiConfig::default()
    };
    let user = shot_user();

    let grid = gtk::Grid::new();
    grid.add_css_class("shot-page");
    grid.set_row_spacing(40);
    grid.set_column_spacing(40);

    grid.attach(&row_caption("Before\nthe first key"), 0, 1, 1, 1);
    grid.attach(&row_caption("After\nthe first key"), 0, 2, 1, 1);
    for (column, mode) in [(1, Mode::Greet), (2, Mode::Lock)] {
        grid.attach(&caption(mode_name(mode)), column, 0, 1, 1);
        for (row, phase) in [(1, Phase::Idle), (2, Phase::Active)] {
            grid.attach(&panel(mode, phase, &config, &user), column, row, 1, 1);
        }
    }

    let window = gtk::Window::builder().decorated(false).child(&grid).build();
    window.present();
    gtk::prelude::RootExt::set_focus(&window, gtk::Widget::NONE);

    let texture = draw(&window, &grid);
    texture.save_to_png(&output).expect("cannot write the file");
    println!("Wrote {output}: {} x {}", texture.width(), texture.height());
}

/// Build one pane, ready for the picture.
fn panel(mode: Mode, phase: Phase, config: &UiConfig, user: &UserInfo) -> gtk::Widget {
    let pane = LoginPane::new(mode, config, user);
    pane.freeze_clock(CLOCK_TIME, CLOCK_DATE);
    pane.set_phase(phase);
    if mode == Mode::Greet {
        pane.add_power_button(PowerAction::Shutdown, |_| {});
        pane.add_power_button(PowerAction::Restart, |_| {});
        pane.set_users(&[user.clone(), other_user()], |_| {});
        pane.set_sessions(&["Hyprland".into(), "Sway".into(), "GNOME".into()]);
        pane.select_session("Hyprland");
    }

    let widget = pane.widget().clone();
    widget.add_css_class("shot-pane");
    widget.set_size_request(WIDTH, HEIGHT);

    // The wallpaper asks for the size of its file, and a widget never gets
    // less than it asks for. gtk::Fixed gives the pane the size in the
    // request, so every panel keeps the same size.
    let holder = gtk::Fixed::new();
    holder.set_size_request(WIDTH, HEIGHT);
    holder.put(&widget, 0.0, 0.0);

    // The pane belongs to the picture now, and nothing else holds it.
    std::mem::forget(pane);
    holder.upcast()
}

/// Draw the grid and read the pixels.
fn draw(window: &gtk::Window, grid: &gtk::Grid) -> gtk::gdk::Texture {
    let context = gtk::glib::MainContext::default();
    let paintable = gtk::WidgetPaintable::new(Some(grid));

    for _ in 0..MAX_TURNS {
        context.iteration(false);
        let width = grid.width();
        let height = grid.height();
        if !grid.is_mapped() || width < WIDTH || height < HEIGHT {
            continue;
        }

        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(&snapshot, f64::from(width), f64::from(height));
        let Some(node) = snapshot.to_node() else {
            continue;
        };
        let renderer = window
            .native()
            .and_then(|native| native.renderer())
            .expect("the window has no renderer");
        return renderer.render_texture(&node, None);
    }

    panic!("The panes drew nothing. Is the window on the screen?");
}

/// Draw a gradient wallpaper in a temporary file, and give back its path.
fn gradient_wallpaper() -> PathBuf {
    let mut pixels = Vec::with_capacity(WALLPAPER_WIDTH * WALLPAPER_HEIGHT * 4);
    for y in 0..WALLPAPER_HEIGHT {
        for x in 0..WALLPAPER_WIDTH {
            let across = x as f32 / WALLPAPER_WIDTH as f32;
            let down = y as f32 / WALLPAPER_HEIGHT as f32;
            let [red, green, blue] = gradient_color(0.85 * down + 0.15 * across);
            pixels.extend_from_slice(&[red, green, blue, 255]);
        }
    }

    let texture = gtk::gdk::MemoryTexture::new(
        WALLPAPER_WIDTH as i32,
        WALLPAPER_HEIGHT as i32,
        gtk::gdk::MemoryFormat::R8g8b8a8,
        &gtk::glib::Bytes::from_owned(pixels),
        WALLPAPER_WIDTH * 4,
    );

    let path = std::env::temp_dir().join("psldm-comparison-wallpaper.png");
    texture
        .save_to_png(&path)
        .expect("cannot write the wallpaper");
    path
}

/// Mix the three colors of the gradient at one point, from 0.0 to 1.0.
fn gradient_color(point: f32) -> [u8; 3] {
    let point = point.clamp(0.0, 1.0);
    let stops = WALLPAPER_STOPS;
    let (start, end) = if point <= stops[1].0 {
        (stops[0], stops[1])
    } else {
        (stops[1], stops[2])
    };

    let span = end.0 - start.0;
    let share = if span > 0.0 {
        (point - start.0) / span
    } else {
        0.0
    };
    let mut color = [0u8; 3];
    for ((channel, from), to) in color.iter_mut().zip(start.1).zip(end.1) {
        *channel = (from + (to - from) * share).round().clamp(0.0, 255.0) as u8;
    }
    color
}

/// Add the style of the page. The panes keep the stylesheet of the programs.
fn add_page_style() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(PAGE_STYLE);
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_USER + 2,
        );
    }
}

/// The label over one column.
fn caption(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("shot-caption");
    label
}

/// The label to the left of one row.
fn row_caption(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("shot-row-caption");
    label.set_justify(gtk::Justification::Right);
    label.set_xalign(1.0);
    label
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Greet => "Greeter",
        Mode::Lock => "Locker",
    }
}

fn shot_user() -> UserInfo {
    UserInfo {
        username: "ada".into(),
        display_name: "Ada Lovelace".into(),
        avatar: None,
    }
}

fn other_user() -> UserInfo {
    UserInfo {
        username: "grace".into(),
        display_name: "Grace Hopper".into(),
        avatar: None,
    }
}
