// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The round picture of a user.
//!
//! A `gtk::Picture` asks for the size of its image, so a large photograph
//! makes a large widget. This widget reports one fixed size instead, and it
//! clips the child to a circle.

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

mod imp {
    use std::cell::{Cell, RefCell};

    use gtk::graphene::Rect;
    use gtk::gsk::RoundedRect;

    use super::*;

    pub struct Avatar {
        pub child: RefCell<Option<gtk::Widget>>,
        pub size: Cell<i32>,
    }

    impl Default for Avatar {
        fn default() -> Self {
            Self {
                child: RefCell::new(None),
                size: Cell::new(96),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Avatar {
        const NAME: &'static str = "PsldmAvatar";
        type Type = super::Avatar;
        type ParentType = gtk::Widget;

    }

    impl ObjectImpl for Avatar {
        fn dispose(&self) {
            if let Some(child) = self.child.borrow_mut().take() {
                child.unparent();
            }
        }
    }

    // The widget has no layout manager on purpose. GTK asks a layout manager
    // first, and it never calls these two methods when one is set.
    impl WidgetImpl for Avatar {
        /// Report the fixed size, and ignore the size of the image.
        fn measure(&self, _orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let size = self.size.get();
            (size, size, -1, -1)
        }

        /// Give the whole widget to the child.
        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            if let Some(child) = self.child.borrow().as_ref() {
                child.allocate(width, height, baseline, None);
            }
        }

        /// Clip the child to a circle.
        ///
        /// A CSS radius needs one value for each avatar size. This clip works
        /// for every size.
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let widget = self.obj();
            let width = widget.width() as f32;
            let height = widget.height() as f32;
            let radius = width.min(height) / 2.0;
            let clip = RoundedRect::from_rect(Rect::new(0.0, 0.0, width, height), radius);

            snapshot.push_rounded_clip(&clip);
            // GTK draws a CSS background around this method, not inside it,
            // so a CSS color would keep the square shape. Fill the circle
            // here instead. A user without a photograph sees this color.
            snapshot.append_color(
                &gtk::gdk::RGBA::new(1.0, 1.0, 1.0, 0.18),
                &Rect::new(0.0, 0.0, width, height),
            );
            self.parent_snapshot(snapshot);
            snapshot.pop();

            // A CSS box-shadow keeps the square shape of the widget, so draw
            // the ring here, on the same rounded rectangle as the clip.
            let ring = gtk::gdk::RGBA::new(1.0, 1.0, 1.0, 0.28);
            snapshot.append_border(&clip, &[1.0; 4], &[ring; 4]);
        }
    }
}

glib::wrapper! {
    pub struct Avatar(ObjectSubclass<imp::Avatar>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Avatar {
    /// Build a round avatar with a side of `size` pixels.
    pub fn new(child: &impl IsA<gtk::Widget>, size: i32) -> Self {
        let object: Self = glib::Object::builder().build();
        object.imp().size.set(size);
        object.add_css_class("psldm-avatar");
        object.set_halign(gtk::Align::Center);
        object.set_valign(gtk::Align::Center);
        object.set_overflow(gtk::Overflow::Hidden);

        let child = child.clone().upcast::<gtk::Widget>();
        child.set_parent(&object);
        object.imp().child.replace(Some(child));
        object
    }

    /// Replace the child.
    pub fn set_child(&self, child: &impl IsA<gtk::Widget>) {
        if let Some(old) = self.imp().child.borrow_mut().take() {
            old.unparent();
        }
        let child = child.clone().upcast::<gtk::Widget>();
        child.set_parent(self);
        self.imp().child.replace(Some(child));
    }
}
