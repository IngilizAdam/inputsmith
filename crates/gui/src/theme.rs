//! Visual style modelled on the Options+ dark theme.

use iced::border::Radius;
use iced::widget::{button, container, slider, text_input, toggler};
use iced::{Background, Border, Color, Font, Shadow, Theme, Vector, font, theme};

pub const BG: Color = rgb(0x1A, 0x1A, 0x1A);
pub const PANEL: Color = rgb(0x0B, 0x0B, 0x0B);
pub const SURFACE: Color = rgb(0x26, 0x26, 0x26);
pub const SURFACE_HOVER: Color = rgb(0x33, 0x33, 0x33);
pub const ACCENT: Color = rgb(0x00, 0xE0, 0xC4);
pub const TEXT: Color = rgb(0xFF, 0xFF, 0xFF);
pub const MUTED: Color = rgb(0x9C, 0x9C, 0x9C);
pub const DANGER: Color = rgb(0xFF, 0x5A, 0x5A);

pub const BOLD: Font = Font { weight: font::Weight::Bold, ..Font::DEFAULT };
pub const SEMIBOLD: Font = Font { weight: font::Weight::Semibold, ..Font::DEFAULT };

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: 1.0 }
}

pub fn theme() -> Theme {
    Theme::custom(
        "inputsmith",
        theme::Palette {
            background: BG,
            text: TEXT,
            primary: ACCENT,
            success: ACCENT,
            warning: rgb(0xFF, 0xC1, 0x07),
            danger: DANGER,
        },
    )
}

pub fn page(_: &Theme) -> container::Style {
    container::Style { background: Some(BG.into()), text_color: Some(TEXT), ..Default::default() }
}

pub fn side_panel(_: &Theme) -> container::Style {
    container::Style { background: Some(PANEL.into()), text_color: Some(TEXT), ..Default::default() }
}

pub fn pill(_: &Theme) -> container::Style {
    container::Style {
        background: Some(rgb(0x10, 0x10, 0x10).into()),
        border: Border { radius: 6.0.into(), width: 1.0, color: rgb(0x30, 0x30, 0x30) },
        text_color: Some(ACCENT),
        ..Default::default()
    }
}

/// Callout box next to a device hotspot.
pub fn callout(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let bg = if selected {
            ACCENT
        } else if status == button::Status::Hovered {
            SURFACE_HOVER
        } else {
            rgb(0x22, 0x22, 0x22)
        };
        button::Style {
            background: Some(bg.into()),
            text_color: if selected { Color::BLACK } else { TEXT },
            border: Border { radius: 3.0.into(), ..Default::default() },
            shadow: Shadow { color: Color { a: 0.5, ..Color::BLACK }, offset: Vector::new(0.0, 2.0), blur_radius: 8.0 },
            snap: true,
        }
    }
}

/// Hotspot dot on the device image.
pub fn marker(selected: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(if selected { ACCENT } else { rgb(0x50, 0x50, 0x50) }.into()),
        border: Border { radius: Radius::from(7.0), width: 2.0, color: TEXT },
        ..Default::default()
    }
}

/// Left navigation entries (BUTTONS / POINT & SCROLL / ...).
pub fn nav(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: if selected {
            Some(ACCENT.into())
        } else if status == button::Status::Hovered {
            Some(SURFACE.into())
        } else {
            None
        },
        text_color: if selected { Color::BLACK } else { TEXT },
        border: Border { radius: 4.0.into(), ..Default::default() },
        ..Default::default()
    }
}

/// Borderless text/icon button.
pub fn ghost(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: (status == button::Status::Hovered).then_some(Background::Color(SURFACE)),
        text_color: TEXT,
        border: Border { radius: 6.0.into(), ..Default::default() },
        ..Default::default()
    }
}

/// Filled call-to-action button.
pub fn primary(_: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => rgb(0x33, 0xF0, 0xD6),
        button::Status::Disabled => SURFACE,
        _ => ACCENT,
    };
    button::Style {
        background: Some(bg.into()),
        text_color: Color::BLACK,
        border: Border { radius: 4.0.into(), ..Default::default() },
        ..Default::default()
    }
}

/// Row in the action list.
pub fn action_row(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: if selected {
            Some(ACCENT.into())
        } else if status == button::Status::Hovered {
            Some(SURFACE.into())
        } else {
            None
        },
        text_color: if selected { Color::BLACK } else { TEXT },
        border: Border { radius: 4.0.into(), ..Default::default() },
        ..Default::default()
    }
}

/// Device tile on the home screen.
pub fn device_tile(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(if status == button::Status::Hovered { rgb(0x22, 0x22, 0x22) } else { BG }.into()),
        text_color: TEXT,
        border: Border { radius: 12.0.into(), ..Default::default() },
        ..Default::default()
    }
}

pub fn slider_style(_: &Theme, status: slider::Status) -> slider::Style {
    let handle = if status == slider::Status::Dragged { TEXT } else { ACCENT };
    slider::Style {
        rail: slider::Rail {
            backgrounds: (ACCENT.into(), rgb(0x40, 0x40, 0x40).into()),
            width: 4.0,
            border: Border { radius: 2.0.into(), ..Default::default() },
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 7.0 },
            background: handle.into(),
            border_width: 2.0,
            border_color: PANEL,
        },
    }
}

pub fn toggler_style(_: &Theme, status: toggler::Status) -> toggler::Style {
    let on =
        matches!(status, toggler::Status::Active { is_toggled: true } | toggler::Status::Hovered { is_toggled: true });
    toggler::Style {
        background: if on { ACCENT } else { rgb(0x40, 0x40, 0x40) }.into(),
        background_border_width: 0.0,
        background_border_color: Color::TRANSPARENT,
        foreground: if on { Color::BLACK } else { rgb(0xBB, 0xBB, 0xBB) }.into(),
        foreground_border_width: 0.0,
        foreground_border_color: Color::TRANSPARENT,
        text_color: None,
        border_radius: None,
        padding_ratio: 0.1,
    }
}

pub fn search(_: &Theme, _status: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: rgb(0x1E, 0x1E, 0x1E).into(),
        border: Border { radius: 4.0.into(), width: 1.0, color: rgb(0x33, 0x33, 0x33) },
        icon: MUTED,
        placeholder: MUTED,
        value: TEXT,
        selection: Color { a: 0.4, ..ACCENT },
    }
}
