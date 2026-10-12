//! A HID++ channel: one physical interface (receiver or direct device) with its
//! short/long/very-long collections, request/response matching and a
//! notification fan-out.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use inputsmith_hid::{Bus, DeviceInfo, HidDevice};

use crate::error::{Error, Hidpp10Error, Hidpp20Error, Result};
use crate::report::{Report, ReportKind, sub_id};

pub const LOGITECH_VID: u16 = 0x046D;
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);
/// Software ID stamped on our HID++ 2.0 requests. Notifications use 0 and other
/// host software (such as Options+) uses its own, so responses can be told apart on a shared handle.
pub const DEFAULT_SW_ID: u8 = 0x0E;

/// Usages identifying HID++ collections.
fn collection_kind(usage_page: u16, usage: u16) -> Option<ReportKind> {
    match (usage_page, usage) {
        (0xFF00, 0x0001) => Some(ReportKind::Short),
        (0xFF00, 0x0002) => Some(ReportKind::Long),
        // Bluetooth LE devices expose a long-only HID++ collection here.
        (0xFF43, 0x0202) => Some(ReportKind::Long),
        (0xFF43, 0x0204) => Some(ReportKind::VeryLong),
        _ => None,
    }
}

/// Static description of a HID++ interface found during enumeration.
#[derive(Debug, Clone)]
pub struct InterfaceInfo {
    pub vendor_id: u16,
    pub product_id: u16,
    pub bus: Bus,
    pub product: Option<String>,
    pub container_id: String,
    pub interface_number: Option<u8>,
    /// Collections by report kind.
    pub collections: BTreeMap<ReportKindKey, DeviceInfo>,
}

/// `ReportKind` ordering key for the collections map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReportKindKey {
    Short,
    Long,
    VeryLong,
}

impl From<ReportKind> for ReportKindKey {
    fn from(k: ReportKind) -> Self {
        match k {
            ReportKind::Short => Self::Short,
            ReportKind::Long => Self::Long,
            ReportKind::VeryLong => Self::VeryLong,
        }
    }
}

impl InterfaceInfo {
    pub fn supports(&self, kind: ReportKind) -> bool {
        self.collections.contains_key(&kind.into())
    }
}

/// Finds every HID++-capable interface of Logitech devices.
pub fn enumerate() -> Result<Vec<InterfaceInfo>> {
    let mut groups: BTreeMap<String, InterfaceInfo> = BTreeMap::new();
    for d in inputsmith_hid::enumerate(Some(LOGITECH_VID))? {
        for &(page, usage) in &d.collections {
            let Some(kind) = collection_kind(page, usage) else { continue };
            let g = groups.entry(d.container_id.clone()).or_insert_with(|| InterfaceInfo {
                vendor_id: d.vendor_id,
                product_id: d.product_id,
                bus: d.bus,
                product: d.product.clone(),
                container_id: d.container_id.clone(),
                interface_number: d.interface_number,
                collections: BTreeMap::new(),
            });
            g.collections.insert(kind.into(), d.clone());
        }
    }
    Ok(groups.into_values().collect())
}

type Matcher = Box<dyn Fn(&Report) -> bool + Send>;

struct Waiter {
    id: u64,
    matcher: Matcher,
    tx: SyncSender<Report>,
}

struct Inner {
    info: InterfaceInfo,
    writers: BTreeMap<ReportKindKey, Arc<HidDevice>>,
    waiters: Mutex<Vec<Waiter>>,
    subscribers: Mutex<Vec<Sender<Report>>>,
    closed: AtomicBool,
    next_id: AtomicU64,
    sw_id: u8,
}

impl Inner {
    fn dispatch(&self, report: Report) {
        log::trace!("<- {report:?}");
        {
            let mut waiters = self.waiters.lock().unwrap();
            if let Some(pos) = waiters.iter().position(|w| (w.matcher)(&report)) {
                let w = waiters.remove(pos);
                let _ = w.tx.try_send(report);
                return;
            }
        }
        self.subscribers.lock().unwrap().retain(|s| s.send(report.clone()).is_ok());
    }
}

/// Shared handle to an open HID++ interface.
#[derive(Clone)]
pub struct Channel {
    inner: Arc<Inner>,
}

impl Channel {
    pub fn open(info: &InterfaceInfo) -> Result<Self> {
        if info.collections.is_empty() {
            return Err(Error::NoInterface);
        }
        // Collections sharing a path (Linux hidraw) share one handle.
        let mut by_path: BTreeMap<String, Arc<HidDevice>> = BTreeMap::new();
        let mut writers = BTreeMap::new();
        for (kind, dev) in &info.collections {
            let h = match by_path.get(&dev.path) {
                Some(h) => h.clone(),
                None => {
                    let h = Arc::new(HidDevice::open(dev)?);
                    by_path.insert(dev.path.clone(), h.clone());
                    h
                }
            };
            writers.insert(*kind, h);
        }
        let inner = Arc::new(Inner {
            info: info.clone(),
            writers,
            waiters: Mutex::new(Vec::new()),
            subscribers: Mutex::new(Vec::new()),
            closed: AtomicBool::new(false),
            next_id: AtomicU64::new(1),
            sw_id: DEFAULT_SW_ID,
        });
        for (path, dev) in by_path {
            let weak = Arc::downgrade(&inner);
            thread::Builder::new()
                .name(format!("hidpp-rx {path}"))
                .spawn(move || reader_loop(weak, dev))
                .expect("spawn reader thread");
        }
        Ok(Self { inner })
    }

    pub fn info(&self) -> &InterfaceInfo {
        &self.inner.info
    }

    pub fn sw_id(&self) -> u8 {
        self.inner.sw_id
    }

    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::Relaxed)
    }

    /// Subscribes to every report that is not consumed as a response.
    pub fn subscribe(&self) -> Receiver<Report> {
        let (tx, rx) = mpsc::channel();
        self.inner.subscribers.lock().unwrap().push(tx);
        rx
    }

    /// Picks a supported report kind for a request: long-only interfaces get
    /// short requests promoted.
    pub fn effective_kind(&self, wanted: ReportKind) -> ReportKind {
        let info = &self.inner.info;
        if info.supports(wanted) {
            wanted
        } else if info.supports(ReportKind::Long) {
            ReportKind::Long
        } else if info.supports(ReportKind::Short) {
            ReportKind::Short
        } else {
            ReportKind::VeryLong
        }
    }

    pub fn send(&self, report: &Report) -> Result<()> {
        let key: ReportKindKey = report.kind().into();
        let dev = self.inner.writers.get(&key).ok_or(Error::NoInterface)?;
        log::trace!("-> {report:?}");
        dev.write(report.as_bytes())?;
        Ok(())
    }

    /// Sends `req` and waits for a report satisfying `matcher`.
    pub fn request_with(&self, req: &Report, matcher: Matcher, timeout: Duration) -> Result<Report> {
        if self.is_closed() {
            return Err(Error::Closed);
        }
        let (tx, rx) = mpsc::sync_channel(1);
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        self.inner.waiters.lock().unwrap().push(Waiter { id, matcher, tx });
        if let Err(e) = self.send(req) {
            self.remove_waiter(id);
            return Err(e);
        }
        match rx.recv_timeout(timeout) {
            Ok(r) => Ok(r),
            Err(_) => {
                self.remove_waiter(id);
                Err(if self.is_closed() { Error::Closed } else { Error::Timeout })
            }
        }
    }

    fn remove_waiter(&self, id: u64) {
        self.inner.waiters.lock().unwrap().retain(|w| w.id != id);
    }

    /// Generic HID++ request: a response echoes bytes 1..=3, an error echoes
    /// bytes 2..=3. Error reports are converted to `Err`.
    pub fn request(&self, req: &Report, timeout: Duration) -> Result<Report> {
        let (idx, b2, b3) = (req.device_index(), req.sub_id(), req.address());
        let matcher: Matcher = Box::new(move |r: &Report| {
            if r.device_index() != idx {
                return false;
            }
            if r.is_error() {
                let (e2, e3, _) = r.error_info().unwrap();
                return e2 == b2 && e3 == b3;
            }
            r.sub_id() == b2 && r.address() == b3
        });
        let resp = self.request_with(req, matcher, timeout)?;
        if let Some((_, _, code)) = resp.error_info() {
            return Err(if resp.sub_id() == sub_id::ERROR_MSG {
                Error::Hidpp10(Hidpp10Error::from(code))
            } else {
                Error::Hidpp20(Hidpp20Error::from(code))
            });
        }
        Ok(resp)
    }

    pub fn close(&self) {
        self.inner.closed.store(true, Ordering::Relaxed);
    }
}

fn reader_loop(weak: std::sync::Weak<Inner>, dev: Arc<HidDevice>) {
    let mut buf = [0u8; 128];
    loop {
        let Some(inner) = weak.upgrade() else { return };
        if inner.closed.load(Ordering::Relaxed) {
            return;
        }
        drop(inner);
        match dev.read(&mut buf, Some(Duration::from_millis(250))) {
            Ok(0) => continue,
            Ok(n) => {
                let Some(inner) = weak.upgrade() else { return };
                if let Some(r) = Report::parse(&buf[..n]) {
                    inner.dispatch(r);
                }
            }
            Err(e) => {
                log::debug!("HID++ reader stopped: {e}");
                if let Some(inner) = weak.upgrade() {
                    inner.closed.store(true, Ordering::Relaxed);
                    // Wake pending requests so they fail fast.
                    inner.waiters.lock().unwrap().clear();
                    inner.subscribers.lock().unwrap().clear();
                }
                return;
            }
        }
    }
}
