// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The blurred wallpaper behind the login pane.
//!
//! The widget draws the wallpaper, applies a blur, and then draws a dark
//! layer on top. The greeter and the locker use the same widget, so the two
//! backgrounds match.

use std::path::Path;

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

mod imp {
    use std::cell::{Cell, RefCell};

    use gtk::gdk::RGBA;
    use gtk::graphene::{Point, Rect};
    use gtk::gsk::ColorStop;

    use super::*;

    #[derive(Default)]
    pub struct Background {
        pub picture: RefCell<Option<gtk::Picture>>,
        pub blur: Cell<f64>,
        pub scrim: Cell<f64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Background {
        const NAME: &'static str = "PsldmBackground";
        type Type = super::Background;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk::BinLayout>();
        }
    }

    impl ObjectImpl for Background {
        fn dispose(&self) {
            if let Some(picture) = self.picture.borrow_mut().take() {
                picture.unparent();
            }
        }
    }

    impl WidgetImpl for Background {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let widget = self.obj();
            let width = widget.width() as f32;
            let height = widget.height() as f32;
            if width <= 0.0 || height <= 0.0 {
                return;
            }
            let bounds = Rect::new(0.0, 0.0, width, height);
            let picture = self.picture.borrow();

            match picture.as_ref() {
                Some(picture) => {
                    let blur = self.blur.get();
                    // A blur reads pixels from outside the widget, and those
                    // pixels are transparent. Draw the wallpaper larger than
                    // the widget, so the edges keep their color.
                    let overscan = blur as f32;
                    snapshot.push_blur(blur);
                    snapshot.save();
                    snapshot.translate(&Point::new(-overscan, -overscan));
                    snapshot.scale(
                        (width + 2.0 * overscan) / width,
                        (height + 2.0 * overscan) / height,
                    );
                    widget.snapshot_child(picture, snapshot);
                    snapshot.restore();
                    snapshot.pop();
                }
                None => {
                    // No wallpaper. Draw a dark gradient instead.
                    snapshot.append_linear_gradient(
                        &bounds,
                        &Point::new(0.0, 0.0),
                        &Point::new(0.0, height),
                        &[
                            ColorStop::new(0.0, RGBA::new(0.13, 0.14, 0.18, 1.0)),
                            ColorStop::new(1.0, RGBA::new(0.04, 0.04, 0.06, 1.0)),
                        ],
                    );
                }
            }

            let scrim = self.scrim.get() as f32;
            if scrim > 0.0 {
                snapshot.append_color(&RGBA::new(0.0, 0.0, 0.0, scrim), &bounds);
            }
        }
    }
}

glib::wrapper! {
    pub struct Background(ObjectSubclass<imp::Background>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Background {
    /// Create the background.
    ///
    /// `blur` is the blur radius in pixels. `scrim` is the opacity of the dark
    /// layer, from 0.0 to 1.0. A missing or unreadable wallpaper gives a dark
    /// gradient.
    pub fn new(wallpaper: Option<&Path>, blur: f64, scrim: f64) -> Self {
        let object: Self = glib::Object::builder().build();

        if let Some(path) = wallpaper.filter(|path| path.exists()) {
            let picture = gtk::Picture::for_filename(path);
            picture.set_content_fit(gtk::ContentFit::Cover);
            picture.set_can_shrink(true);
            picture.set_parent(&object);
            object.imp().picture.replace(Some(picture));
        }

        object.imp().blur.set(blur);
        object.imp().scrim.set(scrim);
        object
    }
}
