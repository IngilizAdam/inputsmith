//! Profile and card helpers shared by the views.

use inputsmith_ipc::model::{
    Application, Assignment, Attribute, Card, MouseSettings, PointerSpeed, Profile, ScrollMode, ScrollWheelSettings,
    SmartShiftSettings, StateChange, VirtualThumbwheelKey, WheelDirection,
};

use crate::vendor::Vendor;

pub const DEFAULT_PROFILE_NAME: &str = "PROFILE_NAME_DEFAULT";

pub fn is_default(p: &Profile) -> bool {
    p.application_id == p.id || p.name == DEFAULT_PROFILE_NAME
}

pub fn default_profile(profiles: &[Profile]) -> Option<&Profile> {
    profiles.iter().find(|p| is_default(p))
}

/// Effective assignment: the selected application profile's own, falling
/// back to the global profile (mirrors the agent's resolution).
pub fn resolve<'a>(profiles: &'a [Profile], app_profile: Option<&str>, slot: &str) -> Option<&'a Assignment> {
    let find = |p: &'a Profile| p.assignments.iter().find(|a| a.slot_id == slot && !a.is_disabled);
    app_profile
        .and_then(|id| profiles.iter().find(|p| p.id == id))
        .and_then(find)
        .or_else(|| default_profile(profiles).and_then(find))
}

/// Whether the selected application profile overrides `slot` itself.
pub fn overrides(profiles: &[Profile], app_profile: Option<&str>, slot: &str) -> bool {
    app_profile
        .and_then(|id| profiles.iter().find(|p| p.id == id))
        .is_some_and(|p| p.assignments.iter().any(|a| a.slot_id == slot))
}

/// Human-readable name of a card, following `ONE_OF` selections.
pub fn card_title(card: &Card) -> String {
    let v = Vendor::get();
    if card.attribute == Attribute::OneOf {
        if let Some(n) = card.nested_cards.get(&card.selected_nested_card) {
            return format!("{}: {}", v.text(&card.name), v.text(&n.name));
        }
    }
    if card.name.is_empty() {
        return card.id.clone();
    }
    v.text(&card.name)
}

pub fn application_name(apps: &[Application], profile: &Profile) -> String {
    apps.iter()
        .find(|a| a.application_id == profile.application_id)
        .map(|a| a.name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| profile.name.clone())
}

/// Pointer speed in percent from a `MOUSE_SETTINGS` card (value −1..1).
pub fn pointer_speed_percent(card: Option<&Card>) -> f32 {
    let v = card
        .and_then(|c| c.mouse_settings.as_ref())
        .and_then(|m| m.pointer_speed.as_ref())
        .and_then(|p| p.active.as_ref())
        .map_or(0.0, |a| a.value);
    ((v + 1.0) * 50.0).clamp(0.0, 100.0)
}

pub fn with_pointer_speed(card: Option<&Card>, slot: &str, percent: f32) -> Card {
    let mut c = card.cloned().unwrap_or_else(|| Card {
        id: format!("inputsmith_{slot}"),
        name: "Mouse".into(),
        attribute: Attribute::MouseSettings,
        ..Default::default()
    });
    let ms = c.mouse_settings.get_or_insert_with(MouseSettings::default);
    let ps = ms.pointer_speed.get_or_insert_with(PointerSpeed::default);
    let active = ps.active.get_or_insert_with(Default::default);
    active.value = (percent / 50.0 - 1.0).clamp(-1.0, 1.0);
    c
}

/// Scroll wheel settings in effect: the card's, else the model's defaults (as the agent applies them).
pub fn scroll_settings(card: Option<&Card>, defaults: Option<&ScrollWheelSettings>) -> ScrollWheelSettings {
    card.and_then(|c| c.mouse_scroll_wheel_settings.clone()).or_else(|| defaults.cloned()).unwrap_or_else(|| {
        ScrollWheelSettings {
            dir: WheelDirection::Standard,
            smartshift: Some(SmartShiftSettings {
                is_enabled: true,
                sensitivity: 50,
                mode: ScrollMode::Ratchet,
                is_scroll_force_enabled: true,
                scroll_force: 60,
                ..Default::default()
            }),
            ..Default::default()
        }
    })
}

/// Whether "hold + wheel" horizontal scrolling is on for control `cid`.
pub fn hscroll_enabled(s: &ScrollWheelSettings, cid: u16) -> bool {
    s.virtual_thumbwheel.as_ref().is_some_and(|v| v.keys.iter().any(|k| k.control_id == cid as u32 && k.enabled))
}

/// Turns horizontal scrolling on or off for `cid`, keeping the other controls' entries.
pub fn set_hscroll(s: &mut ScrollWheelSettings, cid: u16, enabled: bool) {
    let keys = &mut s.virtual_thumbwheel.get_or_insert_with(Default::default).keys;
    let state_change = StateChange::ByUser;
    match keys.iter_mut().find(|k| k.control_id == cid as u32) {
        Some(k) => {
            k.enabled = enabled;
            k.state_change = state_change;
        }
        None => keys.push(VirtualThumbwheelKey { control_id: cid as u32, enabled, state_change }),
    }
}

/// Control behind a Point & Scroll "horizontal scrolling" hotspot, from its slot name.
pub fn hscroll_control(name_key: &str) -> Option<u16> {
    if !name_key.contains("HORIZONTAL_SCROLL") {
        None
    } else if name_key.contains("FORWARD") {
        Some(0x56)
    } else if name_key.contains("BACK") {
        Some(0x53)
    } else {
        None
    }
}

pub fn with_scroll_settings(card: Option<&Card>, slot: &str, s: ScrollWheelSettings) -> Card {
    let mut c = card.cloned().unwrap_or_else(|| Card {
        id: format!("inputsmith_{slot}"),
        name: "Mouse".into(),
        attribute: Attribute::MouseScrollWheelSettings,
        ..Default::default()
    });
    c.mouse_scroll_wheel_settings = Some(s);
    c
}

pub fn assignment(slot: &str, card: Card) -> Assignment {
    Assignment { slot_id: slot.to_string(), card_id: card.id.clone(), card: Some(card), ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_speed_roundtrip() {
        let c = with_pointer_speed(None, "x_mouse_settings", 25.0);
        assert!((pointer_speed_percent(Some(&c)) - 25.0).abs() < 0.01);
        assert_eq!(c.attribute, Attribute::MouseSettings);
    }

    #[test]
    fn horizontal_scroll_toggle() {
        let mut s = scroll_settings(None, None);
        assert!(!hscroll_enabled(&s, 0x56));
        set_hscroll(&mut s, 0x56, true);
        set_hscroll(&mut s, 0x53, false);
        assert!(hscroll_enabled(&s, 0x56) && !hscroll_enabled(&s, 0x53));
        set_hscroll(&mut s, 0x56, false);
        assert!(!hscroll_enabled(&s, 0x56));
        assert_eq!(s.virtual_thumbwheel.unwrap().keys.len(), 2);
        assert_eq!(hscroll_control("SLOT_NAME_HORIZONTAL_SCROLL_BACK_BUTTON"), Some(0x53));
        assert_eq!(hscroll_control("SLOT_NAME_SCROLL_WHEEL_SETTINGS"), None);
    }

    #[test]
    fn resolve_prefers_app_profile() {
        let g = Profile {
            id: "g".into(),
            application_id: "g".into(),
            assignments: vec![assignment("s", Card { id: "global".into(), ..Default::default() })],
            ..Default::default()
        };
        let a = Profile {
            id: "a".into(),
            application_id: "app".into(),
            assignments: vec![assignment("s", Card { id: "app".into(), ..Default::default() })],
            ..Default::default()
        };
        let ps = vec![g, a];
        assert_eq!(resolve(&ps, Some("a"), "s").unwrap().card_id, "app");
        assert_eq!(resolve(&ps, None, "s").unwrap().card_id, "global");
        assert!(overrides(&ps, Some("a"), "s"));
    }
}
