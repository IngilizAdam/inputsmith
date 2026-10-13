//! Application state, update logic and views.

use std::collections::{HashMap, HashSet};

use iced::widget::{
    Space, button, column, container, mouse_area, pin, radio, row, scrollable, slider, stack, svg, text, text_input,
    toggler,
};
use iced::{Alignment, Element, Length, Padding, Subscription, Task, Theme, window};
use inputsmith_ipc::api::{self, ProfileAssignment, ProfileList, routes};
use inputsmith_ipc::model::{Application, Attribute, Card, Profile, ScrollMode, ScrollWheelSettings, WheelDirection};
use inputsmith_ipc::{Message as IpcMessage, Verb};

use crate::client::{self, Event};
use crate::model::{self, card_title};
use crate::theme::{self, ACCENT, BOLD, MUTED, SEMIBOLD};
use crate::tray::{self, TrayEvent};
use crate::catalog::{self, DeviceArt, Hotspot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Buttons,
    PointScroll,
    Settings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Page {
    Home,
    Device { id: String, tab: Tab },
    AppSettings,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Agent(Event),
    Tray(TrayEvent),
    WindowOpened(window::Id),
    WindowClosed(window::Id),
    Navigate(Page),
    SelectSlot(Option<String>),
    Assign {
        slot: String,
        card: Box<Card>,
    },
    RestoreDefault(String),
    SelectNested {
        slot: String,
        key: String,
    },
    Search(String),
    ToggleCategory(String),
    SelectProfile(Option<String>),
    AddApplication,
    ApplicationPicked(Option<String>),
    RemoveProfile(String),
    PointerSpeed(f32),
    Sensitivity(f32),
    ScrollForce(f32),
    CommitScroll,
    CommitPointer,
    SmartShift(bool),
    Smooth(bool),
    /// Scroll direction: `true` = natural ("Inverted").
    Direction(bool),
    HScroll(u16, bool),
    SwapButtons(bool),
    ForgetDevice(String),
    Autostart(bool),
    CreateBackup,
    RestoreBackup(String),
    Export,
    Import,
    ImportOptionsPlus,
    FilePicked(FileOp, Option<String>),
    ResetDevice(String),
    Quit,
}

#[derive(Debug, Clone, Copy)]
pub enum FileOp {
    Export,
    Import,
}

#[derive(Default)]
struct Drafts {
    pointer: Option<f32>,
    sensitivity: Option<f32>,
    force: Option<f32>,
}

pub struct App {
    window: Option<window::Id>,
    connected: bool,
    devices: Vec<api::Device>,
    profiles: Vec<Profile>,
    applications: Vec<Application>,
    backups: Vec<api::Backup>,
    art: HashMap<String, DeviceArt>,
    page: Page,
    selected_slot: Option<String>,
    app_profile: Option<String>,
    search: String,
    expanded: HashSet<String>,
    drafts: Drafts,
    status: Option<String>,
    buttons_swapped: Option<bool>,
    autostart: bool,
}

const SIDE_WIDTH: f32 = 340.0;

/// Selection key of a Point & Scroll "horizontal scrolling" hotspot (they share the scroll wheel slot).
fn hscroll_slot(prefix: &str, cid: u16) -> String {
    format!("{prefix}_hscroll_c{cid}")
}

fn hscroll_slot_cid(prefix: &str, slot: &str) -> Option<u16> {
    slot.strip_prefix(prefix)?.strip_prefix("_hscroll_c")?.parse().ok()
}

impl App {
    pub fn boot(start_hidden: bool) -> (Self, Task<Msg>) {
        tray::create();
        let app = Self {
            window: None,
            connected: false,
            devices: Vec::new(),
            profiles: Vec::new(),
            applications: Vec::new(),
            backups: Vec::new(),
            art: HashMap::new(),
            page: Page::Home,
            selected_slot: None,
            app_profile: None,
            search: String::new(),
            expanded: HashSet::new(),
            drafts: Drafts::default(),
            status: None,
            buttons_swapped: None,
            autostart: crate::desktop::autostart_enabled(),
        };
        let task = if start_hidden { Task::none() } else { open_window() };
        (app, task)
    }

    pub fn title(&self, _: window::Id) -> String {
        "Inputsmith".into()
    }

    pub fn theme(&self, _: window::Id) -> Theme {
        theme::theme()
    }

    pub fn subscription(&self) -> Subscription<Msg> {
        Subscription::batch([
            client::subscription().map(Msg::Agent),
            tray::subscription().map(Msg::Tray),
            window::close_events().map(Msg::WindowClosed),
        ])
    }

    fn device(&self, id: &str) -> Option<&api::Device> {
        self.devices.iter().find(|d| d.id == id)
    }

    fn global_profile_id(&self) -> String {
        model::default_profile(&self.profiles).map(|p| p.id.clone()).unwrap_or_default()
    }

    /// Profile that edits go to: the selected application profile or global.
    fn target_profile(&self) -> String {
        self.app_profile.clone().unwrap_or_else(|| self.global_profile_id())
    }

    fn send_assignment(&self, profile_id: String, slot: &str, card: Card) {
        let pa =
            ProfileAssignment { profile_id, assignment: Some(model::assignment(slot, card)), ..Default::default() };
        client::set(routes::ASSIGNMENT, &pa);
    }

    pub fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Agent(ev) => self.on_agent(ev),
            Msg::Tray(TrayEvent::Open) => {
                return match self.window {
                    Some(id) => window::gain_focus(id),
                    None => open_window(),
                };
            }
            Msg::Tray(TrayEvent::Quit) | Msg::Quit => return iced::exit(),
            Msg::WindowOpened(id) => {
                self.window = Some(id);
                client::refresh_all();
            }
            Msg::WindowClosed(id) => {
                if self.window == Some(id) {
                    self.window = None;
                }
            }
            Msg::Navigate(page) => {
                if !matches!((&self.page, &page), (Page::Device { id: a, .. }, Page::Device { id: b, .. }) if a == b) {
                    self.app_profile = None;
                }
                self.selected_slot = None;
                self.search.clear();
                self.drafts = Drafts::default();
                if matches!(page, Page::Device { tab: Tab::Settings, .. } | Page::AppSettings) {
                    client::get(routes::BACKUPS);
                    client::get(routes::BUTTON_SWAP);
                }
                self.page = page;
            }
            Msg::SelectSlot(s) => {
                self.selected_slot = s;
                self.search.clear();
                self.drafts = Drafts::default();
            }
            Msg::Assign { slot, card } => {
                let mut card = *card;
                // Gesture presets come with no selection; pick the first option.
                if card.attribute == Attribute::OneOf && card.selected_nested_card.is_empty() {
                    if let Some(first) = card.nested_cards_order.first() {
                        card.selected_nested_card = first.clone();
                    }
                }
                self.send_assignment(self.target_profile(), &slot, card);
            }
            Msg::RestoreDefault(slot) => {
                let pa = ProfileAssignment {
                    profile_id: self.target_profile(),
                    assignment: Some(model::assignment(&slot, Card::default())),
                    ..Default::default()
                };
                client::remove(routes::ASSIGNMENT, &pa);
            }
            Msg::SelectNested { slot, key } => {
                if let Some(a) = model::resolve(&self.profiles, self.app_profile.as_deref(), &slot) {
                    if let Some(mut card) = a.card.clone() {
                        card.selected_nested_card = key;
                        self.send_assignment(self.target_profile(), &slot, card);
                    }
                }
            }
            Msg::Search(s) => self.search = s,
            Msg::ToggleCategory(c) => {
                if !self.expanded.remove(&c) {
                    self.expanded.insert(c);
                }
            }
            Msg::SelectProfile(p) => {
                self.app_profile = p;
                self.selected_slot = None;
            }
            Msg::AddApplication => {
                return Task::perform(
                    async {
                        let mut dlg = rfd::AsyncFileDialog::new().set_title("Choose an application");
                        if cfg!(windows) {
                            dlg = dlg.add_filter("Applications", &["exe"]);
                        }
                        dlg.pick_file().await.map(|f| f.path().display().to_string())
                    },
                    Msg::ApplicationPicked,
                );
            }
            Msg::ApplicationPicked(Some(path)) => {
                let name = std::path::Path::new(&path)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.clone());
                let app_id = format!("inputsmith_app_{}", name.to_lowercase().replace(' ', "_"));
                let lower = path.to_lowercase();
                let payload = serde_json::json!({
                    "profile": { "name": name, "applicationId": app_id, "activeForApplication": true },
                    "application": {
                        "applicationId": app_id, "databaseId": app_id, "name": name,
                        "applicationPath": lower, "applicationPathsList": [lower], "isCustom": true, "isInstalled": true
                    }
                });
                client::send(Verb::Set, routes::APP_PROFILE, Some(payload));
            }
            Msg::ApplicationPicked(None) => {}
            Msg::RemoveProfile(id) => {
                if self.app_profile.as_deref() == Some(id.as_str()) {
                    self.app_profile = None;
                }
                client::remove(routes::APP_PROFILE, &api::IdArg { id });
            }
            Msg::PointerSpeed(v) => self.drafts.pointer = Some(v),
            Msg::Sensitivity(v) => self.drafts.sensitivity = Some(v),
            Msg::ScrollForce(v) => self.drafts.force = Some(v),
            Msg::CommitPointer => {
                if let (Some(v), Some(dev)) = (self.drafts.pointer, self.current_device()) {
                    let slot = format!("{}_mouse_settings", dev.slot_prefix);
                    let cur = model::resolve(&self.profiles, None, &slot).and_then(|a| a.card.clone());
                    let card = model::with_pointer_speed(cur.as_ref(), &slot, v.round());
                    self.send_assignment(self.global_profile_id(), &slot, card);
                }
            }
            Msg::CommitScroll => {
                let (s, f) = (self.drafts.sensitivity, self.drafts.force);
                self.edit_scroll(|w| {
                    let ss = w.smartshift.get_or_insert_with(Default::default);
                    if let Some(s) = s {
                        ss.sensitivity = s.round() as u32;
                    }
                    if let Some(f) = f {
                        ss.scroll_force = f.round() as u32;
                        ss.is_scroll_force_enabled = true;
                    }
                });
            }
            Msg::SmartShift(on) => self.edit_scroll(|w| {
                let ss = w.smartshift.get_or_insert_with(Default::default);
                ss.is_enabled = on;
                ss.mode = ScrollMode::Ratchet;
            }),
            Msg::Smooth(on) => self.edit_scroll(|w| w.is_smooth = on),
            Msg::Direction(natural) => {
                self.edit_scroll(|w| w.dir = if natural { WheelDirection::Natural } else { WheelDirection::Standard })
            }
            Msg::HScroll(cid, on) => self.edit_scroll(|w| model::set_hscroll(w, cid, on)),
            Msg::SwapButtons(swapped) => {
                self.buttons_swapped = Some(swapped);
                client::set(routes::BUTTON_SWAP, &api::ButtonSwap { swapped });
                client::get(routes::BUTTON_SWAP);
            }
            Msg::ForgetDevice(id) => {
                client::set(routes::DEVICE_FORGET, &api::IdArg { id });
                self.page = Page::Home;
            }
            Msg::Autostart(on) => match crate::desktop::set_autostart(on) {
                Ok(()) => self.autostart = crate::desktop::autostart_enabled(),
                Err(e) => self.status = Some(format!("Could not change start at login: {e}")),
            },
            Msg::CreateBackup => client::set(routes::BACKUP_CREATE, &api::LabelArg { label: "manual".into() }),
            Msg::RestoreBackup(path) => {
                client::set(routes::BACKUP_RESTORE, &api::PathArg { path });
                self.status = Some("Backup restored".into());
            }
            Msg::Export => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_file_name("inputsmith-settings.json")
                            .add_filter("JSON", &["json"])
                            .save_file()
                            .await
                            .map(|f| f.path().display().to_string())
                    },
                    |p| Msg::FilePicked(FileOp::Export, p),
                );
            }
            Msg::Import => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .add_filter("JSON", &["json"])
                            .pick_file()
                            .await
                            .map(|f| f.path().display().to_string())
                    },
                    |p| Msg::FilePicked(FileOp::Import, p),
                );
            }
            Msg::FilePicked(op, Some(path)) => match op {
                FileOp::Export => {
                    client::set(routes::SETTINGS_EXPORT, &api::PathArg { path: path.clone() });
                    self.status = Some(format!("Settings exported to {path}"));
                }
                FileOp::Import => {
                    client::set(routes::SETTINGS_IMPORT, &api::PathArg { path });
                    self.status = Some("Settings imported (previous settings were backed up)".into());
                }
            },
            Msg::FilePicked(_, None) => {}
            Msg::ImportOptionsPlus => {
                client::send(Verb::Set, routes::SETTINGS_IMPORT_OPTIONS_PLUS, None);
                self.status = Some("Settings imported from Logi Options+ (previous settings were backed up)".into());
                client::get(routes::BACKUPS);
            }
            Msg::ResetDevice(id) => {
                client::set(routes::DEVICE_RESET, &api::IdArg { id });
                self.status = Some("Device restored to defaults".into());
            }
        }
        Task::none()
    }

    /// Developer aid for scripted screenshots: `INPUTSMITH_START_PAGE` =
    /// `buttons` | `point` | `settings` | `app-settings` opens that page of the
    /// first device once; `INPUTSMITH_START_SLOT` preselects a slot.
    fn apply_start_page(&mut self) {
        use std::sync::atomic::{AtomicBool, Ordering};
        static DONE: AtomicBool = AtomicBool::new(false);
        let Ok(page) = std::env::var("INPUTSMITH_START_PAGE") else { return };
        let Some(id) = self.devices.first().map(|d| d.id.clone()) else { return };
        if DONE.swap(true, Ordering::Relaxed) {
            return;
        }
        self.page = match page.as_str() {
            "point" => Page::Device { id, tab: Tab::PointScroll },
            "settings" => Page::Device { id, tab: Tab::Settings },
            "app-settings" => Page::AppSettings,
            _ => Page::Device { id, tab: Tab::Buttons },
        };
        if matches!(self.page, Page::Device { tab: Tab::Settings, .. } | Page::AppSettings) {
            client::get(routes::BACKUPS);
            client::get(routes::BUTTON_SWAP);
        }
        self.selected_slot = std::env::var("INPUTSMITH_START_SLOT").ok();
    }

    fn current_device(&self) -> Option<&api::Device> {
        match &self.page {
            Page::Device { id, .. } => self.device(id),
            _ => None,
        }
    }

    fn edit_scroll(&mut self, f: impl FnOnce(&mut ScrollWheelSettings)) {
        let Some(dev) = self.current_device() else { return };
        let slot = format!("{}_mouse_scroll_wheel_settings", dev.slot_prefix);
        let cur = model::resolve(&self.profiles, None, &slot).and_then(|a| a.card.clone());
        let mut s = model::scroll_settings(cur.as_ref(), dev.scroll_defaults.as_ref());
        f(&mut s);
        let card = model::with_scroll_settings(cur.as_ref(), &slot, s);
        self.send_assignment(self.global_profile_id(), &slot, card);
    }

    fn on_agent(&mut self, ev: Event) {
        match ev {
            Event::Connected => {
                self.connected = true;
                client::refresh_all();
            }
            Event::Disconnected => self.connected = false,
            Event::Message(m) => self.on_message(*m),
        }
    }

    fn on_message(&mut self, m: IpcMessage) {
        if !m.is_ok() {
            self.status = Some(format!("{} failed: {}", m.path, if m.error.is_empty() { "error" } else { &m.error }));
            return;
        }
        let payload = || m.payload.clone();
        match m.path.as_str() {
            routes::DEVICES | routes::DEVICES_CHANGED => {
                if let Some(list) = payload().and_then(|v| serde_json::from_value::<api::DeviceList>(v).ok()) {
                    for d in &list.devices {
                        self.art
                            .entry(d.id.clone())
                            .or_insert_with(|| catalog::art(&d.model_id, &d.slot_prefix));
                    }
                    self.devices = list.devices;
                    self.apply_start_page();
                    tray::update(&self.devices);
                }
            }
            routes::PROFILES => {
                if let Some(p) = payload().and_then(|v| serde_json::from_value::<ProfileList>(v).ok()) {
                    self.profiles = p.profiles;
                }
            }
            routes::PROFILES_CHANGED => {
                if let Some(u) = payload().and_then(|v| serde_json::from_value::<ProfileList>(v).ok()) {
                    self.profiles = u.profiles;
                    self.drafts = Drafts::default();
                }
                client::get(routes::APPLICATIONS);
            }
            routes::APPLICATIONS => {
                if let Some(v) = payload() {
                    if let Ok(a) = serde_json::from_value::<Vec<Application>>(v["applications"].clone()) {
                        self.applications = a;
                    }
                }
            }
            routes::BACKUPS => {
                if let Some(b) = payload().and_then(|v| serde_json::from_value::<api::BackupList>(v).ok()) {
                    self.backups = b.backups;
                }
            }
            routes::BUTTON_SWAP => {
                if let Some(s) = payload().and_then(|v| serde_json::from_value::<api::ButtonSwap>(v).ok()) {
                    self.buttons_swapped = Some(s.swapped);
                }
            }
            routes::BACKUP_CREATE => {
                self.status = Some("Backup created".into());
                client::get(routes::BACKUPS);
            }
            _ => {}
        }
    }

    // ---------------------------------------------------------------- views

    pub fn view(&self, _: window::Id) -> Element<'_, Msg> {
        let body = match &self.page {
            Page::Home => self.home(),
            Page::Device { id, tab } => match self.device(id) {
                Some(d) => self.device_page(d, *tab),
                None => self.home(),
            },
            Page::AppSettings => self.app_settings(),
        };
        let mut col = column![body].height(Length::Fill);
        if !self.connected {
            col = col.push(banner("Connecting to the Inputsmith agent…"));
        } else if let Some(s) = &self.status {
            col = col.push(banner(s));
        }
        container(col).width(Length::Fill).height(Length::Fill).style(theme::page).into()
    }

    fn home(&self) -> Element<'_, Msg> {
        let greeting = greeting();
        let header = row![
            text(greeting).size(30).font(BOLD),
            Space::new().width(Length::Fill),
            button(text("⚙").size(20)).style(theme::ghost).padding([6, 10]).on_press(Msg::Navigate(Page::AppSettings)),
        ]
        .align_y(Alignment::Center)
        .padding(Padding { top: 24.0, right: 28.0, bottom: 8.0, left: 32.0 });

        let tiles: Vec<Element<'_, Msg>> = self.devices.iter().map(|d| self.device_tile(d)).collect();
        let grid = if tiles.is_empty() {
            Element::from(
                container(text(if self.connected { "No devices found" } else { "" }).size(18).color(MUTED))
                    .center(Length::Fill),
            )
        } else {
            row(tiles).spacing(32).wrap().into()
        };
        column![header, container(grid).padding([24, 32]).center_x(Length::Fill).height(Length::Fill)].into()
    }

    fn device_tile<'a>(&'a self, d: &'a api::Device) -> Element<'a, Msg> {
        let art = self.art.get(&d.id);
        // Devices that are off stay listed, dimmed, like Options+ shows them.
        let opacity: f32 = if d.online { 1.0 } else { 0.45 };
        let img: Element<'a, Msg> = match art.and_then(|a| a.image.as_ref()) {
            Some(a) => svg(a.handle.clone()).width(260.0 * a.width / a.height).height(260).opacity(opacity).into(),
            None => container(text(device_glyph(&d.kind)).size(90)).center_x(200).height(260).into(),
        };
        let tile = button(column![img, text(&d.name).size(15).font(SEMIBOLD)].spacing(10).align_x(Alignment::Center))
            .style(theme::device_tile)
            .padding([20, 20])
            .on_press(Msg::Navigate(Page::Device { id: d.id.clone(), tab: Tab::Buttons }));
        let mut status = row![battery_pill(d)].spacing(6).align_y(Alignment::Center);
        if !d.online {
            status = status.push(
                button(text("🗑").size(13))
                    .style(theme::ghost)
                    .padding([3, 8])
                    .on_press(Msg::ForgetDevice(d.id.clone())),
            );
        }
        column![tile, status].spacing(4).align_x(Alignment::Center).into()
    }

    fn device_page<'a>(&'a self, d: &'a api::Device, tab: Tab) -> Element<'a, Msg> {
        let top = row![
            button(text("←").size(22)).style(theme::ghost).padding([2, 10]).on_press(Msg::Navigate(Page::Home)),
            text(&d.name).size(26).font(BOLD),
            Space::new().width(Length::Fill),
            self.profile_bar(),
        ]
        .spacing(12)
        .align_y(Alignment::Center)
        .padding(Padding { top: 18.0, right: 20.0, bottom: 4.0, left: 16.0 });

        let nav_item = |label: &'static str, t: Tab| {
            button(text(label).size(12).font(BOLD))
                .style(theme::nav(t == tab))
                .padding([9, 14])
                .width(Length::Fill)
                .on_press(Msg::Navigate(Page::Device { id: d.id.clone(), tab: t }))
        };
        let mut nav = column![nav_item("BUTTONS", Tab::Buttons)].spacing(6).width(190);
        if d.dpi.is_some() || d.capabilities.smartshift || d.capabilities.hires_wheel {
            nav = nav.push(nav_item("POINT & SCROLL", Tab::PointScroll));
        }
        nav = nav.push(nav_item("SETTINGS", Tab::Settings));
        let left = column![Space::new().height(Length::Fill), nav, Space::new().height(Length::Fill), battery_pill(d)]
            .padding(Padding { top: 0.0, right: 0.0, bottom: 24.0, left: 24.0 })
            .height(Length::Fill);

        let (center, side): (Element<'a, Msg>, Option<Element<'a, Msg>>) = match tab {
            Tab::Buttons => (self.hotspot_view(d, true), self.selected_slot.as_ref().map(|s| self.action_panel(d, s))),
            Tab::PointScroll => (self.hotspot_view(d, false), Some(self.point_scroll_panel(d))),
            Tab::Settings => (self.settings_view(d), None),
        };
        let main = row![left, container(center).center(Length::Fill)].height(Length::Fill);
        let page = column![top, main];
        match side {
            Some(side) => row![
                page.width(Length::Fill),
                container(side).width(SIDE_WIDTH).height(Length::Fill).style(theme::side_panel)
            ]
            .into(),
            None => page.into(),
        }
    }

    fn profile_bar(&self) -> Element<'_, Msg> {
        let mut r = row![
            button(text("All applications").size(12).font(SEMIBOLD))
                .style(theme::nav(self.app_profile.is_none()))
                .padding([6, 10])
                .on_press(Msg::SelectProfile(None))
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        for p in self.profiles.iter().filter(|p| !model::is_default(p)) {
            let selected = self.app_profile.as_deref() == Some(p.id.as_str());
            let name = model::application_name(&self.applications, p);
            let mut item = row![
                button(text(name).size(12).font(SEMIBOLD))
                    .style(theme::nav(selected))
                    .padding([6, 10])
                    .on_press(Msg::SelectProfile(Some(p.id.clone())))
            ]
            .align_y(Alignment::Center);
            if selected {
                item = item.push(
                    button(text("✕").size(11))
                        .style(theme::ghost)
                        .padding([4, 6])
                        .on_press(Msg::RemoveProfile(p.id.clone())),
                );
            }
            r = r.push(item);
        }
        r.push(
            button(text("+  ADD APPLICATION").size(12).font(BOLD))
                .style(theme::ghost)
                .padding([6, 10])
                .on_press(Msg::AddApplication),
        )
        .into()
    }

    /// Device render with hotspot markers and callouts.
    fn hotspot_view<'a>(&'a self, d: &'a api::Device, buttons: bool) -> Element<'a, Msg> {
        const IMG_H: f32 = 430.0;
        const MARGIN: f32 = 230.0;
        const CALLOUT_W: f32 = 180.0;
        let art = self.art.get(&d.id).cloned().unwrap_or_default();
        let spots = if buttons { art.button_hotspots.clone() } else { art.point_scroll_hotspots.clone() };
        let spots = if spots.is_empty() { self.synthetic_spots(d, buttons) } else { spots };
        let Some(img) = art.image else {
            return self.spot_list(d, &spots, buttons);
        };
        let img_w = IMG_H * img.width / img.height;
        let total_w = img_w + 2.0 * MARGIN;
        let mut layers: Vec<Element<'a, Msg>> = vec![
            Space::new().width(total_w).height(IMG_H + 40.0).into(),
            pin(svg(img.handle).width(img_w).height(IMG_H)).x(MARGIN).y(20.0).into(),
        ];
        // Several point&scroll hotspots can share one settings slot; show each slot once.
        let mut seen = HashSet::new();
        for s in spots.iter().filter(|s| seen.insert((s.slot_id.clone(), s.name_key.clone()))) {
            // Point & Scroll shows the scroll wheel panel when nothing is selected.
            let default_slot = (!buttons).then(|| format!("{}_mouse_scroll_wheel_settings", d.slot_prefix));
            // Horizontal scrolling hotspots share the scroll wheel slot; select them by control.
            let key = match (buttons, model::hscroll_control(&s.name_key)) {
                (false, Some(cid)) => hscroll_slot(&d.slot_prefix, cid),
                _ => s.slot_id.clone(),
            };
            let selected = self.selected_slot.as_ref().or(default_slot.as_ref()) == Some(&key);
            let mx = MARGIN + img_w * s.x / 100.0;
            let my = 20.0 + IMG_H * s.y / 100.0;
            layers.push(
                pin(container(Space::new().width(14).height(14)).style(theme::marker(selected)))
                    .x(mx - 7.0)
                    .y(my - 7.0)
                    .into(),
            );
            let (title, detail) = self.slot_summary(d, s, buttons);
            let cx = if s.dx < 0.0 {
                (mx - CALLOUT_W - 40.0).max(0.0)
            } else if s.dx > 0.0 {
                (mx + 40.0).min(total_w - CALLOUT_W)
            } else {
                mx - CALLOUT_W / 2.0
            };
            let cy = my - 18.0 + s.dy * 50.0;
            let callout = button(column![text(title).size(13).font(BOLD), text(detail).size(11)].spacing(2))
                .style(theme::callout(selected))
                .padding([7, 10])
                .width(CALLOUT_W)
                .on_press(Msg::SelectSlot(Some(key)));
            layers.push(pin(callout).x(cx).y(cy.max(0.0)).into());
        }
        mouse_area(stack(layers)).into()
    }

    /// Hotspots from the device's control list when the device has no art.
    fn synthetic_spots(&self, d: &api::Device, buttons: bool) -> Vec<Hotspot> {
        if buttons {
            d.controls
                .iter()
                .filter(|c| c.divertable)
                .map(|c| Hotspot {
                    slot_id: c.slot_id.clone(),
                    name_key: control_name_key(c.cid).into(),
                    ..Default::default()
                })
                .collect()
        } else {
            let scroll_slot = format!("{}_mouse_scroll_wheel_settings", d.slot_prefix);
            let mut spots = vec![
                Hotspot {
                    slot_id: scroll_slot.clone(),
                    name_key: "SLOT_NAME_SCROLL_WHEEL_SETTINGS".into(),
                    ..Default::default()
                },
                Hotspot {
                    slot_id: format!("{}_mouse_settings", d.slot_prefix),
                    name_key: "SLOT_NAME_POINTER_SETTINGS".into(),
                    ..Default::default()
                },
            ];
            for c in d.controls.iter().filter(|c| c.horizontal_scroll) {
                let button = if c.cid == 0x53 { "BACK" } else { "FORWARD" };
                spots.push(Hotspot {
                    slot_id: scroll_slot.clone(),
                    name_key: format!("SLOT_NAME_HORIZONTAL_SCROLL_{button}_BUTTON"),
                    ..Default::default()
                });
            }
            spots
        }
    }

    fn spot_list<'a>(&'a self, d: &'a api::Device, spots: &[Hotspot], buttons: bool) -> Element<'a, Msg> {
        let rows: Vec<Element<'a, Msg>> = spots
            .iter()
            .map(|s| {
                let (title, detail) = self.slot_summary(d, s, buttons);
                let key = match (buttons, model::hscroll_control(&s.name_key)) {
                    (false, Some(cid)) => hscroll_slot(&d.slot_prefix, cid),
                    _ => s.slot_id.clone(),
                };
                let selected = self.selected_slot.as_ref() == Some(&key);
                button(row![text(title).size(14).font(BOLD).width(220), text(detail).size(13)].spacing(12))
                    .style(theme::callout(selected))
                    .padding([10, 14])
                    .width(480)
                    .on_press(Msg::SelectSlot(Some(key)))
                    .into()
            })
            .collect();
        column(rows).spacing(10).into()
    }

    fn slot_summary(&self, d: &api::Device, s: &Hotspot, buttons: bool) -> (String, String) {
        let title = catalog::text(&s.name_key);
        let card =
            model::resolve(&self.profiles, self.app_profile.as_deref(), &s.slot_id).and_then(|a| a.card.as_ref());
        if buttons {
            // Options+ layout: assigned action as the title, then the slot.
            return match card {
                Some(c) if c.attribute == Attribute::OneOf => {
                    let sel = c.nested_cards.get(&c.selected_nested_card).map(|n| catalog::text(&n.name)).unwrap_or_default();
                    (catalog::text(&c.name), format!("{sel}\n{title}"))
                }
                Some(c) => (card_title(c), title),
                None => ("Default".into(), title),
            };
        }
        if s.slot_id.ends_with("_mouse_settings") {
            let pct = self.drafts.pointer.unwrap_or_else(|| model::pointer_speed_percent(card));
            return (title, format!("Speed: {pct:.0}%"));
        }
        let w = model::scroll_settings(card, d.scroll_defaults.as_ref());
        if let Some(cid) = model::hscroll_control(&s.name_key) {
            return (title, format!("Horizontal scroll: {}", on_off(model::hscroll_enabled(&w, cid))));
        }
        let ss = w.smartshift.clone().unwrap_or_default();
        let dir = if w.dir == WheelDirection::Natural { "Inverted" } else { "Standard" };
        let mut detail = format!("Scroll direction: {dir}\nSmooth scrolling: {}", on_off(w.is_smooth));
        if d.capabilities.smartshift {
            detail.push_str(&format!("\nSmartShift: {}", on_off(ss.is_enabled)));
        }
        (title, detail)
    }

    /// Right-hand action picker for a button slot.
    fn action_panel<'a>(&'a self, d: &'a api::Device, slot: &'a str) -> Element<'a, Msg> {
        let cid: u16 = slot.rsplit("_c").next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let current = model::resolve(&self.profiles, self.app_profile.as_deref(), slot).and_then(|a| a.card.as_ref());
        let current_id = current.map(|c| c.id.clone()).unwrap_or_default();
        let q = self.search.to_lowercase();
        let matches = |c: &Card| q.is_empty() || card_title(c).to_lowercase().contains(&q);

        let row_for = |c: &Card| -> Element<'a, Msg> {
            let selected = c.id == current_id;
            button(text(card_title(c)).size(13))
                .style(theme::action_row(selected))
                .padding([8, 12])
                .width(Length::Fill)
                .on_press(Msg::Assign { slot: slot.to_string(), card: Box::new(c.clone()) })
                .into()
        };

        let mut list = column![].spacing(2);
        // Gesture button: preset chooser like Options+.
        if let Some(card) = current.filter(|c| c.attribute == Attribute::OneOf) {
            list = list.push(section_title("GESTURES"));
            for key in &card.nested_cards_order {
                if let Some(n) = card.nested_cards.get(key) {
                    let selected = &card.selected_nested_card == key;
                    list = list.push(
                        button(text(catalog::text(&n.name)).size(13))
                            .style(theme::action_row(selected))
                            .padding([8, 12])
                            .width(Length::Fill)
                            .on_press(Msg::SelectNested { slot: slot.to_string(), key: key.clone() }),
                    );
                }
            }
            if let Some(sel) = card.nested_cards.get(&card.selected_nested_card) {
                let mut dirs = column![].spacing(4).padding([8, 12]);
                for k in ["up", "down", "left", "right", "click"] {
                    if let Some(c) = sel.nested_cards.get(k) {
                        let name = if c.name.is_empty() { "—".to_string() } else { card_title(c) };
                        dirs =
                            dirs.push(row![text(capitalize(k)).size(12).color(MUTED).width(60), text(name).size(12)]);
                    }
                }
                list = list.push(dirs);
            }
        }

        let recommended: Vec<&Card> = catalog::recommended(cid).into_iter().filter(|c| matches(c)).collect();
        if !recommended.is_empty() {
            list = list.push(section_title("RECOMMENDED"));
            for c in recommended {
                list = list.push(row_for(c));
            }
        }
        list = list.push(section_title("OTHER ACTIONS"));
        for (name, cards) in catalog::categories(cid) {
            let cards: Vec<&Card> = cards.into_iter().filter(|c| matches(c)).collect();
            if cards.is_empty() {
                continue;
            }
            let open = self.expanded.contains(name) || !q.is_empty();
            list = list.push(
                button(row![
                    text(name.to_uppercase()).size(12).font(BOLD),
                    Space::new().width(Length::Fill),
                    text(if open { "–" } else { "+" })
                ])
                .style(theme::ghost)
                .padding([8, 12])
                .width(Length::Fill)
                .on_press(Msg::ToggleCategory(name.to_string())),
            );
            if open {
                for c in cards {
                    list = list.push(row_for(c));
                }
            }
        }

        let overridden =
            self.app_profile.is_none() || model::overrides(&self.profiles, self.app_profile.as_deref(), slot);
        let footer = button(
            text(if self.app_profile.is_some() { "USE GLOBAL SETTING" } else { "RESTORE DEFAULT" }).size(12).font(BOLD),
        )
        .style(theme::ghost)
        .padding([8, 12])
        .on_press_maybe(overridden.then(|| Msg::RestoreDefault(slot.to_string())));

        let slot_name = self
            .art
            .get(&d.id)
            .and_then(|a| a.button_hotspots.iter().find(|h| h.slot_id == slot))
            .map(|h| catalog::text(&h.name_key))
            .unwrap_or_else(|| catalog::text(control_name_key(cid)));
        column![
            row![
                text("Actions").size(20).font(BOLD),
                Space::new().width(Length::Fill),
                button(text("✕").size(14)).style(theme::ghost).on_press(Msg::SelectSlot(None))
            ]
            .align_y(Alignment::Center),
            text(slot_name).size(13).color(MUTED),
            text_input("Search", &self.search).on_input(Msg::Search).padding(8).style(theme::search),
            scrollable(list).height(Length::Fill),
            footer,
        ]
        .spacing(12)
        .padding(20)
        .into()
    }

    fn point_scroll_panel<'a>(&'a self, d: &'a api::Device) -> Element<'a, Msg> {
        let scroll_slot = format!("{}_mouse_scroll_wheel_settings", d.slot_prefix);
        let pointer_slot = format!("{}_mouse_settings", d.slot_prefix);
        let selected = self.selected_slot.clone().unwrap_or_else(|| scroll_slot.clone());
        let mut col = column![].spacing(14).padding(20);
        if let Some(cid) = hscroll_slot_cid(&d.slot_prefix, &selected) {
            let card = model::resolve(&self.profiles, None, &scroll_slot).and_then(|a| a.card.as_ref());
            let w = model::scroll_settings(card, d.scroll_defaults.as_ref());
            col = col.push(text("Horizontal scrolling").size(20).font(BOLD));
            col = col.push(
                row![
                    text("Horizontal scrolling").size(14).font(BOLD),
                    Space::new().width(Length::Fill),
                    toggler(model::hscroll_enabled(&w, cid))
                        .on_toggle(move |on| Msg::HScroll(cid, on))
                        .size(16)
                        .style(theme::toggler_style)
                ]
                .align_y(Alignment::Center),
            );
            col = col
                .push(text("Hold the button and use the scroll wheel to scroll horizontally.").size(12).color(MUTED));
        } else if selected == pointer_slot {
            let card = model::resolve(&self.profiles, None, &pointer_slot).and_then(|a| a.card.as_ref());
            let pct = self.drafts.pointer.unwrap_or_else(|| model::pointer_speed_percent(card));
            col = col.push(text("Pointer speed").size(20).font(BOLD));
            col = col.push(labeled_value("Pointer speed", format!("{pct:.0}%")));
            col = col.push(
                slider(0.0..=100.0, pct, Msg::PointerSpeed)
                    .step(1.0_f32)
                    .on_release(Msg::CommitPointer)
                    .style(theme::slider_style),
            );
            if let Some(dpi) = &d.dpi {
                col = col.push(text(format!("Current sensor resolution: {} DPI", dpi.current)).size(12).color(MUTED));
            }
        } else {
            let card = model::resolve(&self.profiles, None, &scroll_slot).and_then(|a| a.card.as_ref());
            let w = model::scroll_settings(card, d.scroll_defaults.as_ref());
            let ss = w.smartshift.clone().unwrap_or_default();
            col = col.push(text("Scroll wheel").size(20).font(BOLD));
            col = col.push(text("Scroll direction").size(14).font(BOLD));
            col = col.push(
                radio("Inverted", true, Some(w.dir == WheelDirection::Natural), Msg::Direction).size(14).text_size(13),
            );
            col = col.push(
                radio("Standard", false, Some(w.dir == WheelDirection::Natural), Msg::Direction).size(14).text_size(13),
            );
            if d.capabilities.smartshift_torque {
                let force = self.drafts.force.unwrap_or(ss.scroll_force as f32);
                col = col.push(labeled_value("Scrolling force", format!("{force:.0}%")));
                col = col.push(
                    slider(1.0..=100.0, force, Msg::ScrollForce)
                        .step(1.0_f32)
                        .on_release(Msg::CommitScroll)
                        .style(theme::slider_style),
                );
            }
            col = col.push(setting_toggle(
                "Smooth scrolling",
                "With smooth scrolling, web pages glide across your screen smoothly making it easy to read and navigate them.",
                w.is_smooth,
                Msg::Smooth,
            ));
            if d.capabilities.smartshift {
                col = col.push(setting_toggle(
                    "SmartShift",
                    "Automatically switches the scroll wheel from line-by-line scrolling to hyper-fast scrolling when you scroll faster.",
                    ss.is_enabled,
                    Msg::SmartShift,
                ));
                if ss.is_enabled {
                    let s = self.drafts.sensitivity.unwrap_or(ss.sensitivity as f32);
                    col = col.push(labeled_value("SENSITIVITY VALUE", format!("{s:.0}%")));
                    col = col.push(
                        slider(0.0..=100.0, s, Msg::Sensitivity)
                            .step(1.0_f32)
                            .on_release(Msg::CommitScroll)
                            .style(theme::slider_style),
                    );
                }
            }
        }
        scrollable(col).into()
    }

    fn settings_view<'a>(&'a self, d: &'a api::Device) -> Element<'a, Msg> {
        let mut about = column![section_title("ABOUT")].spacing(6);
        for fw in &d.firmware {
            about = about.push(text(fw.clone()).size(13));
        }
        if let Some(s) = &d.serial {
            about = about.push(text(format!("Serial number: {s}")).size(13));
        }
        about =
            about.push(text(format!("Model: {}  ·  Connection: {}", d.model_id, d.connection)).size(13).color(MUTED));
        if d.capabilities.easy_switch_hosts > 0 {
            about = about.push(
                text(format!("Easy-Switch channels: {}", d.capabilities.easy_switch_hosts)).size(13).color(MUTED),
            );
        }
        let mut general = column![section_title("GENERAL")].spacing(6);
        if d.kind == "mouse" {
            // System-wide, as in Options+ (applies to every mouse).
            general = general.push(
                row![
                    text("Swap left/right buttons").size(14).font(BOLD),
                    Space::new().width(Length::Fill),
                    toggler(self.buttons_swapped.unwrap_or(false))
                        .on_toggle_maybe(self.buttons_swapped.is_some().then_some(Msg::SwapButtons))
                        .size(16)
                        .style(theme::toggler_style)
                ]
                .align_y(Alignment::Center),
            );
        }
        let col = column![
            about,
            general,
            self.backup_section(),
            column![
                section_title("RESTORE TO DEFAULT"),
                text("Removes all custom button and pointer settings of this device.").size(13).color(MUTED),
                button(text("RESET TO DEFAULT SETTINGS").size(12).font(BOLD))
                    .style(theme::ghost)
                    .padding([8, 0])
                    .on_press(Msg::ResetDevice(d.id.clone())),
            ]
            .spacing(6),
        ]
        .spacing(28)
        .padding(Padding { top: 10.0, right: 40.0, bottom: 20.0, left: 20.0 })
        .width(Length::Fixed(620.0));
        scrollable(col).into()
    }

    fn backup_section(&self) -> Element<'_, Msg> {
        let mut list = column![].spacing(4);
        for b in self.backups.iter().take(8) {
            list = list.push(
                row![
                    text(format!("{}  ·  {}", format_time(b.created), b.label)).size(13).width(Length::Fill),
                    button(text("RESTORE").size(11).font(BOLD))
                        .style(theme::ghost)
                        .padding([4, 8])
                        .on_press(Msg::RestoreBackup(b.path.clone())),
                ]
                .align_y(Alignment::Center),
            );
        }
        column![
            section_title("BACKUPS"),
            text("Backups are stored locally on this computer.").size(13).color(MUTED),
            row![
                button(text("CREATE BACKUP NOW").size(12).font(BOLD))
                    .style(theme::primary)
                    .padding([8, 14])
                    .on_press(Msg::CreateBackup),
                button(text("EXPORT…").size(12).font(BOLD)).style(theme::ghost).padding([8, 14]).on_press(Msg::Export),
                button(text("IMPORT…").size(12).font(BOLD)).style(theme::ghost).padding([8, 14]).on_press(Msg::Import),
            ]
            .spacing(10),
            list,
        ]
        .spacing(8)
        .into()
    }

    fn app_settings(&self) -> Element<'_, Msg> {
        let header = row![
            button(text("←").size(22)).style(theme::ghost).padding([2, 10]).on_press(Msg::Navigate(Page::Home)),
            text("Application Settings").size(26).font(BOLD),
        ]
        .spacing(12)
        .align_y(Alignment::Center)
        .padding(Padding { top: 18.0, right: 20.0, bottom: 4.0, left: 16.0 });
        let body = column![
            column![
                section_title("GENERAL"),
                text(format!("Inputsmith {}", env!("CARGO_PKG_VERSION"))).size(14),
                text(if self.connected { "Agent: running" } else { "Agent: not connected" }).size(13).color(MUTED),
                Space::new().height(6),
                setting_toggle(
                    "Start at login",
                    "Starts inputsmith in the notification area when you sign in, so your devices are configured \
                     without opening this window.",
                    self.autostart,
                    Msg::Autostart,
                ),
            ]
            .spacing(6),
            self.backup_section(),
            column![
                section_title("LOGI OPTIONS+"),
                text(
                    "Copies the button, pointer and scroll settings and application profiles from Logi Options+ on \
                     this computer. Your current settings are backed up first."
                )
                .size(13)
                .color(MUTED),
                button(text("IMPORT FROM LOGI OPTIONS+").size(12).font(BOLD))
                    .style(theme::ghost)
                    .padding([8, 0])
                    .on_press(Msg::ImportOptionsPlus),
            ]
            .spacing(6),
            column![
                section_title("EXIT"),
                text("Closes this window and the tray icon. The agent keeps your devices configured.")
                    .size(13)
                    .color(MUTED),
                button(text("QUIT INPUTSMITH").size(12).font(BOLD))
                    .style(theme::ghost)
                    .padding([8, 0])
                    .on_press(Msg::Quit),
            ]
            .spacing(6),
        ]
        .spacing(28)
        .padding(Padding { top: 10.0, right: 40.0, bottom: 20.0, left: 64.0 })
        .width(Length::Fixed(700.0));
        column![header, scrollable(body)].into()
    }
}

pub fn open_window() -> Task<Msg> {
    let icon = window::icon::from_rgba(crate::window_icon_rgba(), 32, 32).ok();
    let (_, open) = window::open(window::Settings {
        size: iced::Size::new(1216.0, 720.0),
        min_size: Some(iced::Size::new(980.0, 600.0)),
        icon,
        ..Default::default()
    });
    open.map(Msg::WindowOpened)
}

fn banner(s: &str) -> Element<'_, Msg> {
    container(text(s).size(12).color(MUTED)).padding([6, 16]).width(Length::Fill).style(theme::side_panel).into()
}

fn section_title(s: &str) -> Element<'_, Msg> {
    container(text(s).size(12).font(BOLD).color(MUTED))
        .padding(Padding { top: 12.0, right: 0.0, bottom: 4.0, left: 0.0 })
        .into()
}

fn labeled_value(label: &str, value: String) -> Element<'_, Msg> {
    row![text(label).size(14).font(BOLD), Space::new().width(Length::Fill), text(value).size(13).color(ACCENT)].into()
}

fn setting_toggle<'a>(title: &'a str, help: &'a str, value: bool, on: fn(bool) -> Msg) -> Element<'a, Msg> {
    column![
        row![
            text(title).size(14).font(BOLD),
            Space::new().width(Length::Fill),
            toggler(value).on_toggle(on).size(16).style(theme::toggler_style)
        ]
        .align_y(Alignment::Center),
        text(help).size(12).color(MUTED),
    ]
    .spacing(4)
    .into()
}

fn battery_pill(d: &api::Device) -> Element<'_, Msg> {
    let label = match (&d.battery, d.online) {
        (_, false) => "INACTIVE".to_string(),
        (Some(b), true) => match b.percent {
            Some(p) => format!("{p}%{}", if b.charging { " ⚡" } else { "" }),
            None => capitalize(&b.level),
        },
        (None, true) => "Connected".to_string(),
    };
    container(text(label).size(12).font(BOLD)).padding([4, 10]).style(theme::pill).into()
}

fn greeting() -> &'static str {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    // Local offset is unknown without a time zone database; use UTC hour as a
    // coarse approximation unless the TZ offset env var is set.
    let offset: i64 = std::env::var("INPUTSMITH_UTC_OFFSET").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let hour = ((secs as i64 / 3600 + offset).rem_euclid(24)) as u32;
    match hour {
        5..=11 => "Good Morning",
        12..=17 => "Good Afternoon",
        _ => "Good Evening",
    }
}

fn format_time(ts: u64) -> String {
    // Days since epoch -> civil date (Howard Hinnant's algorithm).
    let days = (ts / 86400) as i64;
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let secs = ts % 86400;
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02} UTC", secs / 3600, secs % 3600 / 60)
}

fn on_off(b: bool) -> &'static str {
    if b { "On" } else { "Off" }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn device_glyph(kind: &str) -> &'static str {
    match kind {
        "keyboard" => "⌨",
        _ => "🖱",
    }
}

fn control_name_key(cid: u16) -> &'static str {
    match cid {
        0x52 => "SLOT_NAME_MIDDLE_BUTTON",
        0x53 => "SLOT_NAME_BACK_BUTTON",
        0x56 => "SLOT_NAME_FORWARD_BUTTON",
        0xC3 => "SLOT_NAME_GESTURE_BUTTON",
        0xC4 => "SLOT_NAME_MODESHIFT_BUTTON",
        _ => "SLOT_NAME_BUTTON",
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn formats_backup_time() {
        assert_eq!(super::format_time(0), "1970-01-01 00:00 UTC");
        assert_eq!(super::format_time(1791833229), "2026-10-12 19:27 UTC");
    }
}
