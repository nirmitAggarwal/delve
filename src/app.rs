//! Application shell: window, tabs, toolbar, panels, status bar and actions.

use crate::fs::Entry;
use crate::inspector::Inspector;
use crate::metadata::Metadata;
use crate::model::{Location, SearchFilters, TabState};
use crate::settings::{Density, Settings, ViewKind};
use crate::sidebar::Sidebar;
use crate::theme::ThemeManager;
use crate::views::Views;
use gtk4 as gtk;
use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

pub struct Tab {
    pub id: u64,
    pub state: RefCell<TabState>,
    pub views: Views,
    pub canvas: gtk::Box,
    pub banner: gtk::Box,
    pub banner_title: gtk::Label,
    pub banner_detail: gtk::Label,
}

impl Tab {
    pub fn new(app: &Rc<App>, id: u64, location: Location) -> Tab {
        let state = TabState::new(location, &app.settings.borrow());
        let views = Views::new(app, id);

        let banner = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        banner.add_css_class("banner");
        banner.set_margin_start(10);
        banner.set_margin_end(10);
        banner.set_margin_top(6);
        banner.set_visible(false);
        let banner_icon = gtk::Image::from_icon_name("dialog-information-symbolic");
        banner_icon.set_pixel_size(15);
        banner.append(&banner_icon);
        let text_box = gtk::Box::new(gtk::Orientation::Vertical, 1);
        text_box.set_hexpand(true);
        let banner_title = gtk::Label::new(None);
        banner_title.add_css_class("title");
        banner_title.set_xalign(0.0);
        banner_title.set_wrap(true);
        text_box.append(&banner_title);
        let banner_detail = gtk::Label::new(None);
        banner_detail.add_css_class("detail");
        banner_detail.set_xalign(0.0);
        banner_detail.set_wrap(true);
        banner_detail.set_visible(false);
        text_box.append(&banner_detail);
        banner.append(&text_box);
        let close = gtk::Button::from_icon_name("window-close-symbolic");
        close.add_css_class("flat");
        close.set_valign(gtk::Align::Start);
        let banner_ref = banner.clone();
        close.connect_clicked(move |_| banner_ref.set_visible(false));
        banner.append(&close);

        let canvas = gtk::Box::new(gtk::Orientation::Vertical, 0);
        canvas.append(&banner);
        canvas.append(&views.stack);
        canvas.set_vexpand(true);

        Tab { id, state: RefCell::new(state), views, canvas, banner, banner_title, banner_detail }
    }

    pub fn set_banner(&self, title: &str, detail: &str, error: bool) {
        self.banner_title.set_text(title);
        self.banner_detail.set_text(detail);
        self.banner_detail.set_visible(!detail.is_empty());
        if error {
            self.banner.add_css_class("error");
            self.banner.remove_css_class("warning");
        } else {
            self.banner.remove_css_class("error");
        }
        self.banner.set_visible(true);
    }

    pub fn clear_banner(&self) {
        self.banner.set_visible(false);
    }
}

#[derive(Clone)]
pub struct Clipboard {
    pub paths: Vec<PathBuf>,
    pub cut: bool,
    pub source_tab: u64,
}

pub struct DragState {
    pub paths: Vec<PathBuf>,
    pub source_tab: u64,
}

/// One reversible user operation. Paths are recorded as they were before the
/// operation so it can be replayed in either direction.
#[derive(Clone, Debug)]
pub enum UndoKind {
    /// Move pairs of (original, current) paths.
    Move { pairs: Vec<(PathBuf, PathBuf)> },
    /// Copied `created` roots, plus enough information to redo the copy.
    Copy {
        sources: Vec<PathBuf>,
        created: Vec<PathBuf>,
        dest_dir: PathBuf,
    },
    /// Items sent to the Trash; undo restores them.
    Trash { paths: Vec<PathBuf> },
}

#[derive(Clone, Debug)]
pub struct UndoRecord {
    pub label: String,
    pub kind: UndoKind,
}

pub struct Ui {
    pub shell: gtk::Box,
    pub tab_strip: gtk::Box,
    pub tab_plus: gtk::Button,
    pub canvas_stack: gtk::Stack,
    pub crumb: gtk::Box,
    pub crumb_scroller: gtk::ScrolledWindow,
    pub path_stack: gtk::Stack,
    pub location_entry: gtk::Entry,
    pub edit_location: gtk::Button,
    pub back: gtk::Button,
    pub forward: gtk::Button,
    pub up: gtk::Button,
    pub search: gtk::SearchEntry,
    pub search_btn: gtk::ToggleButton,
    pub search_revealer: gtk::Revealer,
    pub filter_btn: gtk::MenuButton,
    pub view_btn: gtk::MenuButton,
    pub view_btn_icon: gtk::Image,
    pub view_btn_label: gtk::Label,
    pub settings_btn: gtk::Button,
    pub more_btn: gtk::MenuButton,
    pub inspector_btn: gtk::ToggleButton,
    pub cmd_new: gtk::Button,
    pub cmd_undo: gtk::Button,
    pub cmd_redo: gtk::Button,
    pub cmd_cut: gtk::Button,
    pub cmd_copy: gtk::Button,
    pub cmd_paste: gtk::Button,
    pub cmd_rename: gtk::Button,
    pub cmd_trash: gtk::Button,
    pub cmd_hidden: gtk::Button,
    pub sidebar_widget: Sidebar,
    pub inspector_widget: Inspector,
    pub outer_paned: gtk::Paned,
    pub inner_paned: gtk::Paned,
    pub left_drawer: gtk::Revealer,
    pub right_drawer: gtk::Revealer,
    pub scrim: gtk::Box,
    pub drawer_left_box: gtk::Box,
    pub drawer_right_box: gtk::Box,
    pub counts: gtk::Label,
    pub selection_label: gtk::Label,
    pub path_label: gtk::Label,
    pub spinner: gtk::Spinner,
    pub status_progress: gtk::ProgressBar,
    pub cancel_btn: gtk::Button,
    pub narrow: Cell<bool>,
    pub syncing_view_buttons: Cell<bool>,
    pub sidebar_animating: Cell<bool>,
    pub window_title: gtk::Label,
    pub search_filters: RefCell<SearchFilters>,
    pub scroll_accum: Cell<f64>,
    pub last_width: Cell<i32>,
    pub columns_popover: gtk::Popover,
    pub runner: Rc<crate::runner::Runner>,
    /// Home for the runner widget when it is not in its detached window.
    pub runner_slot: gtk::Box,
}

pub struct App {
    pub gtk_app: gtk::Application,
    pub window: gtk::ApplicationWindow,
    pub settings: RefCell<Settings>,
    pub meta: RefCell<Metadata>,
    pub theme: Rc<ThemeManager>,
    pub tabs: RefCell<Vec<Rc<Tab>>>,
    pub active: Cell<usize>,
    pub next_tab_id: Cell<u64>,
    pub closed_tabs: RefCell<Vec<Location>>,
    pub clipboard: RefCell<Option<Clipboard>>,
    pub undo_stack: RefCell<Vec<UndoRecord>>,
    pub redo_stack: RefCell<Vec<UndoRecord>>,
    pub drag: RefCell<Option<DragState>>,
    pub thumbs: RefCell<HashMap<PathBuf, Option<PathBuf>>>,
    pub thumb_pending: RefCell<HashSet<PathBuf>>,
    pub ui: OnceCell<Ui>,
    pub theme_monitor: RefCell<Option<gio::FileMonitor>>,
    pub op_count: Cell<u32>,
    pub last_modifiers: Cell<gdk::ModifierType>,
    pub thumb_serial: Cell<u64>,
    /// Coalesces thumbnail repaints so a large folder does not queue one
    /// idle callback per generated preview.
    pub thumb_refresh_scheduled: Cell<bool>,
    /// Directory monitors for the active tab and its scan.
    pub watchers: RefCell<Vec<gio::FileMonitor>>,
    /// Coalesces bursts of filesystem events into one refresh.
    pub fs_pending: Cell<bool>,
    /// The detached command-output window, when open.
    pub runner_popout: RefCell<Option<gtk::Window>>,
    /// True while the Shift+S quick-run prefix is waiting for a digit.
    pub command_prefix: Cell<bool>,
    pub command_prefix_serial: Cell<u64>,
}

impl App {
    pub fn new(gtk_app: gtk::Application, startup_path: Option<PathBuf>) -> Rc<App> {
        crate::paths::ensure_dirs();
        let settings = Settings::load();
        let meta = Metadata::load();
        let mut saved_folder_colors: Vec<String> = meta
            .items
            .values()
            .filter_map(|item| item.folder_color.clone())
            .collect();
        saved_folder_colors.extend(meta.tags.iter().map(|tag| tag.color.clone()));
        saved_folder_colors.extend(
            settings
                .custom_folder_icons
                .keys()
                .filter(|key| *key != "default")
                .cloned(),
        );
        let display = gdk::Display::default().expect("Delve requires a graphical display");
        let theme = ThemeManager::new(&display, &settings);
        // Reload saved folder colours into the dynamic stylesheet on startup;
        // otherwise their tint classes have no definition after a restart.
        theme.apply_folder_colors(&saved_folder_colors, &settings);
        crate::theme::apply_icon_theme(&settings.icon_theme);

        let window = gtk::ApplicationWindow::builder()
            .application(&gtk_app)
            .title("Delve")
            .default_width(settings.window_width)
            .default_height(settings.window_height)
            .build();
        window.set_icon_name(Some("io.github.delve.FileManager"));
        window.add_css_class("delve");
        window.set_size_request(480, 340);
        if settings.window_maximized {
            window.maximize();
        }

        let app = Rc::new(App {
            gtk_app,
            window,
            settings: RefCell::new(settings),
            meta: RefCell::new(meta),
            theme,
            tabs: RefCell::new(Vec::new()),
            active: Cell::new(0),
            next_tab_id: Cell::new(1),
            closed_tabs: RefCell::new(Vec::new()),
            clipboard: RefCell::new(None),
            undo_stack: RefCell::new(Vec::new()),
            redo_stack: RefCell::new(Vec::new()),
            drag: RefCell::new(None),
            thumbs: RefCell::new(HashMap::new()),
            thumb_pending: RefCell::new(HashSet::new()),
            ui: OnceCell::new(),
            theme_monitor: RefCell::new(None),
            op_count: Cell::new(0),
            last_modifiers: Cell::new(gdk::ModifierType::empty()),
            thumb_serial: Cell::new(0),
            thumb_refresh_scheduled: Cell::new(false),
            watchers: RefCell::new(Vec::new()),
            fs_pending: Cell::new(false),
            runner_popout: RefCell::new(None),
            command_prefix: Cell::new(false),
            command_prefix_serial: Cell::new(0),
        });
        app.build_ui();
        app.apply_window_opacity();
        crate::dispatch::install(&app);
        app.install_actions();
        app.install_shortcut_tracker();
        app.restart_theme_monitor();

        let explicit_path = startup_path.is_some();
        let initial = startup_path.unwrap_or_else(crate::paths::home);
        let initial = if initial.is_dir() {
            initial
        } else {
            crate::paths::home()
        };
        let initial = if explicit_path {
            Location::Path(initial)
        } else {
            Location::Home
        };
        app.open_initial(initial);
        app
    }

    pub fn ui(&self) -> &Ui {
        self.ui.get().expect("UI built during App::new")
    }

    /// Apply background opacity live. This deliberately resets whole-widget
    /// opacity and lets the theme override only surface backgrounds, so text,
    /// icons and hover/selection states stay fully opaque.
    pub fn apply_window_opacity(&self) {
        self.window.set_opacity(1.0);
        let settings = self.settings.borrow().clone();
        self.theme.apply_overrides(&settings);
    }

    /// Rebuild the visible entry views after icon rules change. Custom icons
    /// also suppress thumbnails, so a full sync (not just a repaint) is needed.
    /// Reusing the metadata refresh path keeps colour swatches for new folder
    /// icon rules defined in the dynamic stylesheet.
    pub fn refresh_after_icon_change(self: &Rc<Self>) {
        self.refresh_after_metadata_change();
    }

    /// Whether a text control currently owns the keyboard. The Shift+S prefix
    /// must never steal a capital S from a search field, path entry or editor.
    fn text_input_focused(&self) -> bool {
        let Some(focus) = gtk::prelude::GtkWindowExt::focus(&self.window) else {
            return false;
        };
        // GtkSearchEntry and some other composite controls put focus on an
        // internal child, so inspect the whole ancestor chain.
        let mut widget = Some(focus);
        while let Some(current) = widget {
            if current.is::<gtk::Entry>()
                || current.is::<gtk::SearchEntry>()
                || current.is::<gtk::TextView>()
                || current.is::<gtk::Text>()
                || current.is::<gtk::SpinButton>()
            {
                return true;
            }
            widget = current.parent();
        }
        false
    }

    pub fn active_tab(&self) -> Option<Rc<Tab>> {
        let index = self.active.get();
        self.tabs.borrow().get(index).cloned()
    }

    pub fn active_location(&self) -> Location {
        self.active_tab()
            .map(|t| t.state.borrow().location.clone())
            .unwrap_or(Location::Path(crate::paths::home()))
    }

    pub fn tab_by_id(&self, id: u64) -> Option<Rc<Tab>> {
        self.tabs.borrow().iter().find(|t| t.id == id).cloned()
    }

    pub fn tab_index(&self, id: u64) -> Option<usize> {
        self.tabs.borrow().iter().position(|t| t.id == id)
    }

    pub fn tab_selection(&self, id: u64) -> HashSet<PathBuf> {
        self.tab_by_id(id)
            .map(|t| t.state.borrow().selection.clone())
            .unwrap_or_default()
    }

    // ---------------------------------------------------------------------
    // UI construction
    // ---------------------------------------------------------------------

    fn build_ui(self: &Rc<Self>) {
        let ui_settings = self.settings.borrow().clone();
        let shell = gtk::Box::new(gtk::Orientation::Vertical, 0);
        shell.add_css_class("delve-shell");
        shell.set_vexpand(true);
        let runner = crate::runner::Runner::new(self);

        // --- Tab bar -------------------------------------------------------
        let tab_strip = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        tab_strip.set_margin_start(4);
        tab_strip.set_margin_top(4);
        tab_strip.set_valign(gtk::Align::End);
        let tab_scroller = gtk::ScrolledWindow::builder()
            .child(&tab_strip)
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .hexpand(true)
            .build();
        tab_scroller.add_css_class("delve-tabbar-scrolled");
        let tab_plus = gtk::Button::from_icon_name("tab-new-symbolic");
        tab_plus.add_css_class("delve-tab-add");
        tab_plus.set_tooltip_text(Some("New tab (Ctrl+T)"));
        {
            let weak = Rc::downgrade(self);
            tab_plus.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    let loc = app.active_location();
                    let loc = match loc {
                        Location::Path(p) => Location::Path(p),
                        _ => Location::Path(crate::paths::home()),
                    };
                    app.new_tab(loc);
                }
            });
        }
        let tab_bar = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        tab_bar.add_css_class("delve-tabbar");
        tab_bar.append(&tab_scroller);
        tab_bar.append(&tab_plus);
        shell.append(&tab_bar);

        // --- Toolbar -------------------------------------------------------
        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        toolbar.add_css_class("delve-navbar");
        toolbar.set_margin_start(8);
        toolbar.set_margin_end(8);
        toolbar.set_margin_top(4);
        toolbar.set_margin_bottom(4);

        // The preview toggle sits next to Settings on the right of the command
        // bar. The sidebar itself is always present, so it needs no toggle.
        let inspector_btn = gtk::ToggleButton::new();
        inspector_btn.set_icon_name("sidebar-show-right-symbolic");
        inspector_btn.add_css_class("toolbar-button");
        inspector_btn.set_tooltip_text(Some("Toggle preview pane (F10)"));
        inspector_btn.set_active(ui_settings.inspector_visible);

        let back = nav_button("go-previous-symbolic", "Back (Alt+Left)");
        let forward = nav_button("go-next-symbolic", "Forward (Alt+Right)");
        let up = nav_button("go-up-symbolic", "Parent folder (Alt+Up)");
        toolbar.append(&back);
        toolbar.append(&forward);
        toolbar.append(&up);
        toolbar.append(&gtk::Separator::new(gtk::Orientation::Vertical));

        let crumb = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        crumb.set_margin_start(4);
        let crumb_scroller = gtk::ScrolledWindow::builder()
            .child(&crumb)
            .hscrollbar_policy(gtk::PolicyType::External)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .hexpand(true)
            .build();
        let edit_location = gtk::Button::from_icon_name("document-edit-symbolic");
        edit_location.add_css_class("flat");
        edit_location.set_tooltip_text(Some("Type a location (Ctrl+L)"));
        let location_entry = gtk::Entry::new();
        location_entry.set_placeholder_text(Some("Type a path, ~ is home"));
        location_entry.add_css_class("delve-entry");
        location_entry.set_hexpand(true);
        let entry_page = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        entry_page.append(&location_entry);
        let path_stack = gtk::Stack::new();
        path_stack.set_hexpand(true);
        path_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        path_stack.set_transition_duration(80);
        path_stack.add_named(&crumb_scroller, Some("crumbs"));
        path_stack.add_named(&entry_page, Some("entry"));
        let path_box = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        path_box.add_css_class("location-bar");
        path_box.set_hexpand(true);
        path_box.append(&path_stack);
        path_box.append(&edit_location);
        toolbar.append(&path_box);
        toolbar.append(&gtk::Separator::new(gtk::Orientation::Vertical));

        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some("Filter this folder…"));
        search.set_width_chars(20);
        search.set_max_width_chars(30);
        let search_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        search_box.add_css_class("search-pill");
        search_box.set_margin_start(2);
        search_box.append(&search);
        let search_revealer = gtk::Revealer::new();
        search_revealer.set_transition_type(gtk::RevealerTransitionType::SlideLeft);
        search_revealer.set_transition_duration(160);
        search_revealer.set_reveal_child(false);
        search_revealer.set_child(Some(&search_box));
        let search_btn = gtk::ToggleButton::new();
        search_btn.set_icon_name("system-search-symbolic");
        search_btn.add_css_class("toolbar-button");
        search_btn.set_tooltip_text(Some("Search this folder (Ctrl+F)"));
        toolbar.append(&search_btn);
        toolbar.append(&search_revealer);
        let filter_btn = gtk::MenuButton::new();
        filter_btn.set_icon_name("funnel-symbolic");
        filter_btn.add_css_class("flat");
        filter_btn.set_tooltip_text(Some("Search scope and filters"));
        toolbar.append(&filter_btn);

        // The location bar takes every pixel the search field is not using.
        path_box.set_hexpand(true);
        shell.append(&toolbar);

        // --- Command bar ---------------------------------------------------
        let commandbar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        commandbar.add_css_class("delve-commandbar");
        commandbar.set_margin_start(10);
        commandbar.set_margin_end(10);
        commandbar.set_margin_top(2);
        commandbar.set_margin_bottom(6);
        commandbar.append(&command_button("folder-new-symbolic", "New folder (Ctrl+Shift+N)"));
        let cmd_new = commandbar.last_child().unwrap().downcast::<gtk::Button>().unwrap();
        commandbar.append(&command_button("edit-undo-symbolic", "Undo (Ctrl+Z)"));
        let cmd_undo = commandbar.last_child().unwrap().downcast::<gtk::Button>().unwrap();
        commandbar.append(&command_button("edit-redo-symbolic", "Redo (Ctrl+Shift+Z)"));
        let cmd_redo = commandbar.last_child().unwrap().downcast::<gtk::Button>().unwrap();
        commandbar.append(&gtk::Separator::new(gtk::Orientation::Vertical));
        commandbar.append(&command_button("edit-cut-symbolic", "Cut (Ctrl+X)"));
        let cmd_cut = commandbar.last_child().unwrap().downcast::<gtk::Button>().unwrap();
        commandbar.append(&command_button("edit-copy-symbolic", "Copy (Ctrl+C)"));
        let cmd_copy = commandbar.last_child().unwrap().downcast::<gtk::Button>().unwrap();
        commandbar.append(&command_button("edit-paste-symbolic", "Paste (Ctrl+V)"));
        let cmd_paste = commandbar.last_child().unwrap().downcast::<gtk::Button>().unwrap();
        commandbar.append(&command_button("document-edit-symbolic", "Rename (F2)"));
        let cmd_rename = commandbar.last_child().unwrap().downcast::<gtk::Button>().unwrap();
        commandbar.append(&command_button("user-trash-symbolic", "Move to Trash (Delete)"));
        let cmd_trash = commandbar.last_child().unwrap().downcast::<gtk::Button>().unwrap();
        let cmd_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        cmd_spacer.set_hexpand(true);
        commandbar.append(&cmd_spacer);

        // Single view switcher: shows the active view's icon and name.
        let view_btn = gtk::MenuButton::new();
        view_btn.add_css_class("view-menu-button");
        view_btn.set_tooltip_text(Some("Switch view"));
        let view_body = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let view_btn_icon = gtk::Image::from_icon_name(ui_settings.default_view.icon_name());
        view_btn_icon.set_pixel_size(15);
        view_body.append(&view_btn_icon);
        let view_btn_label = gtk::Label::new(Some(ui_settings.default_view.label()));
        view_btn_label.add_css_class("view-menu-label");
        view_body.append(&view_btn_label);
        let chevron = gtk::Image::from_icon_name("pan-down-symbolic");
        chevron.set_pixel_size(12);
        chevron.add_css_class("dim");
        view_body.append(&chevron);
        view_btn.set_child(Some(&view_body));
        let view_popover = gtk::Popover::new();
        view_popover.add_css_class("view-menu");
        view_popover.set_has_arrow(false);
        let view_list = gtk::Box::new(gtk::Orientation::Vertical, 2);
        view_list.set_width_request(190);
        for kind in ViewKind::all() {
            let row = gtk::Button::new();
            row.add_css_class("flat");
            row.add_css_class("view-menu-row");
            row.set_halign(gtk::Align::Fill);
            let body = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let img = gtk::Image::from_icon_name(kind.icon_name());
            img.set_pixel_size(15);
            body.append(&img);
            let text = gtk::Label::new(Some(kind.label()));
            text.set_xalign(0.0);
            text.set_hexpand(true);
            body.append(&text);
            let hint = gtk::Label::new(Some(match kind {
                ViewKind::Icon => "Ctrl+1",
                ViewKind::Detail => "Ctrl+2",
                ViewKind::Recursive => "Ctrl+3",
                ViewKind::Treemap => "Ctrl+4",
                ViewKind::Graph => "Ctrl+5",
            }));
            hint.add_css_class("tiny");
            hint.add_css_class("dim");
            body.append(&hint);
            row.set_child(Some(&body));
            let weak = Rc::downgrade(self);
            let pop = view_popover.clone();
            row.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.select_view(kind);
                }
                pop.popdown();
            });
            view_list.append(&row);
        }
        view_popover.set_child(Some(&view_list));
        view_btn.set_popover(Some(&view_popover));
        commandbar.append(&view_btn);
        // Open the configured terminal in the current folder, right next to the
        // view switcher.
        let runs_btn = gtk::Button::from_icon_name("system-run-symbolic");
        runs_btn.add_css_class("toolbar-button");
        runs_btn.set_tooltip_text(Some("Run commands panel (Ctrl+Shift+R)"));
        {
            let runner = runner.clone();
            runs_btn.connect_clicked(move |_| {
                runner.reveal(!runner.root.reveals_child());
            });
        }
        commandbar.append(&runs_btn);
        let command_sets_btn = gtk::Button::from_icon_name("view-list-symbolic");
        command_sets_btn.add_css_class("toolbar-button");
        command_sets_btn.set_tooltip_text(Some("Choose command sets (Shift+S, 0)"));
        {
            let weak = Rc::downgrade(self);
            command_sets_btn.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    if let Some(folder) = app.active_location().path().cloned() {
                        crate::dialogs::run_commands_picker(
                            &app,
                            folder,
                            app.selected_paths(),
                        );
                    } else {
                        app.notify("Open a folder before running command sets.", true);
                    }
                }
            });
        }
        commandbar.append(&command_sets_btn);
        let terminal_btn = gtk::Button::from_icon_name("utilities-terminal-symbolic");
        terminal_btn.add_css_class("toolbar-button");
        terminal_btn.set_tooltip_text(Some("Open terminal here (Ctrl+Shift+E)"));
        {
            let weak = Rc::downgrade(self);
            terminal_btn.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.open_terminal();
                }
            });
        }
        commandbar.append(&terminal_btn);
        // Hidden files: an eye that becomes a crossed-out eye when hidden
        // entries are being shown.
        let hidden_btn = gtk::Button::from_icon_name("view-conceal-symbolic");
        hidden_btn.add_css_class("toolbar-button");
        hidden_btn.set_tooltip_text(Some("Show hidden files (Ctrl+H)"));
        {
            let weak = Rc::downgrade(self);
            hidden_btn.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.toggle_hidden();
                }
            });
        }
        commandbar.append(&hidden_btn);
        commandbar.append(&inspector_btn);
        let settings_btn = gtk::Button::from_icon_name("emblem-system-symbolic");
        settings_btn.add_css_class("toolbar-button");
        settings_btn.set_tooltip_text(Some("Settings"));
        {
            let weak = Rc::downgrade(self);
            settings_btn.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.open_settings(None);
                }
            });
        }
        commandbar.append(&settings_btn);
        let more_btn = gtk::MenuButton::new();
        more_btn.set_icon_name("open-menu-symbolic");
        more_btn.add_css_class("flat");
        more_btn.set_tooltip_text(Some("More actions"));
        commandbar.append(&more_btn);
        shell.append(&commandbar);

        // --- Panels --------------------------------------------------------
        let sidebar_widget = Sidebar::new(self);
        let inspector_widget = Inspector::new(self);
        let canvas_stack = gtk::Stack::new();
        canvas_stack.set_hexpand(true);
        canvas_stack.set_vexpand(true);
        canvas_stack.set_transition_type(gtk::StackTransitionType::None);
        canvas_stack.add_css_class("delve-canvas");

        let inner_paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        inner_paned.set_start_child(Some(&canvas_stack));
        inner_paned.set_end_child(Some(&inspector_widget.root));
        inner_paned.set_resize_start_child(true);
        inner_paned.set_shrink_end_child(false);
        inner_paned.set_position(-1);
        inner_paned.set_vexpand(true);

        let outer_paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        outer_paned.set_start_child(Some(&sidebar_widget.root));
        outer_paned.set_end_child(Some(&inner_paned));
        outer_paned.set_resize_start_child(false);
        outer_paned.set_shrink_start_child(false);
        outer_paned.set_vexpand(true);
        outer_paned.set_position(ui_settings.sidebar_width);
        outer_paned.set_vexpand(true);
        shell.append(&outer_paned);
        let runner_slot = gtk::Box::new(gtk::Orientation::Vertical, 0);
        runner_slot.append(&runner.root);
        shell.append(&runner_slot);

        // --- Drawers (narrow layout) --------------------------------------
        let drawer_left_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let left_drawer = gtk::Revealer::new();
        left_drawer.set_transition_type(gtk::RevealerTransitionType::SlideRight);
        left_drawer.set_transition_duration(160);
        left_drawer.set_halign(gtk::Align::Start);
        left_drawer.set_valign(gtk::Align::Fill);
        left_drawer.set_visible(false);
        let drawer_right_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let right_drawer = gtk::Revealer::new();
        right_drawer.set_transition_type(gtk::RevealerTransitionType::SlideLeft);
        right_drawer.set_transition_duration(160);
        right_drawer.set_halign(gtk::Align::End);
        right_drawer.set_valign(gtk::Align::Fill);
        right_drawer.set_visible(false);
        let scrim = gtk::Box::new(gtk::Orientation::Vertical, 0);
        scrim.add_css_class("drawer-scrim");
        scrim.set_visible(false);
        scrim.set_hexpand(true);
        scrim.set_vexpand(true);

        // --- Status bar ----------------------------------------------------
        let status = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        status.add_css_class("delve-statusbar");
        status.set_margin_start(12);
        status.set_margin_end(12);
        status.set_margin_top(2);
        status.set_margin_bottom(2);
        let counts = gtk::Label::new(Some("0 items"));
        counts.add_css_class("status-text");
        status.append(&counts);
        let selection_label = gtk::Label::new(None);
        selection_label.add_css_class("status-text");
        status.append(&selection_label);
        let status_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        status_spacer.set_hexpand(true);
        status.append(&status_spacer);
        let spinner = gtk::Spinner::new();
        spinner.add_css_class("busy-spinner");
        spinner.set_visible(false);
        status.append(&spinner);
        let status_progress = gtk::ProgressBar::new();
        status_progress.add_css_class("op-progress");
        status_progress.set_show_text(false);
        status_progress.set_visible(false);
        status_progress.set_size_request(110, -1);
        status.append(&status_progress);
        let cancel_btn = gtk::Button::from_icon_name("process-stop-symbolic");
        cancel_btn.add_css_class("flat");
        cancel_btn.set_tooltip_text(Some("Cancel the running scan"));
        cancel_btn.set_visible(false);
        status.append(&cancel_btn);
        let path_label = gtk::Label::new(None);
        path_label.add_css_class("status-path");
        path_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        path_label.set_max_width_chars(60);
        path_label.set_selectable(true);
        status.append(&path_label);
        shell.append(&status);

        // --- Assemble with overlay drawers ---------------------------------
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&shell));
        overlay.add_overlay(&scrim);
        let left_wrap = gtk::Box::new(gtk::Orientation::Vertical, 0);
        left_wrap.add_css_class("drawer");
        left_wrap.set_margin_top(8);
        left_wrap.set_margin_bottom(8);
        left_wrap.set_margin_start(8);
        left_wrap.set_width_request(280);
        left_wrap.set_vexpand(true);
        left_wrap.append(&drawer_left_box);
        left_drawer.set_child(Some(&left_wrap));
        let right_wrap = gtk::Box::new(gtk::Orientation::Vertical, 0);
        right_wrap.add_css_class("drawer");
        right_wrap.set_margin_top(8);
        right_wrap.set_margin_bottom(8);
        right_wrap.set_margin_end(8);
        right_wrap.set_width_request(300);
        right_wrap.set_vexpand(true);
        right_wrap.append(&drawer_right_box);
        right_drawer.set_child(Some(&right_wrap));
        overlay.add_overlay(&left_drawer);
        overlay.add_overlay(&right_drawer);
        overlay.set_vexpand(true);
        self.window.set_child(Some(&overlay));

        // --- Signals -------------------------------------------------------
        let ui = Ui {
            shell,
            tab_strip,
            tab_plus,
            canvas_stack,
            crumb,
            crumb_scroller,
            path_stack,
            location_entry,
            edit_location,
            back,
            forward,
            up,
            search,
            search_btn,
            search_revealer,
            filter_btn,
            view_btn,
            view_btn_icon,
            view_btn_label,
            settings_btn,
            more_btn,
            inspector_btn,
            cmd_new,
            cmd_undo,
            cmd_redo,
            cmd_cut,
            cmd_copy,
            cmd_paste,
            cmd_rename,
            cmd_trash,
            cmd_hidden: hidden_btn,
            sidebar_widget,
            inspector_widget,
            outer_paned,
            inner_paned,
            left_drawer,
            right_drawer,
            scrim,
            drawer_left_box,
            drawer_right_box,
            counts,
            selection_label,
            path_label,
            spinner,
            status_progress,
            cancel_btn,
            narrow: Cell::new(false),
            syncing_view_buttons: Cell::new(false),
            sidebar_animating: Cell::new(false),
            window_title: gtk::Label::new(None),
            search_filters: RefCell::new(SearchFilters::default()),
            scroll_accum: Cell::new(0.0),
            last_width: Cell::new(0),
            columns_popover: gtk::Popover::new(),
            runner: runner.clone(),
            runner_slot,
        };
        let _ = self.ui.set(ui);
        self.install_ui_signals();
        self.apply_density();
        self.install_responsive();
        self.install_window_state();
    }

    fn install_ui_signals(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        let back = self.ui().back.clone();
        back.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                app.go_back();
            }
        });
        let weak = Rc::downgrade(self);
        let forward = self.ui().forward.clone();
        forward.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                app.go_forward();
            }
        });
        let weak = Rc::downgrade(self);
        let up = self.ui().up.clone();
        up.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                app.go_up();
            }
        });
        let weak = Rc::downgrade(self);
        let edit = self.ui().edit_location.clone();
        edit.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                app.toggle_location_entry(true);
            }
        });
        {
            let weak = Rc::downgrade(self);
            let btn = self.ui().cmd_new.clone();
            btn.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.create_folder_or_file(true);
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            let btn = self.ui().cmd_undo.clone();
            btn.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.undo();
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            let btn = self.ui().cmd_redo.clone();
            btn.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.redo();
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            let btn = self.ui().cmd_cut.clone();
            btn.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.copy_selection_to_clipboard(true);
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            let btn = self.ui().cmd_copy.clone();
            btn.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.copy_selection_to_clipboard(false);
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            let btn = self.ui().cmd_paste.clone();
            btn.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.paste_into_current();
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            let btn = self.ui().cmd_rename.clone();
            btn.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.rename_selection();
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            let btn = self.ui().cmd_trash.clone();
            btn.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.delete_selection(false);
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            let entry = self.ui().location_entry.clone();
            entry.connect_activate(move |e| {
                if let Some(app) = weak.upgrade() {
                    let text = e.text().to_string();
                    app.toggle_location_entry(false);
                    if !text.trim().is_empty() {
                        let path = crate::paths::expand_user_path(&text);
                        app.navigate_active(Location::Path(path));
                    }
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            let search = self.ui().search.clone();
            search.connect_search_changed(move |s| {
                if let Some(app) = weak.upgrade() {
                    app.on_filter_changed(&s.text());
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            let search = self.ui().search.clone();
            let filter_btn = self.ui().filter_btn.clone();
            search.connect_activate(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.start_recursive_search_from_filter();
                    let _ = &filter_btn;
                }
            });
        }
        // Animated search: a single toggle button expands the field.
        {
            let revealer = self.ui().search_revealer.clone();
            let search = self.ui().search.clone();
            let btn_ref = self.ui().search_btn.clone();
            self.ui().search_btn.connect_toggled(move |b| {
                revealer.set_reveal_child(b.is_active());
                if b.is_active() {
                    let search = search.clone();
                    glib::timeout_add_local_once(std::time::Duration::from_millis(80), move || {
                        search.grab_focus();
                    });
                } else if !search.text().is_empty() {
                    search.set_text("");
                }
                let _ = &btn_ref;
            });
        }
        {
            let search = self.ui().search.clone();
            let btn = self.ui().search_btn.clone();
            let key = gtk::EventControllerKey::new();
            key.connect_key_pressed(move |_, keyval, _, _| {
                if keyval == gdk::Key::Escape {
                    btn.set_active(false);
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            });
            search.add_controller(key);
        }
        {
            let weak = Rc::downgrade(self);
            let btn = self.ui().inspector_btn.clone();
            btn.connect_toggled(move |b| {
                if let Some(app) = weak.upgrade() {
                    app.set_inspector_visible(b.is_active());
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            let scrim = self.ui().scrim.clone();
            let gesture = gtk::GestureClick::new();
            gesture.connect_released(move |_, _, _, _| {
                if let Some(app) = weak.upgrade() {
                    app.close_drawers();
                }
            });
            scrim.add_controller(gesture);
        }
        // More popover
        {
            let popover = gtk::Popover::new();
            self.ui().more_btn.set_popover(Some(&popover));
            let weak = Rc::downgrade(self);
            popover.connect_show(move |p| {
                if let Some(app) = weak.upgrade() {
                    app.fill_more_popover(p);
                }
            });
        }
        // Filter/scope popover
        {
            let popover = gtk::Popover::new();
            self.ui().filter_btn.set_popover(Some(&popover));
            let weak = Rc::downgrade(self);
            popover.connect_show(move |p| {
                if let Some(app) = weak.upgrade() {
                    app.fill_search_popover(p);
                }
            });
        }
        // Paned widths
        {
            let weak = Rc::downgrade(self);
            self.ui().outer_paned.connect_position_notify(move |p| {
                if let Some(app) = weak.upgrade() {
                    if !app.ui().narrow.get() && !app.ui().sidebar_animating.get() {
                        app.settings.borrow_mut().sidebar_width = p.position().max(150);
                    }
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            let inner = self.ui().inner_paned.clone();
            inner.connect_position_notify(move |p| {
                if let Some(app) = weak.upgrade() {
                    if !app.ui().narrow.get() {
                        let width = p.width();
                        let pos = p.position();
                        if width > 0 {
                            app.settings.borrow_mut().inspector_width = (width - pos).max(200);
                        }
                    }
                }
            });
        }
        // Empty-area drop on the canvas stack
        self.install_canvas_drop();
    }

    fn install_window_state(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.window.connect_close_request(move |w| {
            if let Some(app) = weak.upgrade() {
                {
                    let mut settings = app.settings.borrow_mut();
                    settings.window_maximized = w.is_maximized();
                    if !w.is_maximized() {
                        settings.window_width = w.width().max(480);
                        settings.window_height = w.height().max(360);
                    }
                    if !app.ui().narrow.get() && !settings.sidebar_mini {
                        settings.sidebar_width = app.ui().outer_paned.position();
                    }
                    settings.save();
                }
                app.meta.borrow().save();
                app.ui().runner.shutdown();
            }
            glib::Propagation::Proceed
        });
    }

    fn install_responsive(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.window.add_tick_callback(move |window, _| {
            let Some(app) = weak.upgrade() else { return glib::ControlFlow::Break };
            let width = window.width();
            if width != app.ui().last_width.get() {
                app.ui().last_width.set(width);
                app.apply_responsive(width);
            }
            glib::ControlFlow::Continue
        });
        // The first ticks can run before the paned widgets receive their final
        // allocation; run one explicit pass after the window is mapped.
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(std::time::Duration::from_millis(250), move || {
            if let Some(app) = weak.upgrade() {
                app.apply_responsive(app.window.width());
            }
        });
    }

    fn install_shortcut_tracker(self: &Rc<Self>) {
        let key = gtk::EventControllerKey::new();
        key.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        key.connect_key_pressed(move |_, keyval, _, state| {
            if let Some(app) = weak.upgrade() {
                app.last_modifiers.set(state);
                if keyval == gdk::Key::Escape {
                    app.command_prefix.set(false);
                    return app.handle_escape();
                }
                // Shift+S is a prefix, not a normal accelerator: after it, a
                // digit starts the matching command set (0 opens the picker).
                if app.command_prefix.get() {
                    if let Some(slot) = command_shortcut_digit(keyval) {
                        app.command_prefix.set(false);
                        app.run_command_slot(slot);
                        return glib::Propagation::Stop;
                    }
                    app.command_prefix.set(false);
                }
                let blocking_modifiers = gdk::ModifierType::CONTROL_MASK
                    | gdk::ModifierType::ALT_MASK
                    | gdk::ModifierType::SUPER_MASK;
                if !app.text_input_focused()
                    && state.contains(gdk::ModifierType::SHIFT_MASK)
                    && !state.intersects(blocking_modifiers)
                    && matches!(keyval, gdk::Key::s | gdk::Key::S)
                {
                    app.command_prefix.set(true);
                    let serial = app.command_prefix_serial.get() + 1;
                    app.command_prefix_serial.set(serial);
                    let weak = Rc::downgrade(&app);
                    glib::timeout_add_local_once(std::time::Duration::from_millis(1400), move || {
                        if let Some(app) = weak.upgrade() {
                            if app.command_prefix_serial.get() == serial {
                                app.command_prefix.set(false);
                            }
                        }
                    });
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        self.window.add_controller(key);
        let motion = gtk::EventControllerMotion::new();
        let _weak = Rc::downgrade(self);
    }

    pub fn restart_theme_monitor(self: &Rc<Self>) {
        if let Some(monitor) = self.theme_monitor.borrow_mut().take() {
            monitor.cancel();
        }
        let settings = self.settings.borrow().clone();
        if !settings.theme_live_reload {
            return;
        }
        let Some(path) = ThemeManager::watch_path(&settings) else { return };
        // Watch the theme directory so both theme.json and style.css reload.
        let dir = path.parent().map(|p| p.to_path_buf()).unwrap_or(path.clone());
        let file = gio::File::for_path(&dir);
        if let Ok(monitor) = file.monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE) {
            let weak = Rc::downgrade(self);
            monitor.connect_changed(move |_, _, _, event| {
                if matches!(
                    event,
                    gio::FileMonitorEvent::Changed
                        | gio::FileMonitorEvent::ChangesDoneHint
                        | gio::FileMonitorEvent::Created
                ) {
                    if let Some(app) = weak.upgrade() {
                        let settings = app.settings.borrow().clone();
                        match app.theme.reload(&settings) {
                            Ok(()) => app.notify("Theme reloaded", false),
                            Err(err) => app.notify(&format!("Theme not applied: {err}"), true),
                        }
                    }
                }
            });
            *self.theme_monitor.borrow_mut() = Some(monitor);
        }
    }

    pub fn install_canvas_drop(self: &Rc<Self>) {
        let target = gtk::DropTarget::new(
            gdk::FileList::static_type(),
            gdk::DragAction::COPY | gdk::DragAction::MOVE,
        );
        let weak = Rc::downgrade(self);
        let stack = self.ui().canvas_stack.clone();
        target.connect_enter(move |_, _, _| {
            stack.add_css_class("drop-hover");
            gdk::DragAction::MOVE
        });
        let stack2 = self.ui().canvas_stack.clone();
        target.connect_leave(move |_| {
            stack2.remove_css_class("drop-hover");
        });
        target.connect_drop(move |_, value, _, _| {
            let Ok(files) = value.get::<gdk::FileList>() else { return false };
            let paths: Vec<PathBuf> = files.files().iter().filter_map(|f| f.path()).collect();
            if paths.is_empty() {
                return false;
            }
            if let Some(app) = weak.upgrade() {
                let Some(tab) = app.active_tab() else { return false };
                let location = tab.state.borrow().location.clone();
                if let Some(dest) = location.path().cloned() {
                    app.handle_external_drop(tab.id, &dest, paths);
                    return true;
                }
                app.notify("Open a folder before dropping files here.", true);
            }
            false
        });
        self.ui().canvas_stack.add_controller(target);
    }

    // ---------------------------------------------------------------------
    // Chrome refresh
    // ---------------------------------------------------------------------

    pub fn refresh_ui(self: &Rc<Self>) {
        self.rebuild_tab_strip();
        self.update_toolbar();
        self.update_status();
        if let Some(tab) = self.active_tab() {
            let tab_id = tab.id;
            self.ui().inspector_widget.update(self, tab_id);
        }
    }

    pub fn refresh_sidebar(self: &Rc<Self>) {
        self.ui().sidebar_widget.sync(self);
    }

    pub fn rebuild_tab_strip(self: &Rc<Self>) {
        let ui = self.ui();
        let mut child = ui.tab_strip.first_child();
        while let Some(c) = child {
            let next = c.next_sibling();
            ui.tab_strip.remove(&c);
            child = next;
        }
        let tabs = self.tabs.borrow().clone();
        let active = self.active.get();
        for (index, tab) in tabs.iter().enumerate() {            let (title, subtitle, loading, has_scan) = {
                let state = tab.state.borrow();
                (
                    state.location.title(),
                    // A path is already spelled out in the location bar; the
                    // tab only needs the folder name.
                    if matches!(state.location, Location::Path(_)) {
                        String::new()
                    } else {
                        state.location.subtitle()
                    },
                    state.loading,
                    state.scan_running,
                )
            };
            let holder = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            holder.add_css_class("delve-tab");
            if index == active {
                holder.add_css_class("active");
            }
            let inner = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            inner.set_hexpand(true);
            let texts = gtk::Box::new(gtk::Orientation::Vertical, 0);
            texts.set_valign(gtk::Align::Center);
            let name_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
            let name = gtk::Label::new(Some(&crate::util::ellipsize(&title, 26)));
            name.add_css_class("delve-tab-name");
            name.set_xalign(0.0);
            name_row.append(&name);
            if loading || has_scan {
                let busy = gtk::Label::new(Some("●"));
                busy.add_css_class("delve-tab-busy-badge");
                busy.add_css_class("tiny");
                name_row.append(&busy);
            }
            texts.append(&name_row);
            let path_hint = gtk::Label::new(Some(&crate::util::ellipsize(&subtitle, 34)));
            path_hint.add_css_class("delve-tab-path");
            path_hint.set_xalign(0.0);
            path_hint.set_visible(!subtitle.is_empty());
            texts.append(&path_hint);
            inner.append(&texts);
            let tab_click = gtk::Button::new();
            tab_click.set_child(Some(&inner));
            tab_click.add_css_class("delve-flat");
            tab_click.set_hexpand(true);
            tab_click.set_tooltip_text(Some(&format!("{title}\n{subtitle}")));
            {
                let weak = Rc::downgrade(self);
                tab_click.connect_clicked(move |_| {
                    if let Some(app) = weak.upgrade() {
                        app.activate_tab(index);
                    }
                });
            }
            holder.append(&tab_click);
            let close = gtk::Button::from_icon_name("window-close-symbolic");
            close.add_css_class("delve-tab-close");
            close.set_tooltip_text(Some("Close tab (Ctrl+W)"));
            {
                let weak = Rc::downgrade(self);
                let tab_id = tab.id;
                close.connect_clicked(move |_| {
                    if let Some(app) = weak.upgrade() {
                        app.close_tab_by_id(tab_id);
                    }
                });
            }
            holder.append(&close);

            // Context menu on the tab
            {
                let gesture = gtk::GestureClick::new();
                gesture.set_button(3);
                let weak = Rc::downgrade(self);
                let holder_ref = holder.clone();
                let tab_id = tab.id;
                gesture.connect_pressed(move |_, _, _, _| {
                    if let Some(app) = weak.upgrade() {
                        app.show_tab_menu(&holder_ref, tab_id);
                    }
                });
                holder.add_controller(gesture);
            }
            // External file drop onto tab
            {
                let target = gtk::DropTarget::new(
                    gdk::FileList::static_type(),
                    gdk::DragAction::COPY | gdk::DragAction::MOVE,
                );
                let weak = Rc::downgrade(self);
                let holder_ref = holder.clone();
                let tab_id = tab.id;
                target.connect_enter(move |_, _, _| {
                    holder_ref.add_css_class("drop-target");
                    gdk::DragAction::MOVE
                });
                let holder2 = holder.clone();
                target.connect_leave(move |_| {
                    holder2.remove_css_class("drop-target");
                });
                target.connect_drop(move |_, value, _, _| {
                    let Ok(files) = value.get::<gdk::FileList>() else { return false };
                    let paths: Vec<PathBuf> = files.files().iter().filter_map(|f| f.path()).collect();
                    if paths.is_empty() {
                        return false;
                    }
                    if let Some(app) = weak.upgrade() {
                        app.handle_drop_on_tab(tab_id, paths);
                    }
                    true
                });
                holder.add_controller(target);
            }
            // Internal tab reorder
            {
                let source = gtk::DragSource::new();
                source.set_actions(gdk::DragAction::MOVE);
                let weak = Rc::downgrade(self);
                let tab_id = tab.id;
                source.connect_prepare(move |_, _, _| {
                    let app = weak.upgrade()?;
                    let _ = (&app, tab_id);
                    Some(gdk::ContentProvider::for_value(&format!("delve-tab:{}", tab_id).to_value()))
                });
                holder.add_controller(source);
                let target = gtk::DropTarget::new(glib::Type::STRING, gdk::DragAction::MOVE);
                let weak = Rc::downgrade(self);
                let tab_id = tab.id;
                target.connect_drop(move |_, value, _, _| {
                    let Ok(text) = value.get::<String>() else { return false };
                    let Some(source_id) = text
                        .strip_prefix("delve-tab:")
                        .and_then(|s| s.parse::<u64>().ok())
                    else {
                        return false;
                    };
                    if source_id == tab_id {
                        return false;
                    }
                    if let Some(app) = weak.upgrade() {
                        app.reorder_tab(source_id, tab_id);
                    }
                    true
                });
                holder.add_controller(target);
            }
            ui.tab_strip.append(&holder);
        }
        let tab_plus = ui.tab_plus.clone();
        let _ = &tab_plus;
    }

    pub fn update_toolbar(self: &Rc<Self>) {
        let Some(tab) = self.active_tab() else { return };
        let (location, view, can_back, can_forward) = {
            let state = tab.state.borrow();
            (
                state.location.clone(),
                state.view,
                state.history_pos > 0,
                state.history_pos + 1 < state.history.len(),
            )
        };
        let ui = self.ui();
        ui.back.set_sensitive(can_back);
        ui.forward.set_sensitive(can_forward);
        ui.up.set_sensitive(location.path().map(|p| p != std::path::Path::new("/")).unwrap_or(false));
        let (selected, clipboard_has) = {
            let state = tab.state.borrow();
            (state.selection.len(), self.clipboard.borrow().is_some())
        };
        ui.cmd_undo.set_sensitive(!self.undo_stack.borrow().is_empty());
        ui.cmd_redo.set_sensitive(!self.redo_stack.borrow().is_empty());
        ui.cmd_cut.set_sensitive(selected > 0);
        ui.cmd_copy.set_sensitive(selected > 0);
        ui.cmd_paste.set_sensitive(clipboard_has);
        ui.cmd_rename.set_sensitive(selected == 1);
        ui.cmd_trash.set_sensitive(selected > 0);
        ui.view_btn_icon.set_icon_name(Some(view.icon_name()));
        ui.view_btn_label.set_text(view.label());
        ui.view_btn.set_tooltip_text(Some(&format!("{} view — click to switch", view.label())));
        let show_hidden = tab.state.borrow().show_hidden;
        ui.cmd_hidden
            .set_icon_name(if show_hidden { "view-reveal-symbolic" } else { "view-conceal-symbolic" });
        if show_hidden {
            ui.cmd_hidden.add_css_class("active");
        } else {
            ui.cmd_hidden.remove_css_class("active");
        }
        ui.cmd_hidden.set_tooltip_text(Some(if show_hidden {
            "Hide hidden files (Ctrl+H)"
        } else {
            "Show hidden files (Ctrl+H)"
        }));

        // Breadcrumbs
        let mut child = ui.crumb.first_child();
        while let Some(c) = child {
            let next = c.next_sibling();
            ui.crumb.remove(&c);
            child = next;
        }
        let crumbs = location.crumbs();
        if crumbs.is_empty() {
            let icon = gtk::Image::from_icon_name(location.icon_name());
            icon.set_pixel_size(14);
            ui.crumb.append(&icon);
            let label = gtk::Label::new(Some(&location.title()));
            label.add_css_class("crumb");
            label.add_css_class("current");
            ui.crumb.append(&label);
        } else {
            let last_index = crumbs.len().saturating_sub(1);
            for (i, (label_text, path)) in crumbs.iter().enumerate() {
                if i > 0 {
                    let sep = gtk::Label::new(Some("›"));
                    sep.add_css_class("crumb-sep");
                    ui.crumb.append(&sep);
                }
                let button = gtk::Button::new();
                let text = if i == 0 {
                    "/".to_string()
                } else {
                    crate::util::ellipsize(label_text, 22)
                };
                let crumb_body = gtk::Box::new(gtk::Orientation::Horizontal, 4);
                if let Some(color) = self.meta.borrow().folder_color(path) {
                    let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                    dot.add_css_class("color-dot");
                    dot.add_css_class(&format!("swatch-{}", crate::theme::color_class(&color)));
                    dot.set_size_request(6, 6);
                    dot.set_valign(gtk::Align::Center);
                    crumb_body.append(&dot);
                }
                let crumb_text = gtk::Label::new(Some(&text));
                crumb_body.append(&crumb_text);
                button.set_child(Some(&crumb_body));
                button.add_css_class("crumb");
                if i == last_index {
                    button.add_css_class("current");
                }
                button.set_tooltip_text(Some(&path.display().to_string()));
                {
                    let weak = Rc::downgrade(self);
                    let path = path.clone();
                    button.connect_clicked(move |_| {
                        if let Some(app) = weak.upgrade() {
                            app.navigate_active(Location::Path(path.clone()));
                        }
                    });
                }
                // Crumb buttons are drop targets: dragging items onto an
                // ancestor location moves them there.
                crate::views::install_row_drop(&button, self, tab.id);
                ui.crumb.append(&button);
            }
        }
        ui.path_label.set_text(&crate::paths::display_path(
            location.path().map(|p| p.as_path()).unwrap_or(std::path::Path::new("")),
        ));
        ui.window_title
            .set_text(&format!("Delve — {}", location.title()));
        self.window.set_title(Some(&format!("Delve — {}", location.title())));
    }

    pub fn update_status(self: &Rc<Self>) {
        let Some(tab) = self.active_tab() else { return };
        let (entries, selection, loading, scan_running, filter, error) = {
            let state = tab.state.borrow();
            (
                state.visible_entries().len(),
                state.selection.len(),
                state.loading,
                state.scan_running,
                state.filter_text.clone(),
                state.error.clone(),
            )
        };
        let ui = self.ui();
        let mut counts = format!(
            "{} item{}",
            entries,
            if entries == 1 { "" } else { "s" }
        );
        if !filter.trim().is_empty() {
            counts.push_str(" (filtered)");
        }
        ui.counts.set_text(&counts);
        if selection > 0 {
            let size = tab.state.borrow().selected_size();
            let mut text = format!(
                "{} selected · {}",
                selection,
                crate::fs::human_size(size)
            );
            if tab.state.borrow().scan_running {
                text.push_str(" · scanning");
            }
            ui.selection_label.set_text(&text);
        } else {
            ui.selection_label.set_text("");
        }
        let busy = loading || scan_running;
        if busy {
            ui.spinner.set_visible(true);
            ui.spinner.start();
        } else {
            ui.spinner.stop();
            ui.spinner.set_visible(false);
        }
        ui.cancel_btn.set_visible(scan_running);
        let weak = Rc::downgrade(self);
        let current_scan = scan_running;
        let _ = &weak;
        ui.cancel_btn.set_tooltip_text(Some("Cancel the running scan"));
        if let Some(err) = &error {
            ui.path_label.set_text(err);
        }
        let _ = current_scan;
    }

    pub fn notify(self: &Rc<Self>, message: &str, error: bool) {
        crate::util::log(message);
        if let Some(tab) = self.active_tab() {
            if error {
                tab.set_banner(message, "", true);
            } else {
                tab.set_banner(message, "", false);
            }
            let weak = Rc::downgrade(self);
            glib::timeout_add_seconds_local_once(5, move || {
                if let Some(app) = weak.upgrade() {
                    if let Some(tab) = app.active_tab() {
                        tab.clear_banner();
                    }
                }
            });
        }
    }

    pub fn apply_density(self: &Rc<Self>) {
        let density = self.settings.borrow().density;
        let window = self.window.clone();
        for d in Density::all() {
            window.remove_css_class(d.css_class());
        }
        window.add_css_class(density.css_class());
    }

    /// Switch the sidebar between the icon-only rail and the full panel.
    pub fn set_sidebar_mini(self: &Rc<Self>, mini: bool) {
        if self.settings.borrow().sidebar_mini == mini {
            return;
        }
        {
            let mut settings = self.settings.borrow_mut();
            settings.sidebar_mini = mini;
            settings.save();
        }
        self.ui().sidebar_widget.set_mini(self, mini);
        if !self.ui().narrow.get() {
            let width = if mini {
                60
            } else {
                self.settings.borrow().sidebar_width
            };
            self.animate_sidebar_width(width);
        }
    }

    /// Ease the splitter between the rail and full sidebar widths. Position
    /// notifications are ignored while animating so the saved width survives.
    fn animate_sidebar_width(self: &Rc<Self>, to: i32) {
        let paned = self.ui().outer_paned.clone();
        let from = paned.position();
        if from == to || from <= 0 {
            paned.set_position(to);
            return;
        }
        self.ui().sidebar_animating.set(true);
        let weak = Rc::downgrade(self);
        let mut step: i32 = 0;
        const STEPS: i32 = 9;
        glib::timeout_add_local(std::time::Duration::from_millis(14), move || {
            step += 1;
            let t = step as f64 / STEPS as f64;
            let eased = 1.0 - (1.0 - t) * (1.0 - t);
            paned.set_position(from + ((to - from) as f64 * eased) as i32);
            if step >= STEPS {
                if let Some(app) = weak.upgrade() {
                    app.ui().sidebar_animating.set(false);
                }
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    pub fn set_inspector_visible(self: &Rc<Self>, visible: bool) {
        {
            let mut settings = self.settings.borrow_mut();
            settings.inspector_visible = visible;
            settings.save();
        }
        self.ui().inspector_btn.set_active(visible);
        self.apply_responsive(self.window.width());
        if self.ui().narrow.get() {
            if visible {
                self.open_right_drawer();
            } else {
                self.close_drawers();
            }
        }
    }

    pub fn open_left_drawer(self: &Rc<Self>) {
        let ui = self.ui();
        ui.scrim.set_visible(true);
        ui.left_drawer.set_visible(true);
        ui.left_drawer.set_reveal_child(true);
        ui.right_drawer.set_reveal_child(false);
        let weak = Rc::downgrade(self);
        let left = ui.left_drawer.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(180), move || {
            let _ = weak;
            if !left.reveals_child() {
                left.set_visible(false);
            }
        });
    }

    pub fn open_right_drawer(self: &Rc<Self>) {
        let ui = self.ui();
        ui.scrim.set_visible(true);
        ui.right_drawer.set_visible(true);
        ui.right_drawer.set_reveal_child(true);
        ui.left_drawer.set_reveal_child(false);
    }

    /// Open the run panel in its own window. The runner widget is moved, not
    /// copied, so sessions and output continue uninterrupted in both places.
    pub fn open_runner_popout(self: &Rc<Self>) {
        if let Some(window) = self.runner_popout.borrow().as_ref() {
            window.present();
            return;
        }
        let slot = self.ui().runner_slot.clone();
        let root = self.ui().runner.root.clone();
        let slot_widget: gtk::Widget = slot.clone().upcast();
        if root.parent().as_ref() == Some(&slot_widget) {
            slot.remove(&root);
        }

        let window = gtk::Window::builder()
            .title("Delve command runs")
            .transient_for(&self.window)
            .default_width(760)
            .default_height(460)
            .build();
        window.add_css_class("delve-dialog");
        window.set_destroy_with_parent(true);
        root.set_vexpand(true);
        self.ui().runner.reveal(true);
        window.set_child(Some(&root));
        *self.runner_popout.borrow_mut() = Some(window.clone());

        let weak = Rc::downgrade(self);
        window.connect_close_request(move |window| {
            if let Some(child) = window.child() {
                window.set_child(None::<&gtk::Widget>);
                child.set_vexpand(false);
                slot.append(&child);
            }
            if let Some(app) = weak.upgrade() {
                app.runner_popout.borrow_mut().take();
            }
            glib::Propagation::Proceed
        });
        window.present();
    }

    pub fn toggle_runner_popout(self: &Rc<Self>) {
        let window = self.runner_popout.borrow().clone();
        if let Some(window) = window {
            window.close();
        } else {
            self.open_runner_popout();
        }
    }

    /// Hide the run surface: close the detached window when there is one,
    /// otherwise collapse the docked panel.
    pub fn hide_runner(self: &Rc<Self>) {
        let window = self.runner_popout.borrow().clone();
        if let Some(window) = window {
            window.close();
        } else {
            self.ui().runner.reveal(false);
        }
    }

    pub fn close_drawers(self: &Rc<Self>) {
        let ui = self.ui();
        ui.left_drawer.set_reveal_child(false);
        ui.right_drawer.set_reveal_child(false);
        ui.scrim.set_visible(false);
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(std::time::Duration::from_millis(200), move || {
            if let Some(app) = weak.upgrade() {
                if !app.ui().left_drawer.reveals_child() {
                    app.ui().left_drawer.set_visible(false);
                }
                if !app.ui().right_drawer.reveals_child() {
                    app.ui().right_drawer.set_visible(false);
                }
            }
        });
    }

    /// Keep the sidebar docked at every width and move only the inspector
    /// between the splitter (wide) and an overlay drawer (narrow).
    pub fn apply_responsive(self: &Rc<Self>, width: i32) {
        // Before the first allocation the width is 0; switching to the narrow
        // layout then would detach the panels for no reason.
        if width <= 0 {
            return;
        }
        let narrow = width < 900;
        let settings = self.settings.borrow().clone();
        let ui = self.ui();

        // The sidebar is never fully collapsible: it stays in the splitter,
        // either as the full panel or as the compact icon rail.
        let sidebar_parent: gtk::Widget = ui.outer_paned.clone().upcast();
        if ui.sidebar_widget.root.parent().as_ref() != Some(&sidebar_parent) {
            ui.outer_paned.set_start_child(Some(&ui.sidebar_widget.root));
        }
        ui.sidebar_widget.root.set_visible(true);
        ui.outer_paned.set_position(if settings.sidebar_mini {
            60
        } else {
            settings.sidebar_width.max(190)
        });

        if narrow == ui.narrow.get() {
            ui.inspector_btn.set_visible(true);
            ui.inspector_widget.root.set_visible(settings.inspector_visible || narrow);
            if !narrow && settings.inspector_visible {
                let width_now = ui.inner_paned.width();
                if width_now > 0 {
                    ui.inner_paned
                        .set_position((width_now - settings.inspector_width).max(280));
                }
            }
            return;
        }
        ui.narrow.set(narrow);
        if narrow {
            ui.inner_paned.set_end_child(None::<&gtk::Widget>);
            let right_parent: gtk::Widget = ui.drawer_right_box.clone().upcast();
            if ui.inspector_widget.root.parent().as_ref() != Some(&right_parent) {
                ui.drawer_right_box.append(&ui.inspector_widget.root);
            }
            ui.right_drawer.set_visible(false);
        } else {
            ui.close_drawers_public();
            let right_parent: gtk::Widget = ui.drawer_right_box.clone().upcast();
            if ui.inspector_widget.root.parent().as_ref() == Some(&right_parent) {
                ui.drawer_right_box.remove(&ui.inspector_widget.root);
            }
            ui.inner_paned.set_end_child(Some(&ui.inspector_widget.root));
            let width_now = ui.inner_paned.width();
            if width_now > 0 {
                ui.inner_paned
                    .set_position((width_now - settings.inspector_width).max(280));
            }
        }
        ui.inspector_btn.set_visible(true);
        ui.inspector_widget.root.set_visible(settings.inspector_visible || narrow);
    }

    pub fn open_initial(self: &Rc<Self>, location: Location) {
        let tab = self.new_tab(location);
        let show_tips = !self.settings.borrow().tips_seen;
        if show_tips {
            if let Some(tab) = tab {
                tab.set_banner(
                    "Welcome to Delve",
                    "Tabs keep places open · Ctrl+1–5 switch views · tags and folder colours are in the sidebar.",
                    false,
                );
                self.settings.borrow_mut().tips_seen = true;
                self.settings.borrow().save();
            }
        }
    }
}

impl Ui {
    pub fn close_drawers_public(&self) {
        self.left_drawer.set_reveal_child(false);
        self.right_drawer.set_reveal_child(false);
        self.scrim.set_visible(false);
        self.left_drawer.set_visible(false);
        self.right_drawer.set_visible(false);
    }
}

/// Map the number row (with or without Shift) to a command slot. Zero is the
/// special “choose sets” slot.
fn command_shortcut_digit(keyval: gdk::Key) -> Option<u8> {
    match keyval.to_unicode()? {
        '0' => Some(0),
        '1' | '!' => Some(1),
        '2' | '@' => Some(2),
        '3' | '#' => Some(3),
        '4' | '$' => Some(4),
        '5' | '%' => Some(5),
        '6' | '^' => Some(6),
        '7' | '&' => Some(7),
        '8' | '*' => Some(8),
        '9' | '(' => Some(9),
        _ => None,
    }
}

fn command_button(icon: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.add_css_class("command-button");
    button.set_tooltip_text(Some(tooltip));
    button.set_size_request(32, 32);
    button
}

pub fn nav_button(icon: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.add_css_class("toolbar-button");
    button.set_tooltip_text(Some(tooltip));
    button
}

/// Keep entries available for views that need a plain `Entry`.
#[allow(dead_code)]
fn _entry(_: &Entry) {}
