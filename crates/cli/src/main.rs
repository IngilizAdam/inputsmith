//! `inputsmith` — diagnostic CLI for the HID++ stack.

mod sniff;

use std::process::ExitCode;
use std::time::Duration;

use inputsmith_hidpp::channel::{self, Channel, InterfaceInfo};
use inputsmith_hidpp::device::{self, Device};
use inputsmith_hidpp::features::battery::Battery;
use inputsmith_hidpp::features::change_host::ChangeHost;
use inputsmith_hidpp::features::device_info::{DeviceInformation, DeviceName, FriendlyName};
use inputsmith_hidpp::features::dpi::{AdjustableDpi, ExtendedDpi, PointerSpeed};
use inputsmith_hidpp::features::haptics::Haptics;
use inputsmith_hidpp::features::fn_inversion::FnInversion;
use inputsmith_hidpp::features::reprog_controls::{ReprogControls, control_name};
use inputsmith_hidpp::features::wheel::{HiResWheel, SmartShift, Thumbwheel};
use inputsmith_hidpp::receiver::{Receiver, receiver_kind};
use inputsmith_hidpp::{feature_id, report};

fn usage() -> ExitCode {
    eprintln!(
        "usage: inputsmith <command>\n\n\
         commands:\n  \
         list              list HID++ interfaces, receivers and devices\n  \
         info              show detailed state of every reachable device\n  \
         hid               list raw Logitech HID collections
  sniff             watch HID++ events and injected keys (to observe other software)"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match args.first().map(String::as_str) {
        Some("list") => list(false),
        Some("info") => list(true),
        Some("hid") => hid(),
        Some("sniff") => sniff::run(),
        Some("set") => set(&args[1..]),
        Some("ipc") => return ipc(&args[1..]),
        _ => return usage(),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// `ipc <VERB> <path> [json]`: one request to the inputsmith agent, or
/// `ipc subscribe <path>` to print events until interrupted.
fn ipc(args: &[String]) -> ExitCode {
    use inputsmith_ipc::{Message, Verb, transport::Conn};
    let (Some(verb), Some(path)) = (args.first(), args.get(1)) else {
        eprintln!("usage: inputsmith ipc <get|set|remove|subscribe> <path> [json payload]");
        return ExitCode::from(2);
    };
    let Some(verb) = Verb::parse(verb) else {
        eprintln!("bad verb: {verb}");
        return ExitCode::from(2);
    };
    let conn = match Conn::connect(&inputsmith_ipc::transport::agent_path()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("cannot reach agent: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut m = Message { id: 1, ..Message::new(verb, path.as_str()) };
    if let Some(p) = args.get(2) {
        match serde_json::from_str(p) {
            Ok(v) => m.payload = Some(v),
            Err(e) => {
                eprintln!("bad JSON payload: {e}");
                return ExitCode::from(2);
            }
        }
    }
    if conn.send(&m).is_err() {
        return ExitCode::FAILURE;
    }
    let subscribe = verb == Verb::Subscribe;
    for r in conn.messages() {
        match r {
            Ok(r) => {
                println!("{}", serde_json::to_string_pretty(&r).unwrap_or_default());
                if !subscribe {
                    return ExitCode::SUCCESS;
                }
            }
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

fn hid() -> inputsmith_hidpp::Result<()> {
    for d in inputsmith_hid::enumerate(Some(channel::LOGITECH_VID))? {
        println!(
            "{:04x}:{:04x} mi={:?} page={:04x} usage={:04x} in={} out={} {:?} {:?}",
            d.vendor_id,
            d.product_id,
            d.interface_number,
            d.usage_page,
            d.usage,
            d.input_report_len,
            d.output_report_len,
            d.product.unwrap_or_default(),
            d.bus
        );
    }
    Ok(())
}

/// `set smartshift <threshold> <torque>` / `set dpi <value>` on the first
/// HID++ 2.0 device found (testing aid).
fn set(args: &[String]) -> inputsmith_hidpp::Result<()> {
    for iface in channel::enumerate()? {
        let ch = Channel::open(&iface)?;
        let indexes: Vec<u8> = if receiver_kind(iface.product_id).is_some() { (1..=6).collect() } else { vec![0xFF] };
        for i in indexes {
            if !matches!(device::ping(&ch, i, Duration::from_millis(500)), Ok(Some(v)) if v.major >= 2) {
                continue;
            }
            let dev = Device::open(ch.clone(), i)?;
            let num = |k: usize| args.get(k).and_then(|s| s.parse::<u16>().ok());
            match (args.first().map(String::as_str), num(1), num(2)) {
                (Some("smartshift"), Some(t), Some(f)) => SmartShift(&dev).set(None, Some(t as u8), Some(f as u8))?,
                (Some("dpi"), Some(d), _) => AdjustableDpi(&dev).set_dpi(0, d)?,
                _ => {
                    eprintln!("usage: inputsmith set smartshift <threshold> <torque> | set dpi <value>");
                    return Ok(());
                }
            }
            println!("{}: done", DeviceName(&dev).name().unwrap_or_default());
            return Ok(());
        }
    }
    Err(inputsmith_hidpp::Error::NoInterface)
}

fn list(detail: bool) -> inputsmith_hidpp::Result<()> {
    for iface in channel::enumerate()? {
        let ch = match Channel::open(&iface) {
            Ok(c) => c,
            Err(e) => {
                println!("{:04x}:{:04x} {:?}: cannot open: {e}", iface.vendor_id, iface.product_id, iface.product);
                continue;
            }
        };
        if receiver_kind(iface.product_id).is_some() {
            show_receiver(&ch, &iface, detail);
        } else {
            println!(
                "Direct device {:04x}:{:04x} {:?} ({:?})",
                iface.vendor_id,
                iface.product_id,
                iface.product.as_deref().unwrap_or(""),
                iface.bus
            );
            show_device(&ch, report::DEVICE_INDEX_DIRECT, detail, "  ");
        }
        ch.close();
    }
    Ok(())
}

fn show_receiver(ch: &Channel, iface: &InterfaceInfo, detail: bool) {
    let rx = Receiver::new(ch.clone());
    println!("Receiver {:04x}:{:04x} {:?}", iface.vendor_id, iface.product_id, rx.kind());
    if let Ok(s) = rx.serial() {
        println!("  serial: {s}");
    }
    if let Ok(fw) = rx.firmware() {
        println!("  firmware: {fw}");
    }
    match rx.paired_devices() {
        Ok(devs) => {
            for d in devs {
                println!(
                    "  [{}] wpid={:04x} kind={:?} name={:?} serial={:?}",
                    d.index,
                    d.wpid,
                    d.kind,
                    d.name.as_deref().unwrap_or(""),
                    d.serial.as_deref().unwrap_or("")
                );
                show_device(ch, d.index, detail, "      ");
            }
        }
        Err(e) => println!("  pairing table unavailable: {e}"),
    }
}

fn show_device(ch: &Channel, index: u8, detail: bool, ind: &str) {
    match device::ping(ch, index, Duration::from_secs(2)) {
        Ok(None) => {
            println!("{ind}offline");
            return;
        }
        Ok(Some(v)) if v.major < 2 => {
            println!("{ind}HID++ {v} device");
            return;
        }
        Err(e) => {
            println!("{ind}ping failed: {e}");
            return;
        }
        Ok(Some(_)) => {}
    }
    let dev = match Device::open(ch.clone(), index) {
        Ok(d) => d,
        Err(e) => {
            println!("{ind}open failed: {e}");
            return;
        }
    };
    let name = DeviceName(&dev).name().unwrap_or_default();
    let kind = DeviceName(&dev).device_type().map(|t| format!("{t:?}")).unwrap_or_default();
    println!("{ind}{name} ({kind}), HID++ {}", dev.protocol());
    if let Ok(info) = DeviceInformation(&dev).device_info() {
        println!(
            "{ind}model id: {} (transport {:#06x}, pids {:04x?}, ext {})",
            info.extended_model_id_string(),
            info.transport,
            info.model_ids,
            info.extended_model_id
        );
        if info.has_serial_number()
            && let Ok(s) = DeviceInformation(&dev).serial_number()
        {
            println!("{ind}serial: {s}");
        }
    }
    if let Ok(st) = Battery(&dev).status() {
        println!("{ind}battery: {:?}% {:?} {:?}", st.percent, st.level, st.charging);
    }
    if !detail {
        return;
    }
    if let Ok(fws) = DeviceInformation(&dev).all_firmware() {
        for fw in fws {
            println!("{ind}firmware: {:?} {}", fw.kind, fw.version_string());
        }
    }
    if let Ok((n, max)) = FriendlyName(&dev).get() {
        println!("{ind}friendly name: {n:?} (max {max})");
    }
    println!("{ind}features:");
    for f in dev.features() {
        println!(
            "{ind}  [{:02x}] {:04x} v{} {}{}",
            f.index,
            f.id,
            f.version,
            feature_id::name(f.id).unwrap_or("?"),
            if f.flags.hidden() || f.flags.engineering() { " (hidden)" } else { "" }
        );
    }
    if let Ok(h) = ChangeHost(&dev).host_info() {
        println!("{ind}easy-switch: host {}/{}", h.current + 1, h.host_count);
    }
    if dev.has_feature(feature_id::ADJUSTABLE_DPI) {
        let d = AdjustableDpi(&dev);
        if let (Ok(cur), Ok(list)) = (d.dpi(0), d.dpi_list(0)) {
            println!("{ind}dpi: {} (default {}), supported {:?}", cur.0, cur.1, list);
        }
    }
    if dev.has_feature(feature_id::EXTENDED_ADJUSTABLE_DPI) {
        let d = ExtendedDpi(&dev);
        println!("{ind}extended dpi: {:?}, capabilities (separate y, lod) {:?}, supported {:?}", d.dpi(0), d.capabilities(0), d.dpi_list(0));
    }
    if dev.has_feature(feature_id::HAPTIC_FEEDBACK) {
        println!("{ind}haptics: {:?}", Haptics(&dev).config());
    }
    if let Ok(s) = PointerSpeed(&dev).get() {
        println!("{ind}pointer speed: {:.3}x", s as f32 / 256.0);
    }
    if let Ok(s) = SmartShift(&dev).get() {
        println!("{ind}smartshift: {:?}", s);
    }
    if dev.has_feature(feature_id::HIRES_WHEEL) {
        let w = HiResWheel(&dev);
        println!("{ind}hi-res wheel: {:?} {:?}", w.capabilities(), w.mode());
    }
    if dev.has_feature(feature_id::THUMBWHEEL) {
        let t = Thumbwheel(&dev);
        println!("{ind}thumbwheel: {:?} {:?}", t.info(), t.status());
    }
    if let Ok(f) = FnInversion(&dev).get() {
        println!("{ind}fn inversion: {f}");
    }
    if dev.has_feature(feature_id::SPECIAL_KEYS_1B04) {
        let rc = ReprogControls(&dev);
        if let Ok(ctrls) = rc.controls() {
            println!("{ind}controls:");
            for c in ctrls {
                let rep = rc.reporting(c.cid).ok();
                println!(
                    "{ind}  cid {:#06x} ({}) tid {:#06x} flags {:#06x} pos {} group {} gmask {:#04x} reprog={} divertable={} diverted={:?} analytics={:?}",
                    c.cid,
                    c.cid,
                    c.tid,
                    c.flags.0,
                    c.position,
                    c.group,
                    c.group_mask,
                    c.flags.reprogrammable(),
                    c.flags.divertable(),
                    rep.map(|r| r.diverted),
                    rep.map(|r| r.analytics_key_events)
                );
                if let Some(n) = control_name(c.cid) {
                    println!("{ind}      = {n}");
                }
            }
        }
    }
}
