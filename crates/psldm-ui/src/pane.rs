// SPDX-FileCopyrightText: 2026 Paul Moore
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The login pane.
//!
//! The greeter and the locker build the same pane. [`crate::Mode`] decides
//! whether the power buttons, the user row, and the session list appear.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk::glib;
use gtk::prelude::*;

use crate::state::{LoginState, MessageKind, Phase};
use crate::{Chrome, Mode, UiConfig};
use crate::avatar::Avatar;
use crate::background::Background;

/// The side of the avatar over the name, in pixels.
const AVATAR_SIZE: i32 = 96;

/// The side of an avatar in the user row, in pixels.
const USER_ROW_AVATAR_SIZE: i32 = 44;

/// What a power button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    Restart,
    Shutdown,
}

/// One user in the user row.
#[derive(Debug, Clone)]
pub struct UserInfo {
    /// The system user name, such as `paul`.
    pub username: String,
    /// The name to show, such as `Paul Moore`.
    pub display_name: String,
    /// The avatar image of the user.
    pub avatar: Option<PathBuf>,
}

impl UserInfo {
    /// The first letter of the display name, for a user without an avatar.
    fn initial(&self) -> String {
        self.display_name
            .chars()
            .next()
            .map(|letter| letter.to_uppercase().to_string())
            .unwrap_or_else(|| "?".into())
    }
}

/// The widgets of the login pane.
pub struct LoginPane {
    root: gtk::Widget,
    clock_column: gtk::Box,
    center: gtk::Box,
    avatar: Avatar,
    name: gtk::Label,
    entry: gtk::Entry,
    hint: gtk::Label,
    users_row: gtk::Box,
    power_row: gtk::Box,
    sessions: gtk::DropDown,
    shake_source: Rc<RefCell<Option<glib::SourceId>>>,
}

impl LoginPane {
    /// Build the pane for one mode and one user.
    pub fn new(mode: Mode, config: &UiConfig, user: &UserInfo) -> Self {

        let background = Background::new(config.wallpaper.as_deref(), config.blur, config.scrim);

        let clock = build_clock();

        let avatar = Avatar::new(&avatar_content(user), AVATAR_SIZE);
        let name = gtk::Label::new(Some(&user.display_name));
        name.add_css_class("psldm-name");

        let entry = gtk::Entry::new();
        entry.add_css_class("psldm-entry");
        entry.set_halign(gtk::Align::Center);
        entry.set_visibility(false);
        entry.set_input_purpose(gtk::InputPurpose::Password);
        entry.set_secondary_icon_name(Some("go-next-symbolic"));
        entry.set_secondary_icon_activatable(true);
        entry.set_placeholder_text(Some("Enter Password"));
        gtk::prelude::EditableExt::set_alignment(&entry, 0.5);
        entry.set_max_width_chars(18);

        let hint = gtk::Label::new(None);
        hint.add_css_class("psldm-hint");

        let center = gtk::Box::new(gtk::Orientation::Vertical, 0);
        center.add_css_class("psldm-login");
        center.set_halign(gtk::Align::Center);
        center.set_valign(gtk::Align::Center);
        center.append(&avatar);
        center.append(&name);
        center.append(&entry);
        center.append(&hint);

        let users_row = gtk::Box::new(gtk::Orientation::Horizontal, 24);
        users_row.set_halign(gtk::Align::Center);
        users_row.add_css_class("psldm-users");

        let sessions = gtk::DropDown::from_strings(&[]);
        sessions.add_css_class("psldm-session");

        let power_row = gtk::Box::new(gtk::Orientation::Horizontal, 18);
        power_row.set_halign(gtk::Align::Center);
        power_row.add_css_class("psldm-power-row");

        sessions.set_halign(gtk::Align::Center);

        let bottom = gtk::Box::new(gtk::Orientation::Vertical, 14);
        bottom.add_css_class("psldm-bottom");
        bottom.set_valign(gtk::Align::End);
        bottom.append(&users_row);
        bottom.append(&power_row);
        bottom.append(&sessions);

        let column = gtk::CenterBox::new();
        column.set_orientation(gtk::Orientation::Vertical);
        column.set_start_widget(Some(&clock));
        column.set_center_widget(Some(&center));
        column.set_end_widget(Some(&bottom));

        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&background));
        overlay.add_overlay(&column);
        overlay.add_css_class("psldm-root");

        let pane = Self {
            root: overlay.upcast(),
            clock_column: clock,
            center,
            avatar,
            name,
            entry,
            hint,
            users_row,
            power_row,
            sessions,
            shake_source: Rc::new(RefCell::new(None)),
        };
        pane.set_chrome(mode.chrome());
        pane
    }

    /// Show or hide the three parts that the mode owns.
    ///
    /// Everything else is the same in the greeter and in the locker.
    pub fn set_chrome(&self, chrome: Chrome) {
        self.users_row.set_visible(chrome.user_picker);
        self.power_row.set_visible(chrome.power_menu);
        self.sessions.set_visible(chrome.session_picker);
    }

    /// The widget to put in the window.
    pub fn widget(&self) -> &gtk::Widget {
        &self.root
    }

    /// Report whether the field is empty.
    pub fn is_empty(&self) -> bool {
        self.entry.text().is_empty()
    }

    /// Give the keyboard to the field.
    ///
    /// The field keeps the focus while the screen shows the clock only, so
    /// the first key of the password lands in it.
    pub fn focus_entry(&self) {
        self.entry.grab_focus();
    }

    /// Call `handler` on every key and on every click.
    ///
    /// The handler returns `true` when the screen was showing the clock only.
    /// That first key wakes the screen, and the field does not receive it.
    /// Every later key reaches the field.
    pub fn connect_activity(&self, handler: impl Fn() -> bool + Clone + 'static) {
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let key_handler = handler.clone();
        keys.connect_key_pressed(move |_, _, _, _| {
            if key_handler() {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.root.add_controller(keys);

        let clicks = gtk::GestureClick::new();
        clicks.set_propagation_phase(gtk::PropagationPhase::Capture);
        clicks.connect_pressed(move |_, _, _, _| {
            handler();
        });
        self.root.add_controller(clicks);
    }

    /// Show the state of one attempt.
    pub fn render(&self, state: &LoginState) {
        let idle = state.phase == Phase::Idle;
        set_css_class(&self.clock_column, "idle", idle);
        set_css_class(&self.center, "idle", idle);
        self.center.set_can_target(!idle);

        self.entry.set_sensitive(!state.busy);
        self.entry.set_secondary_icon_name(Some(if state.busy {
            "content-loading-symbolic"
        } else {
            "go-next-symbolic"
        }));

        match &state.prompt {
            Some(prompt) => {
                let label = prompt.text.trim_end_matches([':', ' ']);
                self.entry.set_placeholder_text(Some(if prompt.secret {
                    "Enter Password"
                } else {
                    label
                }));
                self.entry.set_visibility(!prompt.secret);
                self.entry.set_input_purpose(if prompt.secret {
                    gtk::InputPurpose::Password
                } else {
                    gtk::InputPurpose::FreeForm
                });
                self.entry.grab_focus();
            }
            None => self.entry.set_text(""),
        }

        match &state.message {
            Some(message) => {
                self.hint.set_text(&message.text);
                if message.kind == MessageKind::Error {
                    self.hint.add_css_class("error");
                } else {
                    self.hint.remove_css_class("error");
                }
            }
            None => {
                self.hint.set_text("");
                self.hint.remove_css_class("error");
            }
        }

        if state.shake {
            self.shake();
        }
    }

    /// Show the selected user.
    pub fn set_user(&self, user: &UserInfo) {
        self.name.set_text(&user.display_name);
        self.avatar.set_child(&avatar_content(user));
    }

    /// Fill the user row. The locker keeps the row hidden.
    pub fn set_users(&self, users: &[UserInfo], on_select: impl Fn(&UserInfo) + Clone + 'static) {
        while let Some(child) = self.users_row.first_child() {
            self.users_row.remove(&child);
        }

        for user in users {
            let button = gtk::Button::new();
            button.add_css_class("psldm-user-button");
            button.set_child(Some(&Avatar::new(
                &avatar_content(user),
                USER_ROW_AVATAR_SIZE,
            )));
            button.set_tooltip_text(Some(&user.display_name));

            let user = user.clone();
            let on_select = on_select.clone();
            button.connect_clicked(move |_| on_select(&user));
            self.users_row.append(&button);
        }
    }

    /// Fill the session list. The locker keeps the list hidden.
    pub fn set_sessions(&self, names: &[String]) {
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        self.sessions
            .set_model(Some(&gtk::StringList::new(&names)));
    }

    /// The name of the selected session, if the list has one.
    pub fn selected_session(&self) -> Option<String> {
        let model = self.sessions.model()?;
        let item = model.item(self.sessions.selected())?;
        item.downcast::<gtk::StringObject>()
            .ok()
            .map(|object| object.string().to_string())
    }

    /// Call `handler` when the user answers the prompt.
    pub fn connect_submit(&self, handler: impl Fn(String) + Clone + 'static) {
        let entry_handler = handler.clone();
        self.entry.connect_activate(move |entry| {
            let answer = entry.text().to_string();
            entry.set_text("");
            entry_handler(answer);
        });

        self.entry.connect_icon_release(move |entry, position| {
            if position == gtk::EntryIconPosition::Secondary {
                let answer = entry.text().to_string();
                entry.set_text("");
                handler(answer);
            }
        });
    }

    /// Add a power button. The greeter adds one button for each action.
    pub fn add_power_button(
        &self,
        action: PowerAction,
        handler: impl Fn(PowerAction) + 'static,
    ) {
        let (icon, tooltip) = match action {
            PowerAction::Restart => ("system-reboot-symbolic", "Restart"),
            PowerAction::Shutdown => ("system-shutdown-symbolic", "Shut Down"),
        };

        let button = gtk::Button::from_icon_name(icon);
        button.add_css_class("psldm-power-button");
        button.set_tooltip_text(Some(tooltip));
        button.connect_clicked(move |_| handler(action));
        self.power_row.append(&button);
    }

    /// Move the pane left and right one time.
    fn shake(&self) {
        if let Some(source) = self.shake_source.borrow_mut().take() {
            source.remove();
        }

        self.center.remove_css_class("psldm-shake");
        self.center.add_css_class("psldm-shake");

        let center = self.center.clone();
        let slot = Rc::clone(&self.shake_source);
        let source = glib::timeout_add_local_once(std::time::Duration::from_millis(460), move || {
            center.remove_css_class("psldm-shake");
            slot.replace(None);
        });
        self.shake_source.replace(Some(source));
    }
}

/// Build the clock and the date, and update them every second.
/// Add or remove one class.
fn set_css_class(widget: &impl IsA<gtk::Widget>, class: &str, present: bool) {
    if present {
        widget.add_css_class(class);
    } else {
        widget.remove_css_class(class);
    }
}

fn build_clock() -> gtk::Box {
    let time = gtk::Label::new(None);
    time.add_css_class("psldm-clock");
    let date = gtk::Label::new(None);
    date.add_css_class("psldm-date");

    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    column.add_css_class("psldm-clock-column");
    column.set_halign(gtk::Align::Center);
    column.set_valign(gtk::Align::Start);
    column.append(&time);
    column.append(&date);

    update_clock(&time, &date);
    let time_clone = time.clone();
    let date_clone = date.clone();
    glib::timeout_add_seconds_local(1, move || {
        update_clock(&time_clone, &date_clone);
        glib::ControlFlow::Continue
    });

    column
}

fn update_clock(time: &gtk::Label, date: &gtk::Label) {
    let Ok(now) = glib::DateTime::now_local() else {
        return;
    };

    // A 12 hour clock without a leading zero, as macOS shows it.
    let hour = match now.hour() % 12 {
        0 => 12,
        hour => hour,
    };
    time.set_text(&format!("{hour}:{:02}", now.minute()));

    let weekday = now.format("%A").unwrap_or_default();
    let month = now.format("%B").unwrap_or_default();
    date.set_text(&format!("{weekday}, {month} {}", now.day_of_month()));
}

/// The picture of the user, or the first letter of the name.
fn avatar_content(user: &UserInfo) -> gtk::Widget {
    match user.avatar.as_deref().filter(|path| path.exists()) {
        Some(path) => {
            let picture = gtk::Picture::for_filename(path);
            picture.set_content_fit(gtk::ContentFit::Cover);
            picture.upcast()
        }
        None => {
            let label = gtk::Label::new(Some(&user.initial()));
            label.add_css_class("psldm-avatar-initial");
            label.set_vexpand(true);
            label.upcast()
        }
    }
}
