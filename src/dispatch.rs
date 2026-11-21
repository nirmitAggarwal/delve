//! Thread-to-UI dispatch.
//!
//! Worker threads never touch GTK or `Rc<App>`. They post a closure that only
//! captures `Send` data; the closure is executed on the GTK main thread where a
//! reference to the application is provided.

use crate::app::App;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Mutex, OnceLock};
use std::rc::Rc;

pub type Task = Box<dyn FnOnce(&Rc<App>) + Send + 'static>;

fn sender_slot() -> &'static Mutex<Option<Sender<Task>>> {
    static SLOT: OnceLock<Mutex<Option<Sender<Task>>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

fn receiver_slot() -> &'static Mutex<Option<Receiver<Task>>> {
    static SLOT: OnceLock<Mutex<Option<Receiver<Task>>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// Install the pump on the main thread. Must be called from the GTK main thread.
pub fn install(app: &Rc<App>) {
    let (tx, rx) = channel::<Task>();
    *sender_slot().lock().unwrap() = Some(tx);
    *receiver_slot().lock().unwrap() = Some(rx);
    let weak = Rc::downgrade(app);
    gtk4::glib::timeout_add_local(std::time::Duration::from_millis(30), move || {
        let Some(app) = weak.upgrade() else {
            return gtk4::glib::ControlFlow::Break;
        };
        let tasks: Vec<Task> = {
            let guard = receiver_slot().lock().unwrap();
            match guard.as_ref() {
                Some(rx) => rx.try_iter().collect(),
                None => Vec::new(),
            }
        };
        for task in tasks {
            task(&app);
        }
        gtk4::glib::ControlFlow::Continue
    });
}

/// Queue work for the main thread. Safe to call from any thread.
pub fn post(task: Task) {
    if let Some(tx) = sender_slot().lock().unwrap().as_ref() {
        let _ = tx.send(task);
    }
}

/// Convenience for blocking worker threads that only need the result callback.
pub fn post_simple<F>(f: F)
where
    F: FnOnce(&Rc<App>) + Send + 'static,
{
    post(Box::new(f));
}
