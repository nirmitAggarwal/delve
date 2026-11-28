//! Delve — a powerful, adaptable Linux file manager.
//!
//! GTK4 + Rust. The application is split into UI (`app`, `views`, `inspector`,
//! `sidebar`, `dialogs`), pure filesystem logic (`fs`, `ops`), and persistent
//! state (`settings`, `metadata`, `theme`).

mod actions;
mod app;
mod dialogs;
mod dispatch;
mod fs;
mod inspector;
mod metadata;
mod model;
mod nav_impl;
mod ops;
mod paths;
mod runner;
mod settings;
mod shortcuts;
mod spotlight;
mod sidebar;
mod terminal;
mod theme;
mod thumbnails;
mod transfers;
mod util;
mod views;
mod volumes;

use gtk4 as gtk;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

thread_local! {
    static APP: RefCell<Option<Rc<app::App>>> = const { RefCell::new(None) };
}

fn main() -> gtk::glib::ExitCode {
    // The full icon theme is embedded in the executable, so no desktop icon
    // package, network access, or installed data directory is required.
    gtk::gio::resources_register_include!("delve.gresource")
        .expect("failed to register bundled Delve resources");

    let startup_path = std::env::args()
        .skip(1)
        .find(|arg| !arg.starts_with('-'))
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_dir());

    let application = gtk::Application::builder()
        .application_id("io.github.delve.FileManager")
        .flags(gtk::gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    application.connect_startup(|_| {
        gtk::Window::set_default_icon_name("io.github.delve.FileManager");
        crate::theme::init_bundled_icons();
        util::log("Delve starting");
    });

    application.connect_activate(move |application| {
        let existing = APP.with(|slot| slot.borrow().clone());
        if let Some(app) = existing {
            app.window.present();
            return;
        }
        let app = app::App::new(application.clone(), startup_path.clone());
        app.window.present();
        APP.with(|slot| *slot.borrow_mut() = Some(app));
    });

    application.connect_open(move |application, files, _hint| {
        let Some(path) = files.first().and_then(|f| f.path()) else { return };
        let location = if path.is_dir() {
            path
        } else {
            path.parent().map(|p| p.to_path_buf()).unwrap_or(path)
        };
        // With HANDLES_OPEN, GTK calls open instead of activate when Delve is
        // launched with a path, so a fresh instance must build its window here.
        let existing = APP.with(|slot| slot.borrow().clone());
        if let Some(app) = existing {
            app.navigate_active(crate::model::Location::Path(location));
            app.window.present();
            return;
        }
        let app = app::App::new(application.clone(), Some(location));
        app.window.present();
        APP.with(|slot| *slot.borrow_mut() = Some(app));
    });

    let code = application.run();
    APP.with(|slot| {
        slot.borrow_mut().take();
    });
    code
}
