//! Run-command process management and the integrated output panel.
//!
//! Commands are always spawned as argv vectors (never through a shell). Each
//! background process gets a process group so the whole tree can be terminated,
//! and stdout/stderr are streamed to the panel through the UI dispatcher.

use crate::app::App;
use crate::settings::{LaunchMode, RunCommand, RunMode, RunStep};
use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::io::{BufRead, BufReader};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Running,
    Done,
    Failed,
    Stopped,
    Terminal,
    Error,
}

impl Status {
    fn css(self) -> &'static str {
        match self {
            Status::Running => "warning",
            Status::Done => "success",
            Status::Failed | Status::Error => "danger",
            Status::Stopped => "dim",
            Status::Terminal => "accent",
        }
    }
}

struct Session {
    id: u64,
    profile: RunCommand,
    row: gtk::Box,
    status: RefCell<Status>,
    status_label: gtk::Label,
    output_label: gtk::Label,
    stop_btn: gtk::Button,
    child: RefCell<Option<Arc<Mutex<Child>>>>,
    output: Rc<RefCell<String>>,
    output_dirty: Rc<Cell<bool>>,
}

pub struct Runner {
    pub root: gtk::Revealer,
    list: gtk::Box,
    summary: gtk::Label,
    sessions: RefCell<Vec<Rc<Session>>>,
    next_id: Cell<u64>,
}

impl Runner {
    pub fn new(app: &Rc<App>) -> Rc<Runner> {
        let root = gtk::Revealer::new();
        root.set_transition_type(gtk::RevealerTransitionType::SlideUp);
        root.set_transition_duration(160);
        root.set_reveal_child(false);
        root.set_visible(true);

        let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        card.add_css_class("runner-panel");
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.set_margin_start(12);
        header.set_margin_end(10);
        header.set_margin_top(5);
        header.set_margin_bottom(4);
        let icon = gtk::Image::from_icon_name("system-run-symbolic");
        icon.set_pixel_size(15);
        header.append(&icon);
        let title = gtk::Label::new(Some("Run commands"));
        title.add_css_class("runner-title");
        header.append(&title);
        let summary = gtk::Label::new(Some("No processes"));
        summary.add_css_class("tiny");
        summary.add_css_class("dim");
        summary.set_margin_start(6);
        header.append(&summary);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        header.append(&spacer);
        let clear = gtk::Button::with_label("Clear finished");
        clear.add_css_class("flat");
        clear.add_css_class("tiny");
        header.append(&clear);
        let popout = gtk::Button::from_icon_name("window-new-symbolic");
        popout.add_css_class("flat");
        popout.set_tooltip_text(Some("Open command runs in a window"));
        header.append(&popout);
        let close = gtk::Button::from_icon_name("window-close-symbolic");
        close.add_css_class("flat");
        close.set_tooltip_text(Some("Hide run panel"));
        header.append(&close);
        card.append(&header);
        card.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let scroller = gtk::ScrolledWindow::builder()
            .child(&list)
            .vexpand(false)
            .propagate_natural_height(true)
            .max_content_height(250)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        card.append(&scroller);
        root.set_child(Some(&card));

        let runner = Rc::new(Runner {
            root,
            list,
            summary,
            sessions: RefCell::new(Vec::new()),
            next_id: Cell::new(1),
        });
        {
            let weak = Rc::downgrade(app);
            close.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.hide_runner();
                }
            });
        }
        {
            let weak = Rc::downgrade(app);
            let runner = runner.clone();
            clear.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    runner.clear_finished(&app);
                }
            });
        }
        {
            let weak = Rc::downgrade(app);
            popout.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.toggle_runner_popout();
                }
            });
        }
        runner
    }

    pub fn reveal(&self, show: bool) {
        self.root.set_reveal_child(show);
    }

    pub fn show_panel(&self) {
        self.root.set_reveal_child(true);
    }

    fn add_session(
        self: &Rc<Self>,
        app: &Rc<App>,
        profile: &RunCommand,
        step: &RunStep,
        folder: &PathBuf,
        selected: &[PathBuf],
        status: Status,
        detail: &str,
    ) -> u64 {
        let id = self.next_id.get();
        self.next_id.set(id + 1);

        let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
        row.add_css_class("runner-session");
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 7);
        head.set_margin_start(12);
        head.set_margin_end(8);
        head.set_margin_top(4);
        let dot = gtk::Label::new(Some("●"));
        dot.add_css_class("runner-dot");
        dot.add_css_class(status.css());
        head.append(&dot);
        let name = gtk::Label::new(Some(&format!("{} · {}", profile.name, crate::util::ellipsize(&step.display(), 58))));
        name.add_css_class("runner-command");
        name.set_xalign(0.0);
        name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        name.set_hexpand(true);
        head.append(&name);
        let status_label = gtk::Label::new(Some(detail));
        status_label.add_css_class("tiny");
        status_label.add_css_class(status.css());
        head.append(&status_label);

        let stop = gtk::Button::from_icon_name("process-stop-symbolic");
        stop.add_css_class("flat");
        stop.set_tooltip_text(Some("Stop this process"));
        stop.set_sensitive(status == Status::Running);
        head.append(&stop);
        let restart = gtk::Button::from_icon_name("view-refresh-symbolic");
        restart.add_css_class("flat");
        restart.set_tooltip_text(Some("Restart the profile"));
        head.append(&restart);
        row.append(&head);

        let output_label = gtk::Label::new(None);
        output_label.add_css_class("runner-output");
        output_label.set_xalign(0.0);
        output_label.set_yalign(0.0);
        output_label.set_wrap(true);
        output_label.set_selectable(true);
        output_label.set_hexpand(true);
        let out_scroll = gtk::ScrolledWindow::builder()
            .child(&output_label)
            .min_content_height(24)
            .max_content_height(150)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        out_scroll.add_css_class("runner-output-scroll");
        let expander = gtk::Expander::new(Some("Output"));
        expander.add_css_class("runner-expander");
        expander.set_child(Some(&out_scroll));
        expander.set_expanded(status == Status::Failed);
        expander.set_margin_start(22);
        expander.set_margin_end(10);
        expander.set_margin_bottom(4);
        row.append(&expander);

        let session = Rc::new(Session {
            id,
            profile: profile.clone(),
            row: row.clone(),
            status: RefCell::new(status),
            status_label,
            output_label,
            stop_btn: stop.clone(),
            child: RefCell::new(None),
            output: Rc::new(RefCell::new(String::new())),
            output_dirty: Rc::new(Cell::new(false)),
        });
        self.sessions.borrow_mut().push(session);
        self.list.append(&row);

        {
            let weak = Rc::downgrade(app);
            let runner = self.clone();
            let profile = profile.clone();
            let folder = folder.clone();
            let selected = selected.to_vec();
            restart.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    runner.start_profile(&app, profile.clone(), folder.clone(), selected.clone());
                }
            });
        }
        {
            let runner = self.clone();
            stop.connect_clicked(move |_| {
                runner.stop_session(id);
            });
        }
        self.update_summary();
        id
    }

    pub fn start_profile(
        self: &Rc<Self>,
        app: &Rc<App>,
        profile: RunCommand,
        folder: PathBuf,
        selected: Vec<PathBuf>,
    ) {
        let steps = profile.effective_steps();
        if steps.is_empty() {
            app.notify("This profile has no commands.", true);
            return;
        }
        self.show_panel();
        if profile.launch == LaunchMode::Terminal {
            self.start_terminal(app, &profile, &steps, &folder, &selected);
            return;
        }
        let mut ids = Vec::new();
        for (index, step) in steps.iter().enumerate() {
            let status = if index == 0 { Status::Running } else { Status::Stopped };
            let detail = if index == 0 { "Running" } else { "Waiting" };
            ids.push(self.add_session(app, &profile, step, &folder, &selected, status, detail));
        }
        self.schedule(profile, steps, ids, folder, selected);
    }

    fn schedule(
        self: &Rc<Self>,
        profile: RunCommand,
        steps: Vec<RunStep>,
        ids: Vec<u64>,
        folder: PathBuf,
        selected: Vec<PathBuf>,
    ) {
        // Sessions after the first are queued until their turn; the scheduler
        // promotes each one to Running when it starts.
        for (index, id) in ids.iter().enumerate() {
            if index > 0 {
                self.set_status(*id, Status::Stopped, "Queued");
            }
        }
        let mode = profile.mode;
        let stop_on_failure = profile.stop_on_failure;
        let profile_id = profile.id.clone();
        let pairs: Vec<(u64, RunStep)> = ids.into_iter().zip(steps).collect();

        std::thread::spawn(move || match mode {
            RunMode::Sequential => {
                for (id, step) in pairs {
                    crate::dispatch::post_simple(move |app| {
                        app.ui().runner.set_status(id, Status::Running, "Running");
                    });
                    match spawn_background(&profile, &step, id, &folder, &selected) {
                        Ok(rx) => {
                            let code = rx.recv().ok().flatten();
                            crate::dispatch::post_simple(move |app| {
                                app.ui().runner.finish(id, code);
                            });
                            if stop_on_failure && code != Some(0) {
                                let profile_id = profile_id.clone();
                                crate::dispatch::post_simple(move |app| {
                                    app.ui().runner.queued_after_failure(profile_id.clone());
                                });
                                break;
                            }
                        }
                        Err(err) => {
                            let err = err.clone();
                            crate::dispatch::post_simple(move |app| {
                                app.ui().runner.set_status(id, Status::Error, &err);
                            });
                            if stop_on_failure {
                                break;
                            }
                        }
                    }
                }
            }
            RunMode::Parallel => {
                let mut receivers = Vec::new();
                for (id, step) in pairs {
                    crate::dispatch::post_simple(move |app| {
                        app.ui().runner.set_status(id, Status::Running, "Running");
                    });
                    match spawn_background(&profile, &step, id, &folder, &selected) {
                        Ok(rx) => receivers.push((id, rx)),
                        Err(err) => {
                            let err = err.clone();
                            crate::dispatch::post_simple(move |app| {
                                app.ui().runner.set_status(id, Status::Error, &err);
                            });
                        }
                    }
                }
                for (id, rx) in receivers {
                    let code = rx.recv().ok().flatten();
                    crate::dispatch::post_simple(move |app| {
                        app.ui().runner.finish(id, code);
                    });
                }
            }
            RunMode::Mixed => {
                let mut groups: Vec<(u32, Vec<(u64, RunStep)>)> = Vec::new();
                for (id, step) in pairs {
                    match groups.iter_mut().find(|(g, _)| *g == step.group) {
                        Some((_, list)) => list.push((id, step)),
                        None => groups.push((step.group, vec![(id, step)])),
                    }
                }
                groups.sort_by_key(|(group, _)| *group);
                'groups: for (_, group) in groups {
                    let mut receivers = Vec::new();
                    for (id, step) in group {
                        crate::dispatch::post_simple(move |app| {
                            app.ui().runner.set_status(id, Status::Running, "Running");
                        });
                        match spawn_background(&profile, &step, id, &folder, &selected) {
                            Ok(rx) => receivers.push((id, rx)),
                            Err(err) => {
                                let err = err.clone();
                                crate::dispatch::post_simple(move |app| {
                                    app.ui().runner.set_status(id, Status::Error, &err);
                                });
                            }
                        }
                    }
                    let mut failed = false;
                    for (id, rx) in receivers {
                        let code = rx.recv().ok().flatten();
                        if stop_on_failure && code != Some(0) {
                            failed = true;
                        }
                        crate::dispatch::post_simple(move |app| {
                            app.ui().runner.finish(id, code);
                        });
                    }
                    if failed {
                        let profile_id = profile_id.clone();
                        crate::dispatch::post_simple(move |app| {
                            app.ui().runner.queued_after_failure(profile_id.clone());
                        });
                        break 'groups;
                    }
                }
            }
        });
    }

    fn start_terminal(
        self: &Rc<Self>,
        app: &Rc<App>,
        profile: &RunCommand,
        steps: &[RunStep],
        folder: &PathBuf,
        selected: &[PathBuf],
    ) {
        let mut tasks = Vec::new();
        for step in steps {
            let id = self.add_session(app, profile, step, folder, selected, Status::Terminal, "Opening terminal…");
            tasks.push((id, step.clone()));
        }
        let settings = app.settings.borrow().clone();
        let folder = folder.clone();
        let profile = profile.clone();
        let selected = selected.to_vec();
        let mode = profile.mode;
        std::thread::spawn(move || {
            let mut last_group = None;
            for (id, step) in tasks {
                let cmd = match crate::terminal::step_command(&profile, &step, &folder, &selected) {
                    Ok(cmd) => cmd,
                    Err(err) => {
                        crate::dispatch::post_simple(move |app| {
                            app.ui().runner.set_status(id, Status::Error, &err);
                        });
                        continue;
                    }
                };
                let program = cmd.get_program().to_string_lossy().to_string();
                let args: Vec<String> = cmd
                    .get_args()
                    .map(|a| a.to_string_lossy().to_string())
                    .collect();
                match crate::terminal::launch_program_in_terminal(
                    &settings,
                    &folder,
                    &program,
                    &args,
                    &profile.env,
                ) {
                    Ok(()) => {
                        crate::dispatch::post_simple(move |app| {
                            app.ui().runner.set_status(id, Status::Terminal, "Terminal opened");
                        });
                    }
                    Err(err) => {
                        crate::dispatch::post_simple(move |app| {
                            app.ui().runner.set_status(id, Status::Error, &err);
                        });
                    }
                }
                // Terminals cannot report completion. Preserve launch order for
                // sequential and mixed profiles without blocking the UI.
                let pause = mode != RunMode::Parallel
                    && (mode == RunMode::Sequential || last_group != Some(step.group));
                last_group = Some(step.group);
                if pause {
                    std::thread::sleep(Duration::from_millis(450));
                }
            }
        });
    }

    pub fn stop_session(&self, id: u64) {
        let session = self.sessions.borrow().iter().find(|s| s.id == id).cloned();
        if let Some(session) = session {
            let child = session.child.borrow().clone();
            self.set_status(id, Status::Stopped, "Stopped");
            if let Some(child) = child {
                kill_tree(&child);
            }
        }
    }

    fn finish(&self, id: u64, code: Option<i32>) {
        let stopped = self
            .sessions
            .borrow()
            .iter()
            .find(|s| s.id == id)
            .map(|s| *s.status.borrow() == Status::Stopped)
            .unwrap_or(false);
        if stopped {
            return;
        }
        if code == Some(0) {
            self.set_status(id, Status::Done, "Done");
        } else {
            let detail = match code {
                Some(code) => format!("Failed (exit {code})"),
                None => "Failed (terminated)".to_string(),
            };
            self.set_status(id, Status::Failed, &detail);
        }
    }

    fn queued_after_failure(&self, profile_id: String) {
        for session in self.sessions.borrow().iter() {
            if session.profile.id == profile_id && *session.status.borrow() == Status::Stopped {
                session.status_label.set_text("Skipped");
            }
        }
    }

    pub fn set_status(&self, id: u64, status: Status, detail: &str) {
        let session = self.sessions.borrow().iter().find(|s| s.id == id).cloned();
        if let Some(session) = session {
            *session.status.borrow_mut() = status;
            session.status_label.set_text(detail);
            for class in ["warning", "success", "danger", "dim", "accent"] {
                session.status_label.remove_css_class(class);
            }
            session.status_label.add_css_class(status.css());
            if let Some(dot) = session.row.first_child().and_then(|c| c.first_child()) {
                if let Some(dot) = dot.downcast_ref::<gtk::Label>() {
                    for class in ["warning", "success", "danger", "dim", "accent"] {
                        dot.remove_css_class(class);
                    }
                    dot.add_css_class(status.css());
                }
            }
            session.stop_btn.set_sensitive(status == Status::Running);
            self.update_summary();
        }
    }

    pub fn attach_child(&self, id: u64, child: Arc<Mutex<Child>>) {
        let session = self.sessions.borrow().iter().find(|s| s.id == id).cloned();
        if let Some(session) = session {
            *session.child.borrow_mut() = Some(child);
        }
    }

    pub fn append_output(&self, id: u64, text: &str) {
        let session = self.sessions.borrow().iter().find(|s| s.id == id).cloned();
        if let Some(session) = session {
            {
                let mut output = session.output.borrow_mut();
                output.push_str(text);
                if output.len() > 400_000 {
                    let cut = output.len() - 300_000;
                    let cut = output[cut..].find('\n').map(|i| cut + i + 1).unwrap_or(cut);
                    output.drain(..cut);
                }
            }
            // Coalesce high-volume output into at most one text relayout per
            // short interval so a noisy build cannot freeze the UI thread.
            if !session.output_dirty.replace(true) {
                let output = session.output.clone();
                let label = session.output_label.clone();
                let dirty = session.output_dirty.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(70), move || {
                    label.set_text(&output.borrow());
                    dirty.set(false);
                });
            }
        }
    }

    fn update_summary(&self) {
        let sessions = self.sessions.borrow();
        let running = sessions
            .iter()
            .filter(|s| *s.status.borrow() == Status::Running)
            .count();
        let failed = sessions
            .iter()
            .filter(|s| *s.status.borrow() == Status::Failed || *s.status.borrow() == Status::Error)
            .count();
        let total = sessions.len();
        if total == 0 {
            self.summary.set_text("No processes");
        } else {
            self.summary.set_text(&format!(
                "{total} process{} · {running} running{}",
                if total == 1 { "" } else { "es" },
                if failed > 0 { format!(" · {failed} failed") } else { String::new() }
            ));
        }
    }

    fn clear_finished(&self, app: &Rc<App>) {
        let keep: Vec<Rc<Session>> = self
            .sessions
            .borrow()
            .iter()
            .filter(|s| {
                matches!(
                    *s.status.borrow(),
                    Status::Running | Status::Terminal
                )
            })
            .cloned()
            .collect();
        for session in self.sessions.borrow().iter() {
            if !keep.iter().any(|k| k.id == session.id) {
                self.list.remove(&session.row);
            }
        }
        *self.sessions.borrow_mut() = keep;
        self.update_summary();
        let _ = app;
    }

    /// Terminate every managed process. Called when Delve is closing.
    pub fn shutdown(&self) {
        let children: Vec<Arc<Mutex<Child>>> = self
            .sessions
            .borrow()
            .iter()
            .filter_map(|s| s.child.borrow().clone())
            .collect();
        for child in children {
            kill_tree(&child);
        }
    }
}

fn spawn_background(
    profile: &RunCommand,
    step: &RunStep,
    id: u64,
    folder: &PathBuf,
    selected: &[PathBuf],
) -> Result<Receiver<Option<i32>>, String> {
    let mut cmd = crate::terminal::step_command(profile, step, folder, selected)?;
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // A dedicated process group lets Delve stop the process and any helpers it
    // spawns, rather than leaving orphans behind.
    cmd.process_group(0);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not run “{}”: {e}", step.program))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let child = Arc::new(Mutex::new(child));
    crate::dispatch::post_simple({
        let child = child.clone();
        move |app| app.ui().runner.attach_child(id, child)
    });
    if let Some(stdout) = stdout {
        stream_output(id, stdout);
    }
    if let Some(stderr) = stderr {
        stream_output(id, stderr);
    }
    let (tx, rx) = channel();
    let child_ref = child.clone();
    std::thread::spawn(move || {
        loop {
            let status = {
                let mut guard = match child_ref.lock() {
                    Ok(g) => g,
                    Err(_) => break,
                };
                match guard.try_wait() {
                    Ok(Some(status)) => Some(status),
                    Ok(None) => None,
                    Err(_) => break,
                }
            };
            if let Some(status) = status {
                let _ = tx.send(status.code());
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    });
    Ok(rx)
}

fn stream_output<R: std::io::Read + Send + 'static>(id: u64, pipe: R) {
    std::thread::spawn(move || {
        let reader = BufReader::new(pipe);
        let mut pending = String::new();
        for line in reader.lines() {
            let Ok(line) = line else { break };
            pending.push_str(&line);
            pending.push('\n');
            if !pending.is_empty() {
                let text = std::mem::take(&mut pending);
                crate::dispatch::post_simple(move |app| {
                    app.ui().runner.append_output(id, &text);
                });
            }
        }
        if !pending.is_empty() {
            crate::dispatch::post_simple(move |app| {
                app.ui().runner.append_output(id, &pending);
            });
        }
    });
}

fn kill_tree(child: &Arc<Mutex<Child>>) {
    let pid = match child.lock() {
        Ok(guard) => guard.id() as i32,
        Err(_) => return,
    };
    // Negative pid targets the process group created with process_group(0).
    unsafe {
        libc::kill(-pid, libc::SIGTERM);
        libc::kill(pid, libc::SIGTERM);
    }
    std::thread::sleep(Duration::from_millis(120));
    if let Ok(mut guard) = child.lock() {
        if matches!(guard.try_wait(), Ok(None)) {
            let _ = guard.kill();
        }
    }
}

/// Entry point used by menus, the editor and Spotlight.
pub fn start(app: &Rc<App>, profile: RunCommand, folder: PathBuf, selected: Vec<PathBuf>) {
    app.ui()
        .runner
        .start_profile(app, profile, folder, selected);
}

/// Start several profiles together. Each profile keeps its own execution
/// mode internally; the profiles themselves run concurrently.
pub fn start_many(
    app: &Rc<App>,
    profiles: Vec<RunCommand>,
    folder: PathBuf,
    selected: Vec<PathBuf>,
) {
    if profiles.is_empty() {
        app.notify("Choose at least one command set.", true);
        return;
    }
    // A batch is easier to follow in the detached output window. Opening it
    // before spawning also means the first output cannot land in a hidden
    // panel that the user did not notice.
    app.open_runner_popout();
    for profile in profiles {
        app.ui()
            .runner
            .start_profile(app, profile, folder.clone(), selected.clone());
    }
}

/// Display a profile's commands before execution. Every execution path funnels
/// through this confirmation so nothing runs implicitly.
pub fn confirm_and_start(
    app: &Rc<App>,
    profile: RunCommand,
    folder: PathBuf,
    selected: Vec<PathBuf>,
) {
    let steps = profile.effective_steps();
    let mut detail = String::new();
    for (index, step) in steps.iter().enumerate() {
        detail.push_str(&format!(
            "{}{}\n",
            if profile.mode == RunMode::Mixed {
                format!("[group {}] ", step.group + 1)
            } else {
                format!("{}. ", index + 1)
            },
            step.display()
        ));
    }
    detail.push_str(&format!(
        "\nMode: {} · {} · {}",
        profile.mode.label(),
        profile.launch.label(),
        profile.scope.label()
    ));
    if !profile.env.is_empty() {
        detail.push_str(&format!("\nEnvironment: {} variable(s)", profile.env.len()));
    }
    crate::dialogs::confirm(
        app,
        &format!("Run “{}”?", profile.name),
        &detail,
        "Run",
        false,
        move |app| {
            start(app, profile.clone(), folder.clone(), selected.clone());
        },
    );
}
