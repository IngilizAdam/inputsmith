//! Gesture-button recognition for `ADAPTER_4WAYS` cards.
//!
//! While the gesture control is held the device reports raw pointer motion
//! (0x1B04 raw XY diversion) instead of moving the cursor. Nested cards are
//! keyed `up`, `down`, `left`, `right`, `click` and optionally `horizontal` /
//! `vertical` (held for the duration of a gesture on that axis, e.g. Alt for
//! application switching).
//!
//! Model measured against Options+ 2.8 (docs/BEHAVIOR.md) and checked by replaying recorded
//! Options+ traces (`testdata/gesture_traces.json`):
//! - the gesture starts after ~40 counts (the axis card, e.g. Alt, is pressed);
//! - a direction action fires after a speed-dependent distance: 118/√v counts at v counts/ms along
//!   the axis (over the last 64 ms), at most 190 and at least 70 (the maximum is scaled by `speedFactor`);
//! - an axis repeats only if its `AxisInfo` has an action duration or speed factor, otherwise it fires
//!   once per gesture; reversing fires the opposite action after ~150 counts;
//! - after an action, nothing fires for the axis' `actionDuration` (ms) and movement during that time is
//!   discarded; without a duration the remainder carries over.

use std::collections::VecDeque;
use std::sync::OnceLock;
use std::time::Instant;

use inputsmith_input::{MediaKey, usage};
use inputsmith_ipc::model::{AxisInfo, Card, GestureImplementation as Impl};

use crate::actions::{DeviceOps, Executor};

/// Milliseconds on a process-wide monotonic clock, for [`GestureSession::on_move`].
pub fn now_ms() -> u64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Thresholds in raw sensor counts.
#[derive(Debug, Clone, Copy)]
pub struct GestureConfig {
    /// Movement before a gesture (rather than a click) is recognised.
    pub start_threshold: f64,
    /// Step distance at speed v (counts/ms) is `step_coefficient / √v`, clamped to `[step_min, step_max]`.
    pub step_coefficient: f64,
    pub step_min: f64,
    pub step_max: f64,
    /// Time over which the pointer speed is measured.
    pub speed_window_ms: u64,
    /// Movement back past the last action that fires the opposite action.
    pub reverse_threshold: f64,
    /// Step for the built-in (custom) implementations, not measured yet.
    pub custom_step: f64,
    /// Wheel units (1/120 detent) per sensor count for pan/scroll gestures.
    pub pan_factor: f64,
}

impl Default for GestureConfig {
    fn default() -> Self {
        Self {
            start_threshold: 40.0,
            step_coefficient: 118.0,
            step_min: 70.0,
            step_max: 190.0,
            speed_window_ms: 64,
            reverse_threshold: 150.0,
            custom_step: 200.0,
            pan_factor: 1.5,
        }
    }
}

/// Per-axis parameters from the card's `GestureInfo.AxisInfo`.
#[derive(Debug, Clone, Copy, Default)]
struct AxisParams {
    repeats: bool,
    /// Quiet time after an action.
    action_duration_ms: u64,
    /// Scales the maximum step (0.5 for volume: twice as many steps).
    factor: f64,
}

impl AxisParams {
    fn of(a: Option<&AxisInfo>) -> Self {
        let Some(a) = a else { return Self { factor: 1.0, ..Default::default() } };
        let duration =
            a.action_duration.as_ref().map_or(a.action_duration_ms, |d| d.minimum_sensitivity_action_duration_ms);
        Self {
            repeats: a.action_duration.is_some() || a.speed_factor > 0.0 || a.action_duration_ms > 0,
            action_duration_ms: duration as u64,
            factor: if a.speed_factor > 0.0 { a.speed_factor } else { 1.0 },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    fn key(self) -> &'static str {
        match self {
            Dir::Up => "up",
            Dir::Down => "down",
            Dir::Left => "left",
            Dir::Right => "right",
        }
    }

    fn horizontal(self) -> bool {
        matches!(self, Dir::Left | Dir::Right)
    }

    fn of(dx: f64, dy: f64) -> Self {
        // Raw XY follows HID: positive y is downward.
        if dx.abs() >= dy.abs() {
            if dx >= 0.0 { Dir::Right } else { Dir::Left }
        } else if dy >= 0.0 {
            Dir::Down
        } else {
            Dir::Up
        }
    }

    fn on_axis(horizontal: bool, positive: bool) -> Self {
        match (horizontal, positive) {
            (true, true) => Dir::Right,
            (true, false) => Dir::Left,
            (false, true) => Dir::Down,
            (false, false) => Dir::Up,
        }
    }
}

pub struct GestureSession {
    card: Card,
    cfg: GestureConfig,
    implementation: Impl,
    locked: bool,
    /// x / y axis parameters.
    axes: (AxisParams, AxisParams),
    acc: (f64, f64),
    /// Recent (time, x, y) samples for the speed estimate.
    history: VecDeque<(u64, f64, f64)>,
    /// No action before this time (action duration of the last action).
    quiet_until: Option<u64>,
    started: bool,
    axis_horizontal: bool,
    /// Axis position of the last fired action (or of the gesture start).
    anchor: f64,
    /// Sign of the last fired action on the current axis (0 = none yet).
    last_sign: f64,
    held: Option<Card>,
    /// Accumulator for the built-in (custom) implementations.
    step_acc: f64,
    pan_rem: (f64, f64),
}

impl GestureSession {
    pub fn new(card: Card, cfg: GestureConfig) -> Self {
        let info = card.gesture_info.as_ref();
        let implementation = info.map_or(Impl::Generic, |g| g.implementation.clone());
        let locked = info.is_some_and(|g| g.direction_locked);
        let axes = (AxisParams::of(info.and_then(|g| g.x.as_ref())), AxisParams::of(info.and_then(|g| g.y.as_ref())));
        Self {
            card,
            cfg,
            implementation,
            locked,
            axes,
            acc: (0.0, 0.0),
            history: VecDeque::new(),
            quiet_until: None,
            started: false,
            axis_horizontal: false,
            anchor: 0.0,
            last_sign: 0.0,
            held: None,
            step_acc: 0.0,
            pan_rem: (0.0, 0.0),
        }
    }

    fn nested(&self, key: &str) -> Option<&Card> {
        self.card.nested_cards.get(key)
    }

    fn axis_pos(&self) -> f64 {
        if self.axis_horizontal { self.acc.0 } else { self.acc.1 }
    }

    /// Switches the active axis, swapping the held axis card.
    fn set_axis(&mut self, horizontal: bool, ex: &Executor, dev: &dyn DeviceOps) {
        if let Some(h) = self.held.take() {
            ex.release(&h);
        }
        self.axis_horizontal = horizontal;
        self.anchor = self.axis_pos();
        self.last_sign = 0.0;
        let key = if horizontal { "horizontal" } else { "vertical" };
        if let Some(h) = self.nested(key).cloned() {
            ex.press(&h, dev);
            self.held = Some(h);
        }
    }

    /// Speed along the current axis (counts/ms) over the last `speed_window_ms`.
    fn axis_speed(&self, now: u64) -> f64 {
        let Some(&(t0, x0, y0)) =
            self.history.iter().rev().find(|(t, ..)| *t + self.cfg.speed_window_ms <= now).or(self.history.front())
        else {
            return 0.0;
        };
        let d = if self.axis_horizontal { self.acc.0 - x0 } else { self.acc.1 - y0 };
        d.abs() / (now.saturating_sub(t0).max(8)) as f64
    }

    /// Distance to the next action at the current speed.
    fn step(&self, now: u64, axis: AxisParams) -> f64 {
        let max = self.cfg.step_max * axis.factor;
        let v = self.axis_speed(now).max(1e-3);
        (self.cfg.step_coefficient / v.sqrt()).min(max).max(self.cfg.step_min.min(max))
    }

    /// Feeds raw motion received at `now` (milliseconds, see [`now_ms`]).
    pub fn on_move(&mut self, dx: i32, dy: i32, now: u64, ex: &Executor, dev: &dyn DeviceOps) {
        let (dx, dy) = (dx as f64, dy as f64);
        self.acc.0 += dx;
        self.acc.1 += dy;
        self.history.push_back((now, self.acc.0, self.acc.1));
        // Keep one sample older than the window as the speed baseline.
        while self.history.len() > 2 && self.history[1].0 + self.cfg.speed_window_ms <= now {
            self.history.pop_front();
        }
        if !self.started {
            if self.acc.0.hypot(self.acc.1) < self.cfg.start_threshold {
                return;
            }
            self.started = true;
            let d = Dir::of(self.acc.0, self.acc.1);
            log::debug!("gesture start {:?} at ({:.0},{:.0})", d, self.acc.0, self.acc.1);
            if self.is_custom() {
                self.custom_move(self.acc.0, self.acc.1, ex);
            } else {
                self.set_axis(d.horizontal(), ex, dev);
            }
            return;
        }
        if self.is_custom() {
            self.custom_move(dx, dy, ex);
            return;
        }
        if !self.locked && self.last_sign == 0.0 {
            // Until the first action fires, the dominant axis may still change.
            let h = self.acc.0.abs() >= self.acc.1.abs();
            if h != self.axis_horizontal {
                self.set_axis(h, ex, dev);
            }
        }
        let axis = if self.axis_horizontal { self.axes.0 } else { self.axes.1 };
        if let Some(until) = self.quiet_until {
            if now <= until {
                // Movement within the action duration does not count towards the next action.
                self.anchor = self.axis_pos();
                return;
            }
            self.quiet_until = None;
        }
        loop {
            if self.last_sign != 0.0 && !axis.repeats {
                return;
            }
            let diff = self.axis_pos() - self.anchor;
            if diff == 0.0 {
                return;
            }
            let sign = diff.signum();
            let needed = if self.last_sign != 0.0 && sign != self.last_sign {
                self.cfg.reverse_threshold
            } else {
                self.step(now, axis)
            };
            if diff.abs() < needed {
                return;
            }
            let d = Dir::on_axis(self.axis_horizontal, sign > 0.0);
            log::debug!("gesture step {:?} at {:.0} (step {needed:.0})", d, self.axis_pos());
            if let Some(c) = self.nested(d.key()).cloned() {
                ex.trigger(&c, dev);
            }
            self.last_sign = sign;
            if axis.action_duration_ms > 0 {
                self.quiet_until = Some(now + axis.action_duration_ms);
                self.anchor = self.axis_pos();
                return;
            }
            self.anchor += sign * needed;
        }
    }

    pub fn on_release(mut self, ex: &Executor, dev: &dyn DeviceOps) {
        log::debug!("gesture release: started={} total=({:.0},{:.0})", self.started, self.acc.0, self.acc.1);
        if !self.started {
            if let Some(c) = self.nested("click").cloned() {
                if !self.is_custom() || c.action.is_some() {
                    ex.trigger(&c, dev);
                } else if self.implementation == Impl::CustomPan {
                    let _ = ex.sink().click(inputsmith_input::MouseButton::Middle);
                }
            }
        }
        if let Some(h) = self.held.take() {
            ex.release(&h);
        }
    }

    fn is_custom(&self) -> bool {
        matches!(
            self.implementation,
            Impl::CustomPan
                | Impl::CustomZoomRotate
                | Impl::CustomZoomInOut
                | Impl::CustomVolumeControl
                | Impl::CustomHorizontalScroll
                | Impl::CustomVerticalScroll
                | Impl::CustomSwitchDesktops
        )
    }

    /// Built-in behaviours whose nested cards carry no macro.
    fn custom_move(&mut self, dx: f64, dy: f64, ex: &Executor) {
        let s = ex.sink();
        let f = self.cfg.pan_factor;
        match self.implementation {
            Impl::CustomPan | Impl::CustomVerticalScroll | Impl::CustomHorizontalScroll => {
                let (mut h, mut v) = (dx * f, -dy * f);
                if self.implementation == Impl::CustomVerticalScroll {
                    h = 0.0;
                }
                if self.implementation == Impl::CustomHorizontalScroll {
                    v = 0.0;
                }
                self.pan_rem.0 += h;
                self.pan_rem.1 += v;
                let (hi, vi) = (self.pan_rem.0.trunc(), self.pan_rem.1.trunc());
                self.pan_rem.0 -= hi;
                self.pan_rem.1 -= vi;
                let _ = s.wheel(vi as i32, hi as i32);
            }
            Impl::CustomZoomRotate | Impl::CustomZoomInOut => {
                self.step_acc += -dy;
                while self.step_acc.abs() >= self.cfg.custom_step {
                    let up = self.step_acc > 0.0;
                    let _ = s.key(usage::LEFT_CTRL, true);
                    let _ = s.wheel(if up { 120 } else { -120 }, 0);
                    let _ = s.key(usage::LEFT_CTRL, false);
                    self.step_acc -= self.step_acc.signum() * self.cfg.custom_step;
                }
            }
            Impl::CustomVolumeControl => {
                self.step_acc += -dy;
                while self.step_acc.abs() >= self.cfg.custom_step / 2.0 {
                    let up = self.step_acc > 0.0;
                    let _ = s.media(if up { MediaKey::VolumeUp } else { MediaKey::VolumeDown });
                    self.step_acc -= self.step_acc.signum() * self.cfg.custom_step / 2.0;
                }
            }
            Impl::CustomSwitchDesktops => {
                self.step_acc += dx;
                while self.step_acc.abs() >= self.cfg.custom_step * 2.0 {
                    let key = if self.step_acc > 0.0 { usage::RIGHT } else { usage::LEFT };
                    let _ = s.chord(&[usage::LEFT_CTRL, usage::LEFT_GUI], Some(key));
                    self.step_acc -= self.step_acc.signum() * self.cfg.custom_step * 2.0;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::NoDevice;
    use inputsmith_input::{InputSink, MouseButton};
    use std::sync::{Arc, Mutex};

    #[derive(Default, Clone)]
    struct Rec(Arc<Mutex<Vec<String>>>);
    impl InputSink for Rec {
        fn key(&self, u: u16, d: bool) -> inputsmith_input::Result<()> {
            self.0.lock().unwrap().push(format!("{u:02x}{}", if d { "v" } else { "^" }));
            Ok(())
        }
        fn mouse_button(&self, _: MouseButton, _: bool) -> inputsmith_input::Result<()> {
            Ok(())
        }
        fn wheel(&self, v: i32, h: i32) -> inputsmith_input::Result<()> {
            self.0.lock().unwrap().push(format!("w{v},{h}"));
            Ok(())
        }
        fn move_relative(&self, _: i32, _: i32) -> inputsmith_input::Result<()> {
            Ok(())
        }
        fn media(&self, k: MediaKey) -> inputsmith_input::Result<()> {
            self.0.lock().unwrap().push(format!("{k:?}"));
            Ok(())
        }
        fn text(&self, _: &str) -> inputsmith_input::Result<()> {
            Ok(())
        }
    }

    fn ks(code: u32, mods: &[u32]) -> serde_json::Value {
        serde_json::json!({"attribute": "MACRO_PLAYBACK", "continuous": true,
            "macro": {"type": "KEYSTROKE", "keystroke": {"code": code, "modifiers": mods}}})
    }

    /// Options+'s "application navigation" preset (x repeats, y fires once).
    fn app_navigation() -> Card {
        serde_json::from_value(serde_json::json!({
            "attribute": "ADAPTER_4WAYS",
            "gestureInfo": {"directionLocked": true, "implementation": "GENERIC",
                "x": {"invertable": true, "speedControl": true,
                      "actionDuration": {"minimumSensitivityActionDurationMs": 100, "maximumSensitivityActionDurationMs": 100}},
                "y": {"speedControl": true, "pauseBetweenActionsMs": 300}},
            "nestedCards": {
                "horizontal": ks(0, &[226]),
                "right": ks(43, &[]),
                "left": ks(43, &[225]),
                "up": ks(0, &[227]),
                "down": ks(7, &[227]),
                "click": {"attribute": "MACRO_PLAYBACK", "macro": {"type": "MEDIA", "media": {"usage": "MUTE"}}}
            }
        }))
        .unwrap()
    }

    fn joined(rec: &Rec) -> String {
        rec.0.lock().unwrap().join(" ")
    }

    /// Moves by (dx, dy) in 1-count reports every 4 ms (0.25 counts/ms: a slow, deliberate swipe),
    /// starting at `*t`.
    fn slide(g: &mut GestureSession, dx: i32, dy: i32, t: &mut u64, ex: &Executor) {
        let n = dx.abs().max(dy.abs());
        for i in 0..n {
            let sx = if i < dx.abs() { dx.signum() } else { 0 };
            let sy = if i < dy.abs() { dy.signum() } else { 0 };
            *t += 4;
            g.on_move(sx, sy, *t, ex, &NoDevice);
        }
    }

    #[test]
    fn click_without_movement() {
        let rec = Rec::default();
        let ex = Executor::new(Box::new(rec.clone()));
        let mut g = GestureSession::new(app_navigation(), GestureConfig::default());
        g.on_move(5, -3, 0, &ex, &NoDevice);
        g.on_release(&ex, &NoDevice);
        assert_eq!(joined(&rec), "Mute");
    }

    /// Slow swipe: Alt at 40, Tab 190 later and every ~215 after (190 + movement during the 100 ms quiet
    /// time), Shift+Tab after moving back 150.
    #[test]
    fn slow_horizontal_steps() {
        let rec = Rec::default();
        let ex = Executor::new(Box::new(rec.clone()));
        let mut g = GestureSession::new(app_navigation(), GestureConfig::default());
        let mut t = 0;
        slide(&mut g, 45, 0, &mut t, &ex);
        assert_eq!(joined(&rec), "e2v");
        slide(&mut g, 150, 0, &mut t, &ex); // 195: nothing yet
        assert_eq!(joined(&rec), "e2v");
        slide(&mut g, 60, 0, &mut t, &ex); // 255: first Tab
        assert_eq!(joined(&rec), "e2v 2bv 2b^");
        slide(&mut g, 215, 0, &mut t, &ex); // 470: second Tab
        slide(&mut g, -170, 0, &mut t, &ex); // back > 150: Shift+Tab
        g.on_release(&ex, &NoDevice);
        assert_eq!(joined(&rec), "e2v 2bv 2b^ 2bv 2b^ e1v 2bv 2b^ e1^ e2^");
    }

    #[test]
    fn vertical_fires_once() {
        let rec = Rec::default();
        let ex = Executor::new(Box::new(rec.clone()));
        let mut g = GestureSession::new(app_navigation(), GestureConfig::default());
        let mut t = 0;
        slide(&mut g, 0, -80, &mut t, &ex);
        assert_eq!(joined(&rec), "");
        slide(&mut g, 0, -1400, &mut t, &ex);
        g.on_release(&ex, &NoDevice);
        assert_eq!(joined(&rec), "e3v e3^");
    }

    /// Options+'s "media control" preset: x waits 400 ms after an action, y (volume) repeats twice as often.
    fn media_control() -> Card {
        let media = |u: &str| serde_json::json!({"attribute": "MACRO_PLAYBACK", "macro": {"type": "MEDIA", "media": {"usage": u}}});
        serde_json::from_value(serde_json::json!({
            "attribute": "ADAPTER_4WAYS",
            "gestureInfo": {
                "x": {"speedControl": true,
                      "actionDuration": {"minimumSensitivityActionDurationMs": 400, "maximumSensitivityActionDurationMs": 400}},
                "y": {"speedControl": true, "speedFactor": 0.5}},
            "nestedCards": {
                "up": media("VOLUME_UP"), "down": media("VOLUME_DOWN"),
                "left": media("PREVIOUS_TRACK"), "right": media("NEXT_TRACK"), "click": media("PLAY_PAUSE")
            }
        }))
        .unwrap()
    }

    #[derive(serde::Deserialize)]
    struct Trace {
        label: String,
        /// (ms since press, dx, dy)
        moves: Vec<(u64, i32, i32)>,
        /// (ms since press, key, down) as injected by Options+
        keys: Vec<(u64, String, bool)>,
    }

    /// Records each action with the time of the report that caused it.
    #[derive(Default, Clone)]
    struct Timed {
        now: Arc<Mutex<u64>>,
        out: Arc<Mutex<Vec<(u64, String)>>>,
    }
    impl Timed {
        fn push(&self, what: &str) {
            let now = *self.now.lock().unwrap();
            self.out.lock().unwrap().push((now, what.to_string()));
        }
    }
    impl InputSink for Timed {
        fn key(&self, u: u16, d: bool) -> inputsmith_input::Result<()> {
            // Only key-downs of the action keys; modifiers held for the axis are reported as "alt".
            match (u, d) {
                (0x2b, true) => self.push("tab"),
                (0xe2, true) => self.push("alt"),
                (0xe3, true) => self.push("win"),
                _ => {}
            }
            Ok(())
        }
        fn mouse_button(&self, _: MouseButton, _: bool) -> inputsmith_input::Result<()> {
            Ok(())
        }
        fn wheel(&self, _: i32, _: i32) -> inputsmith_input::Result<()> {
            Ok(())
        }
        fn move_relative(&self, _: i32, _: i32) -> inputsmith_input::Result<()> {
            Ok(())
        }
        fn media(&self, k: MediaKey) -> inputsmith_input::Result<()> {
            self.push(match k {
                MediaKey::VolumeUp => "vol_up",
                MediaKey::VolumeDown => "vol_down",
                MediaKey::NextTrack => "next",
                MediaKey::PreviousTrack => "prev",
                _ => "media",
            });
            Ok(())
        }
        fn text(&self, _: &str) -> inputsmith_input::Result<()> {
            Ok(())
        }
    }

    /// Replays gestures recorded with `inputsmith sniff` while Options+ handled them and compares the
    /// actions: same sequence, each within 100 ms of Options+ (slow starts differ the most).
    #[test]
    fn replays_options_plus_traces() {
        let traces: Vec<Trace> = serde_json::from_str(include_str!("testdata/gesture_traces.json")).unwrap();
        let mut report = String::new();
        let mut ok = true;
        for tr in &traces {
            let card = if tr.label.starts_with("media") { media_control() } else { app_navigation() };
            let rec = Timed::default();
            let ex = Executor::new(Box::new(rec.clone()));
            let mut g = GestureSession::new(card, GestureConfig::default());
            for &(t, dx, dy) in &tr.moves {
                *rec.now.lock().unwrap() = t;
                g.on_move(dx, dy, t, &ex, &NoDevice);
            }
            g.on_release(&ex, &NoDevice);
            let ours = rec.out.lock().unwrap().clone();
            let theirs: Vec<(u64, String)> =
                tr.keys.iter().filter(|(_, k, down)| *down && k != "shift").map(|(t, k, _)| (*t, k.clone())).collect();
            let same = ours.len() == theirs.len()
                && ours.iter().zip(&theirs).all(|(a, b)| a.1 == b.1 && a.0.abs_diff(b.0) <= 100);
            ok &= same;
            report += &format!(
                "{} {}\n  options+: {:?}\n  ours:     {:?}\n",
                if same { "ok  " } else { "DIFF" },
                tr.label,
                theirs,
                ours
            );
        }
        assert!(ok, "\n{report}");
        eprintln!("{report}");
    }
}
