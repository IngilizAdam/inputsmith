//! Device runtime: opens HID++ interfaces, tracks devices behind receivers,
//! applies settings cards, diverts controls and turns their events into
//! actions.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver as EventReceiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;

use inputsmith_hidpp::channel::InterfaceInfo;
use inputsmith_hidpp::device::{self, Device};
use inputsmith_hidpp::features::device_info::{DeviceInformation, DeviceName};
use inputsmith_hidpp::features::dpi::{AdjustableDpi, DpiList};
use inputsmith_hidpp::features::reprog_controls::{self, ControlEvent, ControlInfo, ReportingUpdate, ReprogControls};
use inputsmith_hidpp::features::wheel::{self, HiResMode, HiResWheel, SmartShift, WheelMode, WheelMovement};
use inputsmith_hidpp::receiver::{ConnectionEvent, Receiver, receiver_kind};
use inputsmith_hidpp::report::{DEVICE_INDEX_DIRECT, Report, sub_id};
use inputsmith_hidpp::{Channel, feature_id};
use inputsmith_input::MouseButton;
use inputsmith_ipc::model::{Attribute, Card, MacroType, MouseAction, ScrollMode, ScrollWheelSettings, WheelDirection};

use crate::actions::{DeviceOps, Executor, effective_card};
use crate::devicedb::DeviceDb;
use crate::gesture::{GestureConfig, GestureSession};
use crate::mapping;
use crate::store::{Settings, Store};
use inputsmith_hidpp::features::battery::{Battery, BatteryLevel, BatteryStatus, ChargingStatus};
use inputsmith_hidpp::features::change_host::ChangeHost;
use inputsmith_hidpp::features::config_change::ConfigChange;
use inputsmith_ipc::api;

/// Changes other components (the IPC server) are notified about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentEvent {
    DevicesChanged,
    ProfilesChanged,
}

/// State shared by every worker.
pub struct Shared {
    pub settings: RwLock<Settings>,
    pub store: Store,
    pub executor: Executor,
    pub db: DeviceDb,
    /// Profile of the foreground application, if it has one.
    pub app_profile: RwLock<Option<String>>,
    pub gesture: GestureConfig,
    pub stop: AtomicBool,
    pub registry: Mutex<BTreeMap<String, Arc<DeviceRt>>>,
    /// Every device seen (last snapshot), persisted so that devices that are off are still listed.
    known: Mutex<BTreeMap<String, api::Device>>,
    listeners: Mutex<Vec<Sender<AgentEvent>>>,
}

impl Shared {
    pub fn new(settings: Settings, store: Store, executor: Executor, db: DeviceDb) -> Self {
        let known = store.load_known_devices().into_iter().map(|d| (d.id.clone(), d)).collect();
        Self {
            known: Mutex::new(known),
            settings: RwLock::new(settings),
            store,
            executor,
            db,
            app_profile: RwLock::new(None),
            gesture: GestureConfig::default(),
            stop: AtomicBool::new(false),
            registry: Mutex::new(BTreeMap::new()),
            listeners: Mutex::new(Vec::new()),
        }
    }

    pub fn stopping(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    pub fn subscribe(&self) -> EventReceiver<AgentEvent> {
        let (tx, rx) = mpsc::channel();
        self.listeners.lock().unwrap().push(tx);
        rx
    }

    pub fn emit(&self, ev: AgentEvent) {
        self.listeners.lock().unwrap().retain(|l| l.send(ev).is_ok());
    }

    /// Replaces the settings, persists them and applies them to devices.
    pub fn replace_settings(&self, s: Settings) -> Result<(), crate::store::StoreError> {
        self.store.save(&s)?;
        *self.settings.write().unwrap() = s;
        self.reconfigure_all();
        self.emit(AgentEvent::ProfilesChanged);
        Ok(())
    }

    /// Applies a change to the settings, then persists and applies them.
    pub fn update_settings<R>(&self, f: impl FnOnce(&mut Settings) -> R) -> Result<R, crate::store::StoreError> {
        let (r, snapshot) = {
            let mut s = self.settings.write().unwrap();
            let r = f(&mut s);
            (r, s.clone())
        };
        self.store.save(&snapshot)?;
        self.reconfigure_all();
        self.emit(AgentEvent::ProfilesChanged);
        Ok(r)
    }

    /// Connected devices, then known ones that are not attached (offline, as Options+ lists them).
    pub fn device_list(&self) -> api::DeviceList {
        let mut devices: Vec<api::Device> = self.registry.lock().unwrap().values().map(|d| d.summary()).collect();
        for d in self.known.lock().unwrap().values() {
            if !devices.iter().any(|x| x.id == d.id) {
                devices.push(api::Device { online: false, battery: None, ..d.clone() });
            }
        }
        api::DeviceList { devices }
    }

    /// Adds a device to the registry and remembers it.
    fn register(&self, d: &Arc<DeviceRt>) {
        self.registry.lock().unwrap().insert(d.key.clone(), d.clone());
        let summary = d.summary();
        let mut known = self.known.lock().unwrap();
        if known.get(&d.key) != Some(&summary) {
            known.insert(d.key.clone(), summary);
            let all: Vec<_> = known.values().cloned().collect();
            if let Err(e) = self.store.save_known_devices(&all) {
                log::warn!("saving known devices: {e}");
            }
        }
    }

    /// Forgets a device that is not connected. Its settings are kept.
    pub fn forget_device(&self, id: &str) -> Result<bool, crate::store::StoreError> {
        if self.registry.lock().unwrap().get(id).is_some_and(|d| d.is_online()) {
            return Ok(false);
        }
        self.registry.lock().unwrap().remove(id);
        let all: Vec<_> = {
            let mut known = self.known.lock().unwrap();
            known.remove(id);
            known.values().cloned().collect()
        };
        self.store.save_known_devices(&all)?;
        self.emit(AgentEvent::DevicesChanged);
        Ok(true)
    }

    /// Re-applies settings to every connected device.
    pub fn reconfigure_all(&self) {
        let devices: Vec<_> = self.registry.lock().unwrap().values().cloned().collect();
        let any = !devices.is_empty();
        for d in devices {
            d.configure(self);
        }
        // Applied settings change live device state (e.g. DPI) shown by clients.
        if any {
            self.emit(AgentEvent::DevicesChanged);
        }
    }
}

/// What a control does natively, used when no card overrides it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Native {
    Button(MouseButton),
    SmartShiftToggle,
    Nothing,
}

fn native(cid: u16) -> Native {
    match cid {
        0x50 => Native::Button(MouseButton::Left),
        0x51 => Native::Button(MouseButton::Right),
        0x52 => Native::Button(MouseButton::Middle),
        0x53 => Native::Button(MouseButton::Back),
        0x56 => Native::Button(MouseButton::Forward),
        0xC4 => Native::SmartShiftToggle,
        _ => Native::Nothing,
    }
}

/// Whether `card` reproduces the control's native function, in which case
/// the control need not be diverted.
fn is_native_card(cid: u16, card: &Card) -> bool {
    let card = effective_card(card);
    let Some(m) = card.action.as_ref().filter(|_| card.attribute == Attribute::MacroPlayback) else {
        return false;
    };
    if m.kind != MacroType::Mouse {
        return false;
    }
    let Some(mm) = &m.mouse else { return false };
    let button = match &mm.action {
        MouseAction::Button => MouseButton::from_hid(mm.hid_usage),
        MouseAction::WinBack => Some(MouseButton::Back),
        MouseAction::WinForward => Some(MouseButton::Forward),
        _ => None,
    };
    button.is_some_and(|b| native(cid) == Native::Button(b))
}

fn is_gesture_card(card: &Card) -> bool {
    effective_card(card).attribute == Attribute::Adapter4Ways
}

enum Active {
    Card(Box<Card>),
    Native(Native),
    Gesture(Box<GestureSession>),
}

/// Scroll wheel behaviour derived from the scroll wheel settings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ScrollConfig {
    /// Wheel diverted; the agent injects the scrolling.
    smooth: bool,
    /// Direction `NATURAL` (shown as "Inverted").
    natural: bool,
    /// Controls that scroll horizontally while held with the wheel.
    hscroll: Vec<u16>,
}

#[derive(Default)]
struct State {
    pressed: Vec<u16>,
    active: HashMap<u16, Active>,
    diverted: Vec<u16>,
    scroll: ScrollConfig,
    /// Held horizontal-scroll controls and whether the wheel moved meanwhile.
    hscroll_held: HashMap<u16, bool>,
    /// Cookie written after the last complete configuration.
    cookie: Option<u16>,
}

struct DpiInfo {
    list: DpiList,
    default: u16,
}

pub struct DeviceRt {
    pub key: String,
    pub name: String,
    pub model_id: String,
    pub extended_model_id: u8,
    pub serial: Option<String>,
    pub slot_prefix: String,
    pub kind: String,
    pub firmware: Vec<String>,
    online: AtomicBool,
    battery: Mutex<Option<BatteryStatus>>,
    hosts: u8,
    dev: Device,
    controls: Vec<ControlInfo>,
    rc_index: Option<u8>,
    wheel_index: Option<u8>,
    wheel_multiplier: u8,
    dpi: Option<DpiInfo>,
    scroll_defaults: Option<ScrollWheelSettings>,
    state: Mutex<State>,
    /// Writes that failed during the current [`Self::configure`] run.
    config_errors: AtomicU32,
}

impl DeviceRt {
    pub fn open(channel: Channel, index: u8, db: &DeviceDb) -> inputsmith_hidpp::Result<Self> {
        let dev = Device::open(channel, index)?;
        let fw_name = DeviceName(&dev).name().unwrap_or_default();
        let (model_id, extended_model_id, serial) = match DeviceInformation(&dev).device_info() {
            Ok(i) => {
                let serial = if i.has_serial_number() { DeviceInformation(&dev).serial_number().ok() } else { None };
                (i.model_id(), i.extended_model_id, serial)
            }
            Err(_) => (format!("{:04x}", dev.channel().info().product_id), 0, None),
        };
        let kind = DeviceName(&dev).device_type().map(|t| format!("{t:?}").to_lowercase()).unwrap_or_default();
        let firmware = DeviceInformation(&dev)
            .all_firmware()
            .map(|v| v.iter().map(|f| format!("{:?} {}", f.kind, f.version_string())).collect())
            .unwrap_or_default();
        let battery = Battery(&dev).status().ok();
        let hosts = ChangeHost(&dev).host_info().map_or(0, |h| h.host_count);
        let controls = if dev.has_feature(feature_id::SPECIAL_KEYS_1B04) {
            ReprogControls(&dev).controls().unwrap_or_default()
        } else {
            Vec::new()
        };
        let rc_index = dev.feature(feature_id::SPECIAL_KEYS_1B04).map(|f| f.index);
        let dpi = dev.has_feature(feature_id::ADJUSTABLE_DPI).then(|| {
            let d = AdjustableDpi(&dev);
            Some(DpiInfo { list: d.dpi_list(0).ok()?, default: d.dpi(0).ok()?.1 })
        });
        let wheel_index = dev.feature(feature_id::HIRES_WHEEL).map(|f| f.index);
        let wheel_multiplier = HiResWheel(&dev).capabilities().map_or(1, |c| c.multiplier.max(1));
        let key = serial.clone().unwrap_or_else(|| format!("{}#{index}", dev.channel().info().container_id));
        Ok(Self {
            key,
            name: db.display_name(&model_id, &fw_name),
            scroll_defaults: db.scroll_defaults(&model_id),
            slot_prefix: db.slot_prefix(&model_id, &fw_name),
            model_id,
            extended_model_id,
            serial,
            kind,
            firmware,
            online: AtomicBool::new(true),
            battery: Mutex::new(battery),
            hosts,
            dev,
            controls,
            rc_index,
            wheel_index,
            wheel_multiplier,
            dpi: dpi.flatten(),
            state: Mutex::new(State::default()),
            config_errors: AtomicU32::new(0),
        })
    }

    pub fn index(&self) -> u8 {
        self.dev.index()
    }

    pub fn set_online(&self, online: bool) {
        self.online.store(online, Ordering::Relaxed);
    }

    pub fn is_online(&self) -> bool {
        self.online.load(Ordering::Relaxed)
    }

    /// Snapshot for the UI.
    pub fn summary(&self) -> api::Device {
        let has = |id| self.dev.has_feature(id);
        let dpi = self.dpi.as_ref().and_then(|d| match d.list {
            DpiList::Range { min, max, step } => {
                Some(api::Dpi { min, max, step, default: d.default, current: self.current_dpi().unwrap_or(d.default) })
            }
            DpiList::Values(ref v) => Some(api::Dpi {
                min: *v.first()?,
                max: *v.last()?,
                step: 0,
                default: d.default,
                current: self.current_dpi().unwrap_or(d.default),
            }),
        });
        api::Device {
            id: self.key.clone(),
            name: self.name.clone(),
            model_id: self.model_id.clone(),
            extended_model_id: self.extended_model_id,
            slot_prefix: self.slot_prefix.clone(),
            kind: self.kind.clone(),
            online: self.online.load(Ordering::Relaxed),
            serial: self.serial.clone(),
            firmware: self.firmware.clone(),
            connection: if self.dev.index() == DEVICE_INDEX_DIRECT { "direct".into() } else { "receiver".into() },
            battery: self.battery.lock().unwrap().map(|b| api::Battery {
                percent: b.percent,
                level: match b.level {
                    BatteryLevel::Critical => "critical",
                    BatteryLevel::Low => "low",
                    BatteryLevel::Good => "good",
                    BatteryLevel::Full => "full",
                    BatteryLevel::Unknown => "unknown",
                }
                .into(),
                charging: matches!(b.charging, ChargingStatus::Charging | ChargingStatus::ChargingSlow),
            }),
            dpi,
            controls: self
                .controls
                .iter()
                .map(|c| api::Control {
                    cid: c.cid,
                    slot_id: self.slot(c.cid),
                    reprogrammable: c.flags.reprogrammable(),
                    divertable: c.flags.divertable(),
                    gesture_capable: c.flags.raw_xy(),
                    horizontal_scroll: c.flags.raw_wheel(),
                })
                .collect(),
            capabilities: api::Capabilities {
                smartshift: has(feature_id::SMART_SHIFT) || has(feature_id::SMART_SHIFT_TUNABLE),
                smartshift_torque: has(feature_id::SMART_SHIFT_TUNABLE),
                hires_wheel: has(feature_id::HIRES_WHEEL),
                thumbwheel: has(feature_id::THUMBWHEEL),
                fn_inversion: has(feature_id::FN_INVERSION)
                    || has(feature_id::FN_INVERSION_2)
                    || has(feature_id::FN_INVERSION_3),
                backlight: has(feature_id::BACKLIGHT),
                easy_switch_hosts: self.hosts,
            },
            scroll_defaults: self.scroll_defaults.clone(),
        }
    }

    fn slot(&self, cid: u16) -> String {
        format!("{}_c{}", self.slot_prefix, cid)
    }

    /// Applies settings cards and control diversion from the current settings, then marks the
    /// device as configured with a cookie derived from those settings (see [`Self::reconnect`]).
    pub fn configure(&self, shared: &Shared) {
        let settings = shared.settings.read().unwrap();
        self.config_errors.store(0, Ordering::Relaxed);
        self.apply_settings_cards(&settings);
        self.apply_diversion(&settings);
        let cookie = self.config_cookie(&settings);
        let ok = self.config_errors.load(Ordering::Relaxed) == 0
            && self.dev.has_feature(feature_id::CONFIG_CHANGE)
            && ConfigChange(&self.dev).set_cookie(cookie).is_ok();
        self.state.lock().unwrap().cookie = ok.then_some(cookie);
    }

    /// After a link-up: skips reconfiguring when the device still carries the cookie written after
    /// our last complete configuration with the same settings (the device kept its state, e.g. after
    /// sleeping). Power loss resets the cookie, another host's software overwrites it.
    pub fn reconnect(&self, shared: &Shared) {
        let expected = self.state.lock().unwrap().cookie;
        let wanted = self.config_cookie(&shared.settings.read().unwrap());
        if expected == Some(wanted) && self.dev.has_feature(feature_id::CONFIG_CHANGE) {
            match ConfigChange(&self.dev).cookie() {
                Ok(c) if c == wanted => {
                    // Belt and braces: the diversions must still be there too.
                    let diverted = self.state.lock().unwrap().diverted.clone();
                    let rc = ReprogControls(&self.dev);
                    if diverted.iter().all(|&cid| rc.reporting(cid).is_ok_and(|r| r.diverted)) {
                        log::info!("{}: configuration intact (cookie {c:#06x})", self.name);
                        return;
                    }
                    log::info!("{}: cookie {c:#06x} kept but diversion lost, reconfiguring", self.name);
                }
                Ok(c) => log::info!("{}: configuration lost (cookie {c:#06x}), reconfiguring", self.name),
                Err(e) => log::debug!("{}: reading cookie: {e}", self.name),
            }
        }
        self.configure(shared);
    }

    /// Cookie identifying the settings this device is configured with. Never 0 (reset value) or
    /// 0x4825 (Options+).
    fn config_cookie(&self, settings: &Settings) -> u16 {
        let mut parts = vec![self.slot_prefix.clone()];
        for suffix in ["mouse_settings", "mouse_scroll_wheel_settings"] {
            let a = settings.resolve(None, &format!("{}_{suffix}", self.slot_prefix));
            parts.push(serde_json::to_string(&a.and_then(|a| a.card.as_ref())).unwrap_or_default());
        }
        parts.push(serde_json::to_string(&self.scroll_defaults).unwrap_or_default());
        for c in &self.controls {
            for card in settings.cards_for_slot(&self.slot(c.cid)) {
                parts.push(serde_json::to_string(card).unwrap_or_default());
            }
        }
        mapping::cookie(parts.iter().map(String::as_str))
    }

    fn apply_settings_cards(&self, settings: &Settings) {
        let prefix = &self.slot_prefix;
        let card = |suffix: &str| settings.resolve(None, &format!("{prefix}_{suffix}")).and_then(|a| a.card.clone());

        if let Some(DpiInfo { list: DpiList::Range { min, max, step }, default }) = &self.dpi {
            // Without a pointer speed card the device's default resolution applies (Options+ does the same).
            let value = card("mouse_settings")
                .and_then(|c| c.mouse_settings)
                .and_then(|m| m.pointer_speed)
                .and_then(|p| p.active)
                .map_or(0.0, |a| a.value);
            let target = mapping::pointer_speed_to_dpi(value as f64, *min, *default, *max, *step);
            match AdjustableDpi(&self.dev).set_dpi(0, target) {
                Ok(()) => log::info!("{}: DPI {target} (pointer speed {value:.2})", self.name),
                Err(e) => {
                    self.config_errors.fetch_add(1, Ordering::Relaxed);
                    log::warn!("{}: set DPI failed: {e}", self.name);
                }
            }
        }

        let scroll = self.scroll_settings(settings);
        if let Some(ws) = &scroll {
            if let Some(ss) = ws.smartshift.as_ref().filter(|_| {
                self.dev.has_feature(feature_id::SMART_SHIFT) || self.dev.has_feature(feature_id::SMART_SHIFT_TUNABLE)
            }) {
                let mode = match &ss.mode {
                    ScrollMode::Freespin => Some(WheelMode::FreeSpin),
                    ScrollMode::Ratchet => Some(WheelMode::Ratchet),
                    ScrollMode::Unchanged | ScrollMode::Other(_) => None,
                };
                // 0xFF keeps the wheel in its mode: SmartShift switching disabled.
                let threshold = if ss.is_enabled { mapping::smartshift_auto_disengage(ss.sensitivity) } else { 0xFF };
                let torque = ss.is_scroll_force_enabled.then_some(ss.scroll_force.clamp(1, 100) as u8);
                match SmartShift(&self.dev).set(mode, Some(threshold), torque) {
                    Ok(()) => {
                        log::info!("{}: SmartShift {:?} threshold {threshold} torque {torque:?}", self.name, mode)
                    }
                    Err(e) => {
                        self.config_errors.fetch_add(1, Ordering::Relaxed);
                        log::warn!("{}: SmartShift failed: {e}", self.name);
                    }
                }
            }
        }
        self.apply_scroll(scroll.as_ref());
    }

    /// Scroll wheel settings: the profile's card, else the model's defaults.
    fn scroll_settings(&self, settings: &Settings) -> Option<ScrollWheelSettings> {
        settings
            .resolve(None, &format!("{}_mouse_scroll_wheel_settings", self.slot_prefix))
            .and_then(|a| a.card.as_ref())
            .and_then(|c| c.mouse_scroll_wheel_settings.clone())
            .or_else(|| self.scroll_defaults.clone())
    }

    /// Wheel mode as Options+ sets it: smooth scrolling diverts the wheel (direction is then applied
    /// when injecting), otherwise the native wheel with the device's invert bit. Never high resolution.
    fn apply_scroll(&self, ws: Option<&ScrollWheelSettings>) {
        let natural = ws.is_some_and(|w| w.dir == WheelDirection::Natural);
        let smooth = ws.is_some_and(|w| w.is_smooth) && self.wheel_index.is_some();
        let hscroll = ws
            .and_then(|w| w.virtual_thumbwheel.as_ref())
            .map(|v| {
                v.keys
                    .iter()
                    .filter(|k| k.enabled)
                    .filter_map(|k| u16::try_from(k.control_id).ok())
                    .filter(|cid| self.controls.iter().any(|c| c.cid == *cid && c.flags.raw_wheel()))
                    .collect()
            })
            .unwrap_or_default();
        if self.wheel_index.is_some() {
            let mode = HiResMode { diverted: smooth, high_resolution: false, inverted: natural && !smooth };
            match HiResWheel(&self.dev).set_mode(mode) {
                Ok(()) => log::info!("{}: wheel {mode:?}", self.name),
                Err(e) => {
                    self.config_errors.fetch_add(1, Ordering::Relaxed);
                    log::warn!("{}: wheel mode failed: {e}", self.name);
                }
            }
        }
        self.state.lock().unwrap().scroll = ScrollConfig { smooth, natural, hscroll };
    }

    fn apply_diversion(&self, settings: &Settings) {
        let Some(_) = self.rc_index else { return };
        let rc = ReprogControls(&self.dev);
        let hscroll = self.state.lock().unwrap().scroll.hscroll.clone();
        let mut diverted = Vec::new();
        for c in &self.controls {
            // Options+ makes the device report every press of every control ("analytics key events",
            // including left/right click) and the device keeps that until it loses power. We have no use
            // for it, so it is switched off wherever the device supports it.
            let no_analytics = c.flags.analytics_key_events().then_some(false);
            // Never take over the primary buttons.
            if !c.flags.divertable() || matches!(c.cid, 0x50 | 0x51) {
                if no_analytics.is_some() {
                    let update = ReportingUpdate { analytics_key_events: no_analytics, ..Default::default() };
                    if let Err(e) = rc.update_reporting(c.cid, update) {
                        log::debug!("{}: analytics off for {:#x}: {e}", self.name, c.cid);
                    }
                }
                continue;
            }
            let slot = self.slot(c.cid);
            let cards: Vec<&Card> = settings.cards_for_slot(&slot).collect();
            let horizontal = hscroll.contains(&c.cid);
            let divert = horizontal || cards.iter().any(|card| !is_native_card(c.cid, card));
            let raw_xy = divert && c.flags.raw_xy() && cards.iter().any(|card| is_gesture_card(card));
            let update = ReportingUpdate {
                divert: Some(divert),
                raw_xy: c.flags.raw_xy().then_some(raw_xy),
                raw_wheel: c.flags.raw_wheel().then_some(horizontal),
                analytics_key_events: no_analytics,
            };
            match rc.update_reporting(c.cid, update) {
                Ok(()) => {
                    if divert {
                        diverted.push(c.cid);
                    }
                }
                Err(e) => {
                    self.config_errors.fetch_add(1, Ordering::Relaxed);
                    log::warn!("{}: diversion of {:#x} failed: {e}", self.name, c.cid);
                }
            }
        }
        log::info!("{}: diverted controls {:x?}", self.name, diverted);
        self.state.lock().unwrap().diverted = diverted;
    }

    /// Restores native behaviour of every diverted control.
    pub fn release_controls(&self) {
        let (diverted, scroll) = {
            let mut st = self.state.lock().unwrap();
            (std::mem::take(&mut st.diverted), std::mem::take(&mut st.scroll))
        };
        let rc = ReprogControls(&self.dev);
        for c in self.controls.iter().filter(|c| diverted.contains(&c.cid)) {
            let update = ReportingUpdate {
                divert: Some(false),
                raw_xy: c.flags.raw_xy().then_some(false),
                raw_wheel: c.flags.raw_wheel().then_some(false),
                analytics_key_events: None,
            };
            let _ = rc.update_reporting(c.cid, update);
        }
        // The device no longer has our configuration.
        if self.state.lock().unwrap().cookie.take().is_some() {
            let _ = ConfigChange(&self.dev).set_cookie(0);
        }
        // A diverted wheel would not scroll at all without us.
        if scroll.smooth {
            let _ = HiResWheel(&self.dev).set_mode(HiResMode { inverted: scroll.natural, ..Default::default() });
        }
    }

    pub fn handle_report(&self, r: &Report, shared: &Shared) {
        if let Some(b) = Battery(&self.dev).parse_event(r) {
            let changed = self.battery.lock().unwrap().replace(b) != Some(b);
            if changed {
                log::info!("{}: battery {:?}% {:?}", self.name, b.percent, b.charging);
                shared.emit(AgentEvent::DevicesChanged);
            }
            return;
        }
        if let Some(m) = self.wheel_index.and_then(|i| wheel::parse_wheel_movement(i, r)) {
            self.on_wheel(m, shared);
            return;
        }
        let Some(rc) = self.rc_index else { return };
        let Some(ev) = reprog_controls::parse_event(rc, r) else { return };
        match ev {
            ControlEvent::Buttons(now) => {
                let prev = std::mem::replace(&mut self.state.lock().unwrap().pressed, now.clone());
                for &cid in now.iter().filter(|c| !prev.contains(c)) {
                    self.on_press(cid, shared);
                }
                for &cid in prev.iter().filter(|c| !now.contains(c)) {
                    self.on_release(cid, shared);
                }
            }
            ControlEvent::RawXY { dx, dy } => {
                let now = crate::gesture::now_ms();
                let mut st = self.state.lock().unwrap();
                for a in st.active.values_mut() {
                    if let Active::Gesture(g) = a {
                        g.on_move(dx as i32, dy as i32, now, &shared.executor, self);
                    }
                }
            }
            ControlEvent::RawWheel { delta } => {
                let natural = {
                    let mut st = self.state.lock().unwrap();
                    if st.hscroll_held.is_empty() {
                        return;
                    }
                    st.hscroll_held.values_mut().for_each(|scrolled| *scrolled = true);
                    st.scroll.natural
                };
                let _ = shared.executor.sink().wheel(0, mapping::hscroll_units(delta, natural));
            }
        }
    }

    /// Diverted wheel (smooth scrolling): one injected event per report, as Options+ does.
    fn on_wheel(&self, m: WheelMovement, shared: &Shared) {
        let natural = self.state.lock().unwrap().scroll.natural;
        let per_notch = if m.high_resolution { self.wheel_multiplier } else { 1 };
        let _ = shared.executor.sink().wheel(mapping::wheel_units(m.delta, per_notch, natural), 0);
    }

    fn on_press(&self, cid: u16, shared: &Shared) {
        {
            let mut st = self.state.lock().unwrap();
            if st.scroll.hscroll.contains(&cid) {
                // The button's own action runs at release, unless it was used to scroll.
                st.hscroll_held.insert(cid, false);
                return;
            }
        }
        self.start_action(cid, shared);
    }

    fn on_release(&self, cid: u16, shared: &Shared) {
        let held = self.state.lock().unwrap().hscroll_held.remove(&cid);
        match held {
            Some(false) => {
                self.start_action(cid, shared);
                self.end_action(cid, shared);
            }
            Some(true) => {}
            None => self.end_action(cid, shared),
        }
    }

    fn start_action(&self, cid: u16, shared: &Shared) {
        let card = {
            let settings = shared.settings.read().unwrap();
            let app = shared.app_profile.read().unwrap();
            let app_profile = app.as_ref().and_then(|id| settings.profiles.iter().find(|p| &p.id == id));
            settings.resolve(app_profile, &self.slot(cid)).and_then(|a| a.card.clone())
        };
        log::debug!("{}: press {cid:#x} -> {:?}", self.name, card.as_ref().map(|c| &c.id));
        let active = match card {
            Some(c) if is_gesture_card(&c) => {
                Active::Gesture(Box::new(GestureSession::new(effective_card(&c).clone(), shared.gesture)))
            }
            Some(c) => {
                shared.executor.press(&c, self);
                Active::Card(Box::new(c))
            }
            None => {
                let n = native(cid);
                match n {
                    Native::Button(b) => {
                        let _ = shared.executor.sink().mouse_button(b, true);
                    }
                    Native::SmartShiftToggle => self.toggle_smartshift(),
                    Native::Nothing => {}
                }
                Active::Native(n)
            }
        };
        self.state.lock().unwrap().active.insert(cid, active);
    }

    fn end_action(&self, cid: u16, shared: &Shared) {
        log::debug!("{}: release {cid:#x}", self.name);
        let Some(active) = self.state.lock().unwrap().active.remove(&cid) else { return };
        match active {
            Active::Card(c) => shared.executor.release(&c),
            Active::Gesture(g) => g.on_release(&shared.executor, self),
            Active::Native(Native::Button(b)) => {
                let _ = shared.executor.sink().mouse_button(b, false);
            }
            Active::Native(_) => {}
        }
    }

    fn set_dpi_snapped(&self, target: f64) {
        let Some(dpi) = &self.dpi else { return };
        let v = match &dpi.list {
            DpiList::Range { min, max, step } => {
                let s = (*step).max(1) as f64;
                (*min as f64 + ((target - *min as f64) / s).round() * s).clamp(*min as f64, *max as f64) as u16
            }
            DpiList::Values(vals) => {
                *vals.iter().min_by_key(|&&x| (x as f64 - target).abs() as u32).unwrap_or(&dpi.default)
            }
        };
        match AdjustableDpi(&self.dev).set_dpi(0, v) {
            Ok(()) => log::info!("{}: DPI -> {v}", self.name),
            Err(e) => log::warn!("{}: DPI change failed: {e}", self.name),
        }
    }

    fn current_dpi(&self) -> Option<u16> {
        AdjustableDpi(&self.dev).dpi(0).ok().map(|d| d.0)
    }
}

impl DeviceOps for DeviceRt {
    fn dpi_step(&self, delta: i32) {
        if let Some(cur) = self.current_dpi() {
            self.set_dpi_snapped(cur as f64 * 1.25f64.powi(delta));
        }
    }

    fn dpi_cycle(&self) {
        let (Some(dpi), Some(cur)) = (&self.dpi, self.current_dpi()) else { return };
        let presets = [dpi.default as f64 / 2.0, dpi.default as f64, dpi.default as f64 * 2.0];
        let next = presets.iter().copied().find(|&p| p > cur as f64 + 1.0).unwrap_or(presets[0]);
        self.set_dpi_snapped(next);
    }

    fn dpi_default(&self) {
        if let Some(d) = &self.dpi {
            self.set_dpi_snapped(d.default as f64);
        }
    }

    fn toggle_smartshift(&self) {
        let ss = SmartShift(&self.dev);
        if let Ok(st) = ss.get() {
            let next = if st.mode == WheelMode::Ratchet { WheelMode::FreeSpin } else { WheelMode::Ratchet };
            let _ = ss.set(Some(next), None, None);
        }
    }
}

/// Runs one HID++ interface until it disappears or the agent stops.
pub fn run_channel(info: InterfaceInfo, shared: Arc<Shared>) {
    let channel = match Channel::open(&info) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("cannot open {:04x}:{:04x}: {e}", info.vendor_id, info.product_id);
            return;
        }
    };
    let rx = channel.subscribe();
    let mut devices: HashMap<u8, Arc<DeviceRt>> = HashMap::new();
    let is_receiver = receiver_kind(info.product_id).is_some();

    let attach = |index: u8, devices: &mut HashMap<u8, Arc<DeviceRt>>| {
        for attempt in 0..5 {
            if shared.stopping() {
                return;
            }
            match DeviceRt::open(channel.clone(), index, &shared.db) {
                Ok(d) => {
                    let d = Arc::new(d);
                    log::info!(
                        "device online: {} [{}] slot prefix {} (index {index})",
                        d.name,
                        d.model_id,
                        d.slot_prefix
                    );
                    d.configure(&shared);
                    shared.register(&d);
                    devices.insert(index, d);
                    shared.emit(AgentEvent::DevicesChanged);
                    return;
                }
                Err(e) => {
                    log::debug!("open index {index} attempt {attempt}: {e}");
                    thread::sleep(Duration::from_millis(400));
                }
            }
        }
    };

    if is_receiver {
        let rcv = Receiver::new(channel.clone());
        if let Err(e) = rcv.enable_notifications() {
            log::warn!("receiver notifications: {e}");
        }
        log::info!("receiver {:04x} ({:?}) attached", info.product_id, rcv.kind());
        for i in 1..=rcv.max_devices() {
            if matches!(device::ping(&channel, i, Duration::from_millis(800)), Ok(Some(v)) if v.major >= 2) {
                attach(i, &mut devices);
            }
        }
        // Sleeping devices answer nothing; they are attached on their next
        // link-up notification. This also announces devices we missed.
        if let Err(e) = rcv.trigger_device_arrival() {
            log::debug!("device arrival trigger: {e}");
        }
    } else {
        attach(DEVICE_INDEX_DIRECT, &mut devices);
    }

    loop {
        if shared.stopping() {
            break;
        }
        match rx.recv_timeout(Duration::from_millis(300)) {
            Ok(r) => {
                if is_receiver && r.sub_id() == sub_id::DEVICE_CONNECTION {
                    if let Some(ev) = ConnectionEvent::parse(&r) {
                        if ev.link_established {
                            log::info!("link up on index {} (wpid {:04x})", ev.index, ev.wpid);
                            // A known device that reconnected keeps its feature table but loses
                            // volatile state (diversion, SmartShift), so only reconfigure it.
                            let known = devices.get(&ev.index).cloned();
                            let reachable = |d: &DeviceRt| {
                                matches!(device::ping(&channel, d.index(), Duration::from_millis(800)), Ok(Some(_)))
                            };
                            match known {
                                Some(d) if reachable(&d) => {
                                    d.set_online(true);
                                    d.reconnect(&shared);
                                    // Re-registers it in case it was forgotten while off.
                                    shared.register(&d);
                                    shared.emit(AgentEvent::DevicesChanged);
                                }
                                _ => {
                                    if let Some(old) = devices.remove(&ev.index) {
                                        shared.registry.lock().unwrap().remove(&old.key);
                                    }
                                    attach(ev.index, &mut devices);
                                }
                            }
                        } else {
                            log::info!("link down on index {}", ev.index);
                            if let Some(d) = devices.get(&ev.index) {
                                d.set_online(false);
                                shared.emit(AgentEvent::DevicesChanged);
                            }
                        }
                    }
                    continue;
                }
                if let Some(d) = devices.get(&r.device_index()) {
                    d.handle_report(&r, &shared);
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if channel.is_closed() {
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    for d in devices.values() {
        if !channel.is_closed() {
            d.release_controls();
        }
        shared.registry.lock().unwrap().remove(&d.key);
    }
    channel.close();
    shared.emit(AgentEvent::DevicesChanged);
    log::info!("interface {:04x}:{:04x} detached", info.vendor_id, info.product_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(v: serde_json::Value) -> Card {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn native_cards_are_not_diverted() {
        let middle = card(serde_json::json!({"attribute": "MACRO_PLAYBACK",
            "macro": {"type": "MOUSE", "mouse": {"action": "BUTTON", "hidUsage": 3}}}));
        let back = card(serde_json::json!({"attribute": "MACRO_PLAYBACK",
            "macro": {"type": "MOUSE", "mouse": {"action": "WIN_BACK"}}}));
        let copy = card(serde_json::json!({"attribute": "MACRO_PLAYBACK",
            "macro": {"type": "KEYSTROKE", "keystroke": {"code": 6, "modifiers": [224]}}}));
        assert!(is_native_card(0x52, &middle));
        assert!(is_native_card(0x53, &back));
        assert!(!is_native_card(0x56, &back));
        assert!(!is_native_card(0x52, &copy));
    }
}
