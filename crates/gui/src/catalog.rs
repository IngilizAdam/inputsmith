//! What the settings window shows about devices and actions, from Inputsmith's
//! own data (`inputsmith-data`): device art and hotspots, the action catalog,
//! per-control recommendations and UI text. Nothing is read from Options+.

use std::collections::HashSet;

use iced::widget::svg::Handle;
use inputsmith_ipc::model::Card;

#[derive(Debug, Clone, Default)]
pub struct Hotspot {
    pub slot_id: String,
    pub name_key: String,
    /// Marker position in percent of the art's size.
    pub x: f32,
    pub y: f32,
    /// Label direction: -1 left, 1 right, 0 centred; `dy` likewise vertically.
    pub dx: f32,
    pub dy: f32,
}

/// Device art (an SVG) with its intrinsic size.
#[derive(Debug, Clone)]
pub struct Art {
    pub handle: Handle,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Default)]
pub struct DeviceArt {
    pub image: Option<Art>,
    pub button_hotspots: Vec<Hotspot>,
    pub point_scroll_hotspots: Vec<Hotspot>,
}

/// Text for a UI key, or the key made readable when there is none.
pub fn text(key: &str) -> String {
    if let Some(s) = inputsmith_data::text(key) {
        return s.to_string();
    }
    let base = key
        .trim_start_matches("ASSIGNMENT_NAME_")
        .trim_start_matches("SLOT_NAME_")
        .trim_end_matches("_BUTTON")
        .replace('_', " ")
        .to_lowercase();
    let mut c = base.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

pub fn card(id: &str) -> Option<&'static Card> {
    inputsmith_data::cards().iter().find(|c| c.id == id)
}

/// The card a control of this model uses until the user assigns one, when that differs from the
/// control's own function (the gesture button).
pub fn default_card(model_id: &str, cid: u16) -> Option<&'static Card> {
    inputsmith_data::default_card(model_id, cid)
}

/// The card describing what an unassigned control does by itself, for showing it by name (Options+
/// lists "Middle button", "Back", … rather than "Default").
pub fn native_card(cid: u16) -> Option<&'static Card> {
    let id = match cid {
        0x52 => "card_global_presets_middle_button",
        0x53 => "card_global_presets_win_back",
        0x56 => "card_global_presets_win_forward",
        0xC4 => "card_global_presets_mode_shift",
        _ => return None,
    };
    card(id)
}

/// Cards shown first for a control, in order.
pub fn recommended(cid: u16) -> Vec<&'static Card> {
    inputsmith_data::control(cid).into_iter().flat_map(|c| &c.recommended).filter_map(|id| card(id)).collect()
}

/// Cards allowed on a control, grouped into the picker's categories.
pub fn categories(cid: u16) -> Vec<(&'static str, Vec<&'static Card>)> {
    let allowed: Vec<&str> =
        inputsmith_data::control(cid).map(|c| c.allowed_tags.iter().map(String::as_str).collect()).unwrap_or_default();
    let ok = |c: &&Card| {
        !c.tags.iter().any(|t| t == "PRESET_TAG_MACROS_UNSUPPORTED")
            && (allowed.is_empty() || c.tags.iter().any(|t| allowed.contains(&t.as_str())))
    };
    let groups: [(&str, &[&str]); 5] = [
        ("Mouse", &["PRESET_TAG_MOUSE", "PRESET_MOUSE_FUNCTIONS", "PRESET_TAG_MOUSE_SMARTSHIFT"]),
        ("Computer", &["PRESET_COMPUTER_FUNCTIONS"]),
        ("Media", &["PRESET_TAG_MEDIA"]),
        ("Keyboard", &["PRESET_KEYBOARD_FUNCTIONS", "PRESET_TAG_KEY_OR_BUTTON"]),
        ("Gestures", &["PRESET_TAG_GESTURE"]),
    ];
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for (name, tags) in groups {
        let cards: Vec<&Card> = inputsmith_data::cards()
            .iter()
            .filter(ok)
            .filter(|c| c.tags.iter().any(|t| tags.contains(&t.as_str())))
            .filter(|c| seen.insert(c.id.clone()))
            .collect();
        if !cards.is_empty() {
            out.push((name, cards));
        }
    }
    out
}

/// Art and hotspots for a model; empty for devices without a description.
pub fn art(model_id: &str, slot_prefix: &str) -> DeviceArt {
    let Some(d) = inputsmith_data::device(model_id) else { return DeviceArt::default() };
    let spots = |list: &[inputsmith_data::Hotspot]| {
        list.iter()
            .map(|h| Hotspot {
                slot_id: format!("{slot_prefix}_{}", h.slot),
                name_key: h.name.clone(),
                x: h.x,
                y: h.y,
                dx: h.dx,
                dy: h.dy,
            })
            .collect()
    };
    DeviceArt {
        image: d.art.as_deref().and_then(inputsmith_data::art).and_then(load_svg),
        button_hotspots: spots(&d.buttons),
        point_scroll_hotspots: spots(&d.point_scroll),
    }
}

fn load_svg(bytes: &'static [u8]) -> Option<Art> {
    let (width, height) = view_box_size(std::str::from_utf8(bytes).ok()?)?;
    Some(Art { handle: Handle::from_memory(bytes), width, height })
}

/// Width and height from the root element's `viewBox`.
fn view_box_size(svg: &str) -> Option<(f32, f32)> {
    let rest = &svg[svg.find("viewBox=\"")? + 9..];
    let nums: Vec<f32> = rest[..rest.find('"')?].split_whitespace().filter_map(|n| n.parse().ok()).collect();
    match nums[..] {
        [_, _, w, h] if w > 0.0 && h > 0.0 => Some((w, h)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn mx_anywhere_3_art() {
        let art = super::art("6b025", "mx-anywhere-3-6b025");
        let img = art.image.unwrap();
        assert_eq!((img.width, img.height), (266.0, 422.0));
        assert!(art.button_hotspots.iter().any(|h| h.slot_id == "mx-anywhere-3-6b025_c82"));
        assert!(super::art("4082", "x").image.is_none());
    }

    #[test]
    fn picker_matches_options_plus() {
        let rec: Vec<&str> = super::recommended(83).iter().map(|c| c.id.as_str()).collect();
        assert_eq!(
            rec,
            [
                "card_global_presets_win_back",
                "card_global_presets_win_copy",
                "card_global_presets_volume_down",
                "card_global_presets_win_undo"
            ]
        );
        // Shift wheel mode is only offered on the mode shift button.
        let has_shift =
            |cid| super::categories(cid).iter().flat_map(|(_, c)| c).any(|c| c.id == "card_global_presets_mode_shift");
        assert!(has_shift(196) && !has_shift(83));
    }

    #[test]
    fn unassigned_buttons_are_named_after_their_function() {
        assert_eq!(super::native_card(0x53).map(|c| c.id.as_str()), Some("card_global_presets_win_back"));
        assert!(super::native_card(0xC3).is_none());
        assert_eq!(
            super::default_card("6b023", 0xC3).map(|c| c.id.as_str()),
            Some("card_global_presets_one_of_gesture_button")
        );
    }

    #[test]
    fn text_falls_back_to_the_key() {
        assert_eq!(super::text("SLOT_NAME_BACK_BUTTON"), "Back button");
        assert_eq!(super::text("ASSIGNMENT_NAME_SOMETHING_ELSE"), "Something else");
    }
}
