// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The proof that the greeter and the locker draw the same pane.
//!
//! The test builds one pane for each mode, hides the three parts that the
//! greeter owns, draws both, and compares every pixel. A change that only one
//! mode shows makes this test fail.
//!
//! The test needs a Wayland or an X11 display, because GTK draws with the
//! graphics driver. Without a display the test reports the reason and stops.
//! Set `PSLDM_REQUIRE_DISPLAY` to make the missing display a failure. The
//! continuous-integration job sets it, so that a skipped test cannot pass.

use gtk::prelude::*;
use psldm_ui::{Chrome, LoginPane, Mode, Phase, UiConfig, UserInfo};

/// The width of the drawing, in pixels.
const WIDTH: i32 = 1280;

/// The height of the drawing, in pixels.
const HEIGHT: i32 = 800;

/// The height of the bottom bar and its margin, in pixels.
///
/// The greeter draws its power buttons and its session list there. Every row
/// above it must match the locker.
const BAR_HEIGHT: i32 = 96;

/// The time that both panes show during the test.
const CLOCK_TIME: &str = "9:41";

/// The date that both panes show during the test.
const CLOCK_DATE: &str = "Monday, June 1";

/// The limit on main loop turns while the test waits for the first frame.
const MAX_TURNS: usize = 2000;

/// Nothing that the greeter owns is visible during the comparison.
const NO_CHROME: Chrome = Chrome {
    power_menu: false,
    user_picker: false,
    session_picker: false,
};

/// One test holds both checks, because GTK must stay on one thread and the
/// test harness gives each test its own thread.
#[test]
fn the_two_modes_draw_the_same_pane() {
    if !start_gtk() {
        return;
    }

    let config = UiConfig::default();
    let user = test_user();

    let lock = Drawing::new(Mode::Lock, &config, &user);
    let greet = Drawing::new(Mode::Greet, &config, &user);

    // The two panes draw one after the other. A minute that changes between
    // the two drawings makes two equal panes differ, so both panes show one
    // fixed time.
    lock.pane.freeze_clock(CLOCK_TIME, CLOCK_DATE);
    greet.pane.freeze_clock(CLOCK_TIME, CLOCK_DATE);

    // One pane must draw the same pixels twice. Without this check, a change
    // between two drawings would hide a real difference.
    let unstable = count_different(&lock.pixels(), &lock.pixels());
    assert_eq!(
        unstable, 0,
        "One pane drew {unstable} different pixels twice. The comparison \
         below cannot mean anything until this is 0."
    );

    // A new pane must already show the clock only. The state machine starts
    // in Phase::Idle, and a pane that starts anywhere else shows the field
    // for one frame and then fades it away.
    let fresh = lock.pixels();
    lock.pane.set_phase(Phase::Idle);
    assert_eq!(
        count_different(&fresh, &lock.pixels()),
        0,
        "A new pane does not draw the idle phase."
    );

    lock.pane.set_phase(Phase::Active);
    assert!(
        count_different(&fresh, &lock.pixels()) > 0,
        "The pane looks the same in both phases."
    );

    // Compare the two modes with the whole pane on the screen. The idle
    // phase hides the avatar, the name, and the field, and a difference
    // there would stay invisible.
    greet.pane.set_phase(Phase::Active);

    // The greeter must draw more than the locker. Without this check, a pane
    // that draws nothing would pass the comparison below.
    let with_chrome = count_different(&lock.pixels(), &greet.pixels());
    dump("lock", &lock.pixels(), WIDTH as usize, HEIGHT as usize);
    dump(
        "greet-chrome",
        &greet.pixels(),
        WIDTH as usize,
        HEIGHT as usize,
    );
    assert!(
        with_chrome > 0,
        "The greeter must show the power buttons, the user row, and the \
         session list."
    );

    // The bar must not move the pane. The greeter keeps its power buttons
    // and its session list here, and only those rows may differ. The bar
    // once stood under the picker and pushed it up in the greeter alone.
    let above_bar = count_different_above(&lock.pixels(), &greet.pixels(), BAR_HEIGHT);
    assert_eq!(
        above_bar, 0,
        "The greeter moved the pane. {above_bar} pixels differ above the \
         bottom bar, where the two modes must match."
    );

    // Hide the three parts that the greeter owns. Everything else must match.
    greet.pane.set_chrome(NO_CHROME);
    let lock_pixels = lock.pixels();
    let greet_pixels = greet.pixels();
    dump("lock-2", &lock_pixels, WIDTH as usize, HEIGHT as usize);
    dump(
        "greet-plain",
        &greet_pixels,
        WIDTH as usize,
        HEIGHT as usize,
    );
    let different = count_different(&lock_pixels, &greet_pixels);
    assert_eq!(
        different, 0,
        "The two modes drew {different} different pixels. Only the power \
         buttons, the user row, and the session list may differ, and this \
         comparison hides all three."
    );
}

fn test_user() -> UserInfo {
    UserInfo {
        username: "tester".into(),
        display_name: "Test User".into(),
        avatar: None,
    }
}

/// One pane in one window, ready to draw.
struct Drawing {
    pane: LoginPane,
    window: gtk::Window,
}

impl Drawing {
    fn new(mode: Mode, config: &UiConfig, user: &UserInfo) -> Self {
        let pane = LoginPane::new(mode, config, user);

        // A window manager decides the size of a window, and two windows can
        // differ. gtk::Fixed ignores its own size and gives the child the
        // size that the child asks for, so both panes always lay out at the
        // same size.
        pane.widget().set_size_request(WIDTH, HEIGHT);
        let holder = gtk::Fixed::new();
        holder.put(pane.widget(), 0.0, 0.0);

        let window = gtk::Window::builder()
            .default_width(WIDTH)
            .default_height(HEIGHT)
            .decorated(false)
            .child(&holder)
            .build();
        window.present();
        // A focused field draws a caret that blinks, so two drawings of one
        // pane would differ. No pane holds the focus during the test.
        gtk::prelude::RootExt::set_focus(&window, gtk::Widget::NONE);

        Self { pane, window }
    }

    /// Draw the pane and read the pixels.
    ///
    /// GTK draws when its main loop runs, so the method turns the loop until
    /// the pane produces a drawing.
    fn pixels(&self) -> Vec<u8> {
        settle();
        let context = gtk::glib::MainContext::default();
        let paintable = gtk::WidgetPaintable::new(Some(self.pane.widget()));

        for _ in 0..MAX_TURNS {
            context.iteration(false);

            let widget = self.pane.widget();
            if !widget.is_mapped() || widget.width() != WIDTH || widget.height() != HEIGHT {
                continue;
            }

            let snapshot = gtk::Snapshot::new();
            paintable.snapshot(&snapshot, f64::from(WIDTH), f64::from(HEIGHT));
            let Some(node) = snapshot.to_node() else {
                continue;
            };

            let renderer = self
                .window
                .native()
                .and_then(|native| native.renderer())
                .expect("the window has no renderer");
            let texture = renderer.render_texture(&node, None);

            let width = texture.width() as usize;
            let height = texture.height() as usize;
            let stride = width * 4;
            let mut pixels = vec![0u8; stride * height];
            texture.download(&mut pixels, stride);
            return pixels;
        }

        panic!("The pane drew nothing. Is the window on the screen?");
    }
}

/// Run the main loop for a moment.
///
/// GTK applies a style change and draws in a frame, and a frame comes from a
/// timer. Reading the pixels at once would give the state before the change.
fn settle() {
    let main_loop = gtk::glib::MainLoop::new(None, false);
    let stop = main_loop.clone();
    gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(150), move || {
        stop.quit();
    });
    main_loop.run();
}

/// Start GTK. Returns `false` when the test must stop.
fn start_gtk() -> bool {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() && std::env::var_os("DISPLAY").is_none() {
        return no_display("no Wayland display and no X11 display");
    }
    if gtk::init().is_err() {
        return no_display("GTK cannot open the display");
    }
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_cursor_blink(false);
        // A CSS transition needs time, and the test reads the pixels at once.
        // Without this the pane still shows the state before the change.
        settings.set_gtk_enable_animations(false);
    }

    // The pane needs the same stylesheet that the two programs load. Without
    // it the test compares panes that no user ever sees.
    psldm_ui::load_style(None);
    true
}

/// Report a display that the test cannot use. Returns `false` to stop.
///
/// A skipped test looks like a pass. `PSLDM_REQUIRE_DISPLAY` therefore turns
/// the skip into a failure, and the job that runs the tests sets it.
fn no_display(reason: &str) -> bool {
    assert!(
        std::env::var_os("PSLDM_REQUIRE_DISPLAY").is_none(),
        "PSLDM_REQUIRE_DISPLAY is set, but the test found {reason}."
    );
    println!("Skipped: {reason}.");
    false
}

/// Save both drawings and their difference, for a failure that needs eyes.
///
/// Set `PSLDM_TEST_DUMP` to a directory to turn this on.
fn dump(name: &str, pixels: &[u8], width: usize, height: usize) {
    let Some(directory) = std::env::var_os("PSLDM_TEST_DUMP") else {
        return;
    };
    let path = std::path::Path::new(&directory).join(format!("{name}.ppm"));
    let mut data = format!("P6\n{width} {height}\n255\n").into_bytes();
    for pixel in pixels.chunks_exact(4) {
        data.extend_from_slice(&pixel[0..3]);
    }
    if let Err(err) = std::fs::write(&path, data) {
        println!("Cannot write {}: {err}", path.display());
    }
}

/// Count the pixels that differ above the bottom bar.
fn count_different_above(left: &[u8], right: &[u8], bar_height: i32) -> usize {
    let rows = (HEIGHT - bar_height).max(0) as usize;
    let row_bytes = WIDTH as usize * 4;
    let end = rows * row_bytes;
    count_different(&left[..end.min(left.len())], &right[..end.min(right.len())])
}

/// Count the pixels that differ.
fn count_different(left: &[u8], right: &[u8]) -> usize {
    assert_eq!(left.len(), right.len(), "The two drawings differ in size.");
    left.chunks_exact(4)
        .zip(right.chunks_exact(4))
        .filter(|(left, right)| left != right)
        .count()
}
