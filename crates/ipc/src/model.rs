//! Inputsmith's settings model: profiles, button assignments, actions and device settings.
//!
//! Settings are stored as JSON in the same layout Logi Options+ uses for its profiles, so existing
//! Options+ settings can be imported and Inputsmith settings stay readable by people who know that format.
//! Only what Inputsmith understands is typed. Every other key is kept in an `extra` map and written back
//! unchanged, and unknown enum values are kept as [`Other`](Attribute::Other) strings, so a round trip
//! never loses data.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// Defines an enum stored as an upper-case string, keeping unknown values.
macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident { $default:ident = $default_s:literal $(, $variant:ident = $s:literal)* $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
        pub enum $name {
            #[default]
            $default,
            $($variant,)*
            /// A value this version does not know; kept as is.
            Other(String),
        }

        impl $name {
            pub fn as_str(&self) -> &str {
                match self {
                    Self::$default => $default_s,
                    $(Self::$variant => $s,)*
                    Self::Other(s) => s,
                }
            }

            pub fn parse(s: &str) -> Self {
                match s {
                    $default_s => Self::$default,
                    $($s => Self::$variant,)*
                    other => Self::Other(other.to_string()),
                }
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                Ok(match Value::deserialize(d)? {
                    Value::String(s) => Self::parse(&s),
                    other => Self::Other(other.to_string()),
                })
            }
        }
    };
}

// ------------------------------------------------------------------ profiles

/// A set of assignments: the global profile, or one bound to an application.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Profile {
    pub id: String,
    #[serde(skip_serializing_if = "is_default")]
    pub name: String,
    #[serde(skip_serializing_if = "is_default")]
    pub application_id: String,
    #[serde(skip_serializing_if = "is_default")]
    pub active_for_application: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub assignments: Vec<Assignment>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// What a slot (a button, or a device's settings) is set to.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Assignment {
    pub slot_id: String,
    #[serde(skip_serializing_if = "is_default")]
    pub card_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card: Option<Card>,
    #[serde(skip_serializing_if = "is_default")]
    pub is_disabled: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// An application a profile can be bound to.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Application {
    pub application_id: String,
    #[serde(skip_serializing_if = "is_default")]
    pub database_id: String,
    #[serde(skip_serializing_if = "is_default")]
    pub name: String,
    #[serde(skip_serializing_if = "is_default")]
    pub application_path: String,
    #[serde(skip_serializing_if = "is_default")]
    pub application_paths_list: Vec<String>,
    #[serde(skip_serializing_if = "is_default")]
    pub is_custom: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub is_installed: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub is_disabled: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

// ---------------------------------------------------------------------- cards

string_enum! {
    /// What a card is.
    Attribute {
        Invalid = "INVALID",
        MacroPlayback = "MACRO_PLAYBACK",
        MouseSettings = "MOUSE_SETTINGS",
        MouseThumbWheelSettings = "MOUSE_THUMB_WHEEL_SETTINGS",
        MouseScrollWheelSettings = "MOUSE_SCROLL_WHEEL_SETTINGS",
        Adapter2Ways = "ADAPTER_2WAYS",
        Adapter4Ways = "ADAPTER_4WAYS",
        KeyboardSettings = "KEYBOARD_SETTINGS",
        Gesture = "GESTURE",
        BacklightingSettings = "BACKLIGHTING_SETTINGS",
        OneOf = "ONE_OF",
        Folder = "FOLDER",
    }
}

/// An action or a group of settings assigned to a slot.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Card {
    pub id: String,
    #[serde(skip_serializing_if = "is_default")]
    pub name: String,
    #[serde(skip_serializing_if = "is_default")]
    pub attribute: Attribute,
    #[serde(skip_serializing_if = "is_default")]
    pub read_only: bool,
    /// The action of a `MACRO_PLAYBACK` card.
    #[serde(rename = "macro", skip_serializing_if = "Option::is_none")]
    pub action: Option<Macro>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mouse_settings: Option<MouseSettings>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mouse_scroll_wheel_settings: Option<ScrollWheelSettings>,
    /// For `ONE_OF` cards: the key of the chosen nested card.
    #[serde(skip_serializing_if = "is_default")]
    pub selected_nested_card: String,
    /// Options (`ONE_OF`) or directions (`ADAPTER_4WAYS`: `up`, `down`, `left`, `right`, `click`,
    /// `horizontal`, `vertical`).
    #[serde(skip_serializing_if = "is_default")]
    pub nested_cards: BTreeMap<String, Card>,
    #[serde(skip_serializing_if = "is_default")]
    pub nested_cards_order: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gesture_info: Option<GestureInfo>,
    #[serde(skip_serializing_if = "is_default")]
    pub continuous: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub tags: Vec<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

// -------------------------------------------------------------------- actions

string_enum! {
    MacroType {
        Invalid = "INVALID",
        Keystroke = "KEYSTROKE",
        TextBlock = "TEXT_BLOCK",
        Mouse = "MOUSE",
        System = "SYSTEM",
        App = "APP",
        Media = "MEDIA",
        Sequence = "SEQUENCE",
        QuickLaunch = "QUICK_LAUNCH",
        DoNothing = "DO_NOTHING",
        OpenFileFolder = "OPEN_FILE_FOLDER",
        OpenWebPage = "OPEN_WEB_PAGE",
        AdvancedClick = "ADVANCED_CLICK",
        ScreenCapture = "SCREEN_CAPTURE",
        Delay = "DELAY",
    }
}

/// An action ("macro" in the stored format).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Macro {
    #[serde(rename = "type", skip_serializing_if = "is_default")]
    pub kind: MacroType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keystroke: Option<Keystroke>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mouse: Option<MouseMacro>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemMacro>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<App>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<Media>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<Sequence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quick_launch: Option<QuickLaunch>,
    #[serde(rename = "advancedclick", skip_serializing_if = "Option::is_none")]
    pub advanced_click: Option<AdvancedClick>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_file_folder: Option<OpenFileFolder>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_web_page: Option<OpenWebPage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_block: Option<TextBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay: Option<Delay>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A key chord: `code` is a HID keyboard usage (0 for modifiers only), `modifiers` are HID usages
/// 0xE0..=0xE7.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Keystroke {
    #[serde(skip_serializing_if = "is_default")]
    pub code: u32,
    #[serde(skip_serializing_if = "is_default")]
    pub modifiers: Vec<u32>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

string_enum! {
    MouseAction {
        Invalid = "INVALID",
        DpiUp = "DPI_UP",
        DpiDown = "DPI_DOWN",
        DpiShift = "DPI_SHIFT",
        DpiCycle = "DPI_CYCLE",
        DpiDefault = "DPI_DEFAULT",
        Button = "BUTTON",
        ButtonDoubleClick = "BUTTON_DOUBLE_CLICK",
        WheelUp = "WHEEL_UP",
        WheelDown = "WHEEL_DOWN",
        WheelLeft = "WHEEL_LEFT",
        WheelRight = "WHEEL_RIGHT",
        OsxGestureBack = "OSX_GESTURE_BACK",
        OsxGestureForward = "OSX_GESTURE_FORWARD",
        WinBack = "WIN_BACK",
        WinForward = "WIN_FORWARD",
    }
}

/// A mouse action; `hid_usage` is the HID button number for `BUTTON` (1 left, 2 right, 3 middle, 4 back,
/// 5 forward).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MouseMacro {
    #[serde(skip_serializing_if = "is_default")]
    pub action: MouseAction,
    #[serde(skip_serializing_if = "is_default")]
    pub hid_usage: u32,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

string_enum! {
    SystemAction {
        Invalid = "INVALID",
        Close = "CLOSE",
        Minimize = "MINIMIZE",
        Maximize = "MAXIMIZE",
        Fullscreen = "FULLSCREEN",
        Copy = "COPY",
        Paste = "PASTE",
        Undo = "UNDO",
        Redo = "REDO",
        Desktop = "DESKTOP",
        ShowDesktop = "SHOW_DESKTOP",
        FileClient = "FILE_CLIENT",
        MailClient = "MAIL_CLIENT",
        ZoomIn = "ZOOM_IN",
        ZoomOut = "ZOOM_OUT",
        SnapLeft = "SNAP_LEFT",
        SnapRight = "SNAP_RIGHT",
        AppExpose = "APP_EXPOSE",
        TaskView = "TASK_VIEW",
        SwitchBetweenDesktopsLeft = "SWITCH_BETWEEN_DESKTOPS_LEFT",
        SwitchBetweenDesktopsRight = "SWITCH_BETWEEN_DESKTOPS_RIGHT",
        SwitchApps = "SWITCH_APPS",
        ShiftSwitchApps = "SHIFT_SWITCH_APPS",
        Lock = "LOCK",
        WindowsScreenCapture = "WINDOWS_SCREEN_CAPTURE",
        WindowsPrintScreen = "WINDOWS_PRINT_SCREEN",
        AppMenu = "APP_MENU",
        SwitchLanguage = "SWITCH_LANGUAGE",
        LanguageSwitch = "LANGUAGE_SWITCH",
        EnableDictation = "ENABLE_DICTATION",
        WindowsHighContrast = "WINDOWS_HIGH_CONTRAST",
        Home = "HOME",
        ScrollLeft = "SCROLL_LEFT",
        ScrollRight = "SCROLL_RIGHT",
        None = "NONE",
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SystemMacro {
    #[serde(skip_serializing_if = "is_default")]
    pub action: SystemAction,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Launches a program.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct App {
    #[serde(skip_serializing_if = "is_default")]
    pub executable_path: String,
    #[serde(skip_serializing_if = "is_default")]
    pub working_directory_path: String,
    #[serde(skip_serializing_if = "is_default")]
    pub argument_list: Vec<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

string_enum! {
    MediaUsage {
        Unknown = "UNKNOWN",
        PlayPause = "PLAY_PAUSE",
        Stop = "STOP",
        VolumeUp = "VOLUME_UP",
        VolumeDown = "VOLUME_DOWN",
        NextTrack = "NEXT_TRACK",
        PreviousTrack = "PREVIOUS_TRACK",
        Mute = "MUTE",
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Media {
    #[serde(skip_serializing_if = "is_default")]
    pub usage: MediaUsage,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

string_enum! {
    QuickLaunchAction {
        Calculator = "CALCULATOR",
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct QuickLaunch {
    #[serde(skip_serializing_if = "is_default")]
    pub action: QuickLaunchAction,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

string_enum! {
    AdvancedClickType {
        SingleClick = "SINGLE_CLICK",
        DoubleClick = "DOUBLE_CLICK",
        TripleClick = "TRIPLE_CLICK",
        RightClick = "RIGHT_CLICK",
        MiddleClick = "MIDDLE_CLICK",
    }
}

/// A click with modifiers held.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AdvancedClick {
    #[serde(skip_serializing_if = "is_default")]
    pub click_type: AdvancedClickType,
    #[serde(skip_serializing_if = "is_default")]
    pub is_shift_enabled: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub is_ctrl_enabled: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub is_alt_enabled: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub is_win_enabled: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OpenFileFolder {
    #[serde(skip_serializing_if = "is_default")]
    pub path: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OpenWebPage {
    #[serde(skip_serializing_if = "is_default")]
    pub url: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TextBlock {
    #[serde(skip_serializing_if = "is_default")]
    pub text: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Delay {
    #[serde(skip_serializing_if = "is_default")]
    pub duration_ms: u32,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A recorded sequence of input events and actions.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Sequence {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub simple_sequence: Option<StateSequence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub press_sequence: Option<StateSequence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_sequence: Option<StateSequence>,
    #[serde(skip_serializing_if = "is_default")]
    pub use_default_delay: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub default_delay: u32,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StateSequence {
    #[serde(skip_serializing_if = "is_default")]
    pub components: Vec<SequenceStep>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One step of a sequence; exactly one field is set.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SequenceStep {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyboard: Option<KeyEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mouse: Option<MouseEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay: Option<Delay>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_block: Option<TextBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keystroke: Option<Keystroke>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mouse_macro: Option<MouseMacro>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemMacro>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<Media>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<App>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A key going down or up (HID keyboard usage).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct KeyEvent {
    #[serde(skip_serializing_if = "is_default")]
    pub hid_usage: u32,
    #[serde(skip_serializing_if = "is_default")]
    pub is_down: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MouseEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub button: Option<ButtonEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wheel: Option<WheelEvent>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A mouse button going down or up (HID button number).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ButtonEvent {
    #[serde(skip_serializing_if = "is_default")]
    pub hid_usage: u32,
    #[serde(skip_serializing_if = "is_default")]
    pub is_down: bool,
}

/// One wheel notch; `hid_usage` 0x238 (AC Pan) is the horizontal wheel.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WheelEvent {
    #[serde(skip_serializing_if = "is_default")]
    pub hid_usage: u32,
    #[serde(skip_serializing_if = "is_default")]
    pub is_positive: bool,
}

// ------------------------------------------------------------------ gestures

string_enum! {
    /// Built-in gesture behaviours; `GENERIC` runs the direction cards.
    GestureImplementation {
        Generic = "GENERIC",
        CustomApplicationNavigation = "CUSTOM_APPLICATION_NAVIGATION",
        CustomZoomRotate = "CUSTOM_ZOOM_ROTATE",
        CustomPan = "CUSTOM_PAN",
        CustomSwitchDesktops = "CUSTOM_SWITCH_DESKTOPS",
        CustomHorizontalScroll = "CUSTOM_HORIZONTAL_SCROLL",
        CustomVolumeControl = "CUSTOM_VOLUME_CONTROL",
        CustomZoomInOut = "CUSTOM_ZOOM_IN_OUT",
        CustomDoNothing = "CUSTOM_DO_NOTHING",
        CustomVerticalScroll = "CUSTOM_VERTICAL_SCROLL",
    }
}

/// How a gesture card turns movement into actions.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GestureInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<AxisInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<AxisInfo>,
    /// Once a direction is chosen, the other axis is ignored for the rest of the gesture.
    #[serde(skip_serializing_if = "is_default")]
    pub direction_locked: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub implementation: GestureImplementation,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AxisInfo {
    #[serde(skip_serializing_if = "is_default")]
    pub invertable: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub speed_control: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub auto_repeat: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub pause_between_actions_ms: u32,
    #[serde(skip_serializing_if = "is_default")]
    pub action_duration_ms: u32,
    #[serde(skip_serializing_if = "is_default")]
    pub speed_factor: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_duration: Option<ActionDuration>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ActionDuration {
    #[serde(skip_serializing_if = "is_default")]
    pub minimum_sensitivity_action_duration_ms: u32,
    #[serde(skip_serializing_if = "is_default")]
    pub maximum_sensitivity_action_duration_ms: u32,
}

// ------------------------------------------------------------ device settings

/// Pointer settings (`MOUSE_SETTINGS` cards).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MouseSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pointer_speed: Option<PointerSpeed>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PointerSpeed {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<PointerSpeedValue>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Pointer speed from −1 (slowest) to 1 (fastest); 0 is the device default.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PointerSpeedValue {
    #[serde(skip_serializing_if = "is_default")]
    pub value: f32,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

string_enum! {
    /// Scroll direction; `NATURAL` is shown as "Inverted".
    WheelDirection {
        Undefined = "UNDEFINED",
        Natural = "NATURAL",
        Standard = "STANDARD",
    }
}

string_enum! {
    ScrollMode {
        Unchanged = "UNCHANGED",
        Freespin = "FREESPIN",
        Ratchet = "RATCHET",
    }
}

/// Scroll wheel settings (`MOUSE_SCROLL_WHEEL_SETTINGS` cards). Some sources (the per-model defaults in
/// the Options+ device database) use snake_case keys, hence the aliases.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScrollWheelSettings {
    #[serde(skip_serializing_if = "is_default")]
    pub speed: f64,
    #[serde(skip_serializing_if = "is_default")]
    pub dir: WheelDirection,
    #[serde(alias = "is_smooth", skip_serializing_if = "is_default")]
    pub is_smooth: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smartshift: Option<SmartShiftSettings>,
    #[serde(alias = "virtual_thumbwheel", skip_serializing_if = "Option::is_none")]
    pub virtual_thumbwheel: Option<VirtualThumbwheel>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SmartShiftSettings {
    #[serde(alias = "is_enabled", skip_serializing_if = "is_default")]
    pub is_enabled: bool,
    /// 0..=100
    #[serde(skip_serializing_if = "is_default")]
    pub sensitivity: u32,
    #[serde(skip_serializing_if = "is_default")]
    pub mode: ScrollMode,
    #[serde(alias = "is_scroll_force_enabled", skip_serializing_if = "is_default")]
    pub is_scroll_force_enabled: bool,
    /// 1..=100
    #[serde(alias = "scroll_force", skip_serializing_if = "is_default")]
    pub scroll_force: u32,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// "Hold a button and turn the wheel" horizontal scrolling, per button.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VirtualThumbwheel {
    #[serde(skip_serializing_if = "is_default")]
    pub keys: Vec<VirtualThumbwheelKey>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

string_enum! {
    StateChange {
        Default = "DEFAULT",
        ByUser = "BY_USER",
        DueToConflictResolve = "DUE_TO_CONFLICT_RESOLVE",
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VirtualThumbwheelKey {
    /// The button's control id.
    #[serde(alias = "control_id", skip_serializing_if = "is_default")]
    pub control_id: u32,
    #[serde(skip_serializing_if = "is_default")]
    pub enabled: bool,
    #[serde(alias = "state_change", skip_serializing_if = "is_default")]
    pub state_change: StateChange,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_fields_and_values_survive_a_round_trip() {
        let src = serde_json::json!({
            "id": "c", "attribute": "SOMETHING_NEW", "taskId": 7,
            "macro": {"type": "KEYSTROKE", "keystroke": {"code": 6, "modifiers": [224], "displayCharacter": "C"}},
            "nestedCards": {"a": {"id": "a", "attribute": "MACRO_PLAYBACK"}}
        });
        let card: Card = serde_json::from_value(src.clone()).unwrap();
        assert_eq!(card.attribute, Attribute::Other("SOMETHING_NEW".into()));
        assert_eq!(card.action.as_ref().unwrap().kind, MacroType::Keystroke);
        assert_eq!(card.nested_cards["a"].attribute, Attribute::MacroPlayback);
        assert_eq!(serde_json::to_value(&card).unwrap(), src);
    }

    #[test]
    fn reads_snake_case_scroll_defaults() {
        let s: ScrollWheelSettings = serde_json::from_value(serde_json::json!({
            "dir": "STANDARD", "isSmooth": false,
            "smartshift": {"is_enabled": true, "scroll_force": 50, "sensitivity": 83, "mode": "RATCHET"},
            "virtualThumbwheel": {"keys": [{"controlId": 83, "enabled": true, "stateChange": "DEFAULT"}]}
        }))
        .unwrap();
        let ss = s.smartshift.unwrap();
        assert!(ss.is_enabled);
        assert_eq!((ss.scroll_force, ss.mode), (50, ScrollMode::Ratchet));
        assert_eq!(s.virtual_thumbwheel.unwrap().keys[0].control_id, 83);
    }
}
