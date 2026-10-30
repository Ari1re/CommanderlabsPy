use anyhow::{Context, Result};
use ratatui::{style::{Color, Modifier, Style}, widgets::BorderType};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::Path,
    sync::{OnceLock, RwLock},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RgbValue {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RgbValue {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn color(self) -> Color {
        Color::Rgb(self.r, self.g, self.b)
    }

    pub fn hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemePalette {
    pub bg: RgbValue,
    pub panel: RgbValue,
    pub panel_alt: RgbValue,
    pub text: RgbValue,
    pub muted: RgbValue,
    pub blue: RgbValue,
    pub cyan: RgbValue,
    pub green: RgbValue,
    pub orange: RgbValue,
    pub yellow: RgbValue,
    pub magenta: RgbValue,
    pub red: RgbValue,
    pub purple: RgbValue,
    pub white_mana: RgbValue,
    pub blue_mana: RgbValue,
    pub black_mana: RgbValue,
    pub red_mana: RgbValue,
    pub green_mana: RgbValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedTheme {
    pub name: String,
    pub builtin: bool,
    pub palette: ThemePalette,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeStyle {
    Square,
    Rounded,
}

impl Default for EdgeStyle {
    fn default() -> Self {
        Self::Rounded
    }
}

impl EdgeStyle {
    pub fn label(self) -> &'static str {
        match self {
            Self::Square => "SQUARE",
            Self::Rounded => "ROUNDED",
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Self::Square => Self::Rounded,
            Self::Rounded => Self::Square,
        }
    }

    pub fn border_type(self) -> BorderType {
        match self {
            Self::Square => BorderType::Plain,
            Self::Rounded => BorderType::Rounded,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PriceCurrency {
    Usd,
    Eur,
    Tix,
}

impl Default for PriceCurrency {
    fn default() -> Self {
        Self::Usd
    }
}

impl PriceCurrency {
    pub fn label(self) -> &'static str {
        match self {
            Self::Usd => "USD",
            Self::Eur => "EUR",
            Self::Tix => "MTGO TIX",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Usd => Self::Eur,
            Self::Eur => Self::Tix,
            Self::Tix => Self::Usd,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct UiStyleSettings {
    #[serde(default)]
    edge_style: EdgeStyle,
    #[serde(default)]
    price_currency: PriceCurrency,
    #[serde(default)]
    transparent_background: bool,
}

impl Default for UiStyleSettings {
    fn default() -> Self {
        Self {
            edge_style: EdgeStyle::Rounded,
            price_currency: PriceCurrency::Usd,
            transparent_background: false,
        }
    }
}

impl Default for ThemePalette {
    fn default() -> Self {
        // Neon Slate: dark blue-violet terminal base with cyan focus,
        // magenta secondary accents, and yellow/orange status emphasis.
        // This is the new Commander Lab default.
        Self {
            bg: RgbValue::new(37, 35, 55),
            panel: RgbValue::new(42, 40, 62),
            panel_alt: RgbValue::new(50, 47, 72),
            text: RgbValue::new(224, 222, 232),
            muted: RgbValue::new(151, 150, 174),
            blue: RgbValue::new(73, 208, 246),
            cyan: RgbValue::new(20, 224, 235),
            green: RgbValue::new(76, 220, 137),
            orange: RgbValue::new(255, 166, 59),
            yellow: RgbValue::new(252, 204, 63),
            magenta: RgbValue::new(229, 65, 139),
            red: RgbValue::new(255, 75, 113),
            purple: RgbValue::new(181, 87, 238),
            white_mana: RgbValue::new(238, 236, 220),
            blue_mana: RgbValue::new(73, 208, 246),
            black_mana: RgbValue::new(109, 103, 135),
            red_mana: RgbValue::new(255, 94, 94),
            green_mana: RgbValue::new(76, 220, 137),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeSlot {
    Background,
    Panel,
    PanelAlt,
    Text,
    Muted,
    Blue,
    Cyan,
    Green,
    Orange,
    Yellow,
    Magenta,
    Red,
    Purple,
    WhiteMana,
    BlueMana,
    BlackMana,
    RedMana,
    GreenMana,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeChannel {
    Red,
    Green,
    Blue,
}

impl ThemeChannel {
    pub const ALL: [ThemeChannel; 3] = [
        ThemeChannel::Red,
        ThemeChannel::Green,
        ThemeChannel::Blue,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ThemeChannel::Red => "RED",
            ThemeChannel::Green => "GREEN",
            ThemeChannel::Blue => "BLUE",
        }
    }

    pub fn value(self, rgb: RgbValue) -> u8 {
        match self {
            ThemeChannel::Red => rgb.r,
            ThemeChannel::Green => rgb.g,
            ThemeChannel::Blue => rgb.b,
        }
    }

    pub fn set(self, rgb: &mut RgbValue, value: u8) {
        match self {
            ThemeChannel::Red => rgb.r = value,
            ThemeChannel::Green => rgb.g = value,
            ThemeChannel::Blue => rgb.b = value,
        }
    }

    pub fn preview(self, value: u8) -> Color {
        match self {
            ThemeChannel::Red => RgbValue::new(value, 0, 0).color(),
            ThemeChannel::Green => RgbValue::new(0, value, 0).color(),
            ThemeChannel::Blue => RgbValue::new(0, 0, value).color(),
        }
    }
}

impl ThemeSlot {
    pub const ALL: [ThemeSlot; 18] = [
        ThemeSlot::Background,
        ThemeSlot::Panel,
        ThemeSlot::PanelAlt,
        ThemeSlot::Text,
        ThemeSlot::Muted,
        ThemeSlot::Blue,
        ThemeSlot::Cyan,
        ThemeSlot::Green,
        ThemeSlot::Orange,
        ThemeSlot::Yellow,
        ThemeSlot::Magenta,
        ThemeSlot::Red,
        ThemeSlot::Purple,
        ThemeSlot::WhiteMana,
        ThemeSlot::BlueMana,
        ThemeSlot::BlackMana,
        ThemeSlot::RedMana,
        ThemeSlot::GreenMana,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ThemeSlot::Background => "APP BACKGROUND",
            ThemeSlot::Panel => "PANEL SURFACE",
            ThemeSlot::PanelAlt => "SELECTED SURFACE",
            ThemeSlot::Text => "PRIMARY TEXT",
            ThemeSlot::Muted => "MUTED TEXT",
            ThemeSlot::Blue => "PRIMARY ACCENT",
            ThemeSlot::Cyan => "ACTIVE / FOCUS",
            ThemeSlot::Green => "SUCCESS / POSITIVE",
            ThemeSlot::Orange => "ACTION / ORANGE",
            ThemeSlot::Yellow => "WARNING / EMPHASIS",
            ThemeSlot::Magenta => "SECONDARY ACCENT",
            ThemeSlot::Red => "DANGER / ERROR",
            ThemeSlot::Purple => "AI / DEEP LEARN",
            ThemeSlot::WhiteMana => "WHITE MANA",
            ThemeSlot::BlueMana => "BLUE MANA",
            ThemeSlot::BlackMana => "BLACK MANA",
            ThemeSlot::RedMana => "RED MANA",
            ThemeSlot::GreenMana => "GREEN MANA",
        }
    }
}

fn legacy_lambda_palette() -> ThemePalette {
    ThemePalette {
        bg: RgbValue::new(28, 20, 19),
        panel: RgbValue::new(38, 27, 25),
        panel_alt: RgbValue::new(48, 34, 31),
        text: RgbValue::new(222, 207, 184),
        muted: RgbValue::new(151, 128, 108),
        blue: RgbValue::new(184, 96, 50),
        cyan: RgbValue::new(198, 119, 68),
        green: RgbValue::new(137, 181, 82),
        orange: RgbValue::new(229, 116, 61),
        yellow: RgbValue::new(221, 170, 70),
        magenta: RgbValue::new(177, 102, 82),
        red: RgbValue::new(207, 78, 68),
        purple: RgbValue::new(159, 102, 83),
        white_mana: RgbValue::new(226, 210, 173),
        blue_mana: RgbValue::new(91, 135, 155),
        black_mana: RgbValue::new(111, 96, 88),
        red_mana: RgbValue::new(186, 77, 61),
        green_mana: RgbValue::new(108, 151, 83),
    }
}

pub fn built_in_themes() -> Vec<NamedTheme> {
    vec![
        NamedTheme {
            name: "Neon Slate".to_string(),
            builtin: true,
            palette: ThemePalette::default(),
        },
        NamedTheme {
            name: "Lambda".to_string(),
            builtin: true,
            palette: legacy_lambda_palette(),
        },
        NamedTheme {
            name: "Dracula".to_string(),
            builtin: true,
            palette: ThemePalette {
                bg: RgbValue::new(40, 42, 54),
                panel: RgbValue::new(48, 50, 66),
                panel_alt: RgbValue::new(68, 71, 90),
                text: RgbValue::new(248, 248, 242),
                muted: RgbValue::new(98, 114, 164),
                blue: RgbValue::new(139, 233, 253),
                cyan: RgbValue::new(139, 233, 253),
                green: RgbValue::new(80, 250, 123),
                orange: RgbValue::new(255, 184, 108),
                yellow: RgbValue::new(241, 250, 140),
                magenta: RgbValue::new(255, 121, 198),
                red: RgbValue::new(255, 85, 85),
                purple: RgbValue::new(189, 147, 249),
                white_mana: RgbValue::new(248, 248, 242),
                blue_mana: RgbValue::new(139, 233, 253),
                black_mana: RgbValue::new(98, 114, 164),
                red_mana: RgbValue::new(255, 85, 85),
                green_mana: RgbValue::new(80, 250, 123),
            },
        },
        NamedTheme {
            name: "Gruvbox Dark".to_string(),
            builtin: true,
            palette: ThemePalette {
                bg: RgbValue::new(40, 40, 40),
                panel: RgbValue::new(50, 48, 47),
                panel_alt: RgbValue::new(60, 56, 54),
                text: RgbValue::new(235, 219, 178),
                muted: RgbValue::new(146, 131, 116),
                blue: RgbValue::new(131, 165, 152),
                cyan: RgbValue::new(142, 192, 124),
                green: RgbValue::new(184, 187, 38),
                orange: RgbValue::new(254, 128, 25),
                yellow: RgbValue::new(250, 189, 47),
                magenta: RgbValue::new(211, 134, 155),
                red: RgbValue::new(251, 73, 52),
                purple: RgbValue::new(177, 98, 134),
                white_mana: RgbValue::new(235, 219, 178),
                blue_mana: RgbValue::new(131, 165, 152),
                black_mana: RgbValue::new(102, 92, 84),
                red_mana: RgbValue::new(251, 73, 52),
                green_mana: RgbValue::new(184, 187, 38),
            },
        },
        NamedTheme {
            name: "Nord".to_string(),
            builtin: true,
            palette: ThemePalette {
                bg: RgbValue::new(46, 52, 64),
                panel: RgbValue::new(59, 66, 82),
                panel_alt: RgbValue::new(67, 76, 94),
                text: RgbValue::new(236, 239, 244),
                muted: RgbValue::new(129, 161, 193),
                blue: RgbValue::new(94, 129, 172),
                cyan: RgbValue::new(136, 192, 208),
                green: RgbValue::new(163, 190, 140),
                orange: RgbValue::new(208, 135, 112),
                yellow: RgbValue::new(235, 203, 139),
                magenta: RgbValue::new(180, 142, 173),
                red: RgbValue::new(191, 97, 106),
                purple: RgbValue::new(180, 142, 173),
                white_mana: RgbValue::new(236, 239, 244),
                blue_mana: RgbValue::new(94, 129, 172),
                black_mana: RgbValue::new(76, 86, 106),
                red_mana: RgbValue::new(191, 97, 106),
                green_mana: RgbValue::new(163, 190, 140),
            },
        },
        NamedTheme {
            name: "Tokyo Night".to_string(),
            builtin: true,
            palette: ThemePalette {
                bg: RgbValue::new(26, 27, 38),
                panel: RgbValue::new(36, 40, 59),
                panel_alt: RgbValue::new(41, 46, 66),
                text: RgbValue::new(192, 202, 245),
                muted: RgbValue::new(86, 95, 137),
                blue: RgbValue::new(122, 162, 247),
                cyan: RgbValue::new(125, 207, 255),
                green: RgbValue::new(158, 206, 106),
                orange: RgbValue::new(255, 158, 100),
                yellow: RgbValue::new(224, 175, 104),
                magenta: RgbValue::new(187, 154, 247),
                red: RgbValue::new(247, 118, 142),
                purple: RgbValue::new(157, 124, 216),
                white_mana: RgbValue::new(192, 202, 245),
                blue_mana: RgbValue::new(122, 162, 247),
                black_mana: RgbValue::new(86, 95, 137),
                red_mana: RgbValue::new(247, 118, 142),
                green_mana: RgbValue::new(158, 206, 106),
            },
        },
        NamedTheme {
            name: "Solarized Dark".to_string(),
            builtin: true,
            palette: ThemePalette {
                bg: RgbValue::new(0, 43, 54),
                panel: RgbValue::new(7, 54, 66),
                panel_alt: RgbValue::new(88, 110, 117),
                text: RgbValue::new(238, 232, 213),
                muted: RgbValue::new(131, 148, 150),
                blue: RgbValue::new(38, 139, 210),
                cyan: RgbValue::new(42, 161, 152),
                green: RgbValue::new(133, 153, 0),
                orange: RgbValue::new(203, 75, 22),
                yellow: RgbValue::new(181, 137, 0),
                magenta: RgbValue::new(211, 54, 130),
                red: RgbValue::new(220, 50, 47),
                purple: RgbValue::new(108, 113, 196),
                white_mana: RgbValue::new(238, 232, 213),
                blue_mana: RgbValue::new(38, 139, 210),
                black_mana: RgbValue::new(88, 110, 117),
                red_mana: RgbValue::new(220, 50, 47),
                green_mana: RgbValue::new(133, 153, 0),
            },
        },
    ]
}

pub fn load_named_themes(path: impl AsRef<Path>) -> Vec<NamedTheme> {
    let mut themes = built_in_themes();
    let path = path.as_ref();

    if let Ok(text) = fs::read_to_string(path) {
        if let Ok(mut user) = serde_json::from_str::<Vec<NamedTheme>>(&text) {
            user.retain(|theme| !theme.name.trim().is_empty());

            for mut item in user {
                item.builtin = false;

                if !themes.iter().any(|existing| {
                    existing.name.eq_ignore_ascii_case(&item.name)
                }) {
                    themes.push(item);
                }
            }
        }
    }

    themes
}

pub fn save_named_theme(
    path: impl AsRef<Path>,
    name: &str,
) -> Result<()> {
    let name = name.trim();

    if name.is_empty() {
        anyhow::bail!("Theme name cannot be empty.");
    }

    let path = path.as_ref();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut user = if path.exists() {
        let text = fs::read_to_string(path)?;
        serde_json::from_str::<Vec<NamedTheme>>(&text)
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let entry = NamedTheme {
        name: name.to_string(),
        builtin: false,
        palette: current(),
    };

    if let Some(existing) = user
        .iter_mut()
        .find(|theme| theme.name.eq_ignore_ascii_case(name))
    {
        *existing = entry;
    } else {
        user.push(entry);
    }

    fs::write(path, serde_json::to_string_pretty(&user)?)?;
    Ok(())
}

pub fn apply_named_theme(theme: &NamedTheme) {
    replace(theme.palette.clone());
}

pub fn current_theme_name(themes: &[NamedTheme]) -> String {
    let palette = current();

    themes
        .iter()
        .find(|theme| theme.palette == palette)
        .map(|theme| theme.name.clone())
        .unwrap_or_else(|| "Custom".to_string())
}

fn legacy_metro_palette() -> ThemePalette {
    ThemePalette {
        bg: RgbValue::new(15, 23, 32),
        panel: RgbValue::new(27, 39, 51),
        panel_alt: RgbValue::new(34, 49, 63),
        text: RgbValue::new(238, 244, 248),
        muted: RgbValue::new(135, 154, 168),
        blue: RgbValue::new(0, 164, 239),
        cyan: RgbValue::new(0, 188, 242),
        green: RgbValue::new(127, 186, 0),
        orange: RgbValue::new(242, 80, 34),
        yellow: RgbValue::new(255, 185, 0),
        magenta: RgbValue::new(227, 0, 140),
        red: RgbValue::new(232, 17, 35),
        purple: RgbValue::new(104, 33, 122),
        white_mana: RgbValue::new(248, 231, 185),
        blue_mana: RgbValue::new(14, 104, 171),
        black_mana: RgbValue::new(125, 115, 110),
        red_mana: RgbValue::new(211, 55, 48),
        green_mana: RgbValue::new(0, 115, 62),
    }
}


static TRANSPARENT_BACKGROUND: OnceLock<RwLock<bool>> = OnceLock::new();

fn transparent_background_store() -> &'static RwLock<bool> {
    TRANSPARENT_BACKGROUND.get_or_init(|| RwLock::new(false))
}

pub fn transparent_background() -> bool {
    transparent_background_store()
        .read()
        .map(|value| *value)
        .unwrap_or(false)
}

pub fn toggle_transparent_background() -> bool {
    let next = !transparent_background();

    if let Ok(mut guard) = transparent_background_store().write() {
        *guard = next;
    }

    next
}

static PRICE_CURRENCY: OnceLock<RwLock<PriceCurrency>> = OnceLock::new();

fn price_currency_store() -> &'static RwLock<PriceCurrency> {
    PRICE_CURRENCY.get_or_init(|| RwLock::new(PriceCurrency::Usd))
}

pub fn price_currency() -> PriceCurrency {
    price_currency_store()
        .read()
        .map(|currency| *currency)
        .unwrap_or_default()
}

pub fn set_price_currency(currency: PriceCurrency) {
    if let Ok(mut guard) = price_currency_store().write() {
        *guard = currency;
    }
}

pub fn cycle_price_currency() -> PriceCurrency {
    let next = price_currency().next();
    set_price_currency(next);
    next
}

static EDGE_STYLE: OnceLock<RwLock<EdgeStyle>> = OnceLock::new();

fn edge_store() -> &'static RwLock<EdgeStyle> {
    EDGE_STYLE.get_or_init(|| RwLock::new(EdgeStyle::Rounded))
}

pub fn edge_style() -> EdgeStyle {
    edge_store()
        .read()
        .map(|style| *style)
        .unwrap_or_default()
}

pub fn set_edge_style(style: EdgeStyle) {
    if let Ok(mut guard) = edge_store().write() {
        *guard = style;
    }
}

pub fn toggle_edge_style() -> EdgeStyle {
    let next = edge_style().toggle();
    set_edge_style(next);
    next
}

pub fn border_type() -> BorderType {
    edge_style().border_type()
}

pub fn load_ui_style(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();

    if !path.exists() {
        return Ok(());
    }

    let text = fs::read_to_string(path)
        .with_context(|| format!("Could not read {}", path.display()))?;
    let settings = serde_json::from_str::<UiStyleSettings>(&text)
        .with_context(|| format!("Could not parse {}", path.display()))?;

    set_edge_style(settings.edge_style);
    set_price_currency(settings.price_currency);

    if let Ok(mut guard) = transparent_background_store().write() {
        *guard = settings.transparent_background;
    }

    Ok(())
}

pub fn save_ui_style(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let settings = UiStyleSettings {
        edge_style: edge_style(),
        price_currency: price_currency(),
        transparent_background: transparent_background(),
    };

    fs::write(path, serde_json::to_string_pretty(&settings)?)?;
    Ok(())
}

static THEME: OnceLock<RwLock<ThemePalette>> = OnceLock::new();

fn store() -> &'static RwLock<ThemePalette> {
    THEME.get_or_init(|| RwLock::new(ThemePalette::default()))
}

pub fn current() -> ThemePalette {
    store()
        .read()
        .map(|palette| palette.clone())
        .unwrap_or_else(|_| ThemePalette::default())
}

pub fn replace(palette: ThemePalette) {
    if let Ok(mut guard) = store().write() {
        *guard = palette;
    }
}

pub fn reset() {
    replace(ThemePalette::default());
}

pub fn load_saved_theme(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(());
    }

    let text = fs::read_to_string(path)
        .with_context(|| format!("Could not read {}", path.display()))?;
    let palette = serde_json::from_str::<ThemePalette>(&text)
        .with_context(|| format!("Could not parse {}", path.display()))?;

    if palette == legacy_metro_palette()
        || palette == legacy_lambda_palette()
    {
        replace(ThemePalette::default());

        // Only auto-migrate exact former built-in defaults. User-customized
        // palettes remain untouched.
        let _ = fs::write(
            path,
            serde_json::to_string_pretty(&current())?,
        );
    } else {
        replace(palette);
    }

    Ok(())
}

pub fn save_theme(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(path, serde_json::to_string_pretty(&current())?)?;
    Ok(())
}

pub fn slot_value(slot: ThemeSlot) -> RgbValue {
    let p = current();
    match slot {
        ThemeSlot::Background => p.bg,
        ThemeSlot::Panel => p.panel,
        ThemeSlot::PanelAlt => p.panel_alt,
        ThemeSlot::Text => p.text,
        ThemeSlot::Muted => p.muted,
        ThemeSlot::Blue => p.blue,
        ThemeSlot::Cyan => p.cyan,
        ThemeSlot::Green => p.green,
        ThemeSlot::Orange => p.orange,
        ThemeSlot::Yellow => p.yellow,
        ThemeSlot::Magenta => p.magenta,
        ThemeSlot::Red => p.red,
        ThemeSlot::Purple => p.purple,
        ThemeSlot::WhiteMana => p.white_mana,
        ThemeSlot::BlueMana => p.blue_mana,
        ThemeSlot::BlackMana => p.black_mana,
        ThemeSlot::RedMana => p.red_mana,
        ThemeSlot::GreenMana => p.green_mana,
    }
}

pub fn set_slot(slot: ThemeSlot, value: RgbValue) {
    let mut p = current();
    match slot {
        ThemeSlot::Background => p.bg = value,
        ThemeSlot::Panel => p.panel = value,
        ThemeSlot::PanelAlt => p.panel_alt = value,
        ThemeSlot::Text => p.text = value,
        ThemeSlot::Muted => p.muted = value,
        ThemeSlot::Blue => p.blue = value,
        ThemeSlot::Cyan => p.cyan = value,
        ThemeSlot::Green => p.green = value,
        ThemeSlot::Orange => p.orange = value,
        ThemeSlot::Yellow => p.yellow = value,
        ThemeSlot::Magenta => p.magenta = value,
        ThemeSlot::Red => p.red = value,
        ThemeSlot::Purple => p.purple = value,
        ThemeSlot::WhiteMana => p.white_mana = value,
        ThemeSlot::BlueMana => p.blue_mana = value,
        ThemeSlot::BlackMana => p.black_mana = value,
        ThemeSlot::RedMana => p.red_mana = value,
        ThemeSlot::GreenMana => p.green_mana = value,
    }
    replace(p);
}

pub fn bg() -> Color {
    if transparent_background() {
        Color::Reset
    } else {
        current().bg.color()
    }
}

pub fn panel_color() -> Color {
    if transparent_background() {
        Color::Reset
    } else {
        current().panel.color()
    }
}

pub fn panel_alt_color() -> Color {
    if transparent_background() {
        Color::Reset
    } else {
        current().panel_alt.color()
    }
}
pub fn text() -> Color { current().text.color() }
pub fn muted_color() -> Color { current().muted.color() }
pub fn blue() -> Color { current().blue.color() }
pub fn cyan() -> Color { current().cyan.color() }
pub fn green() -> Color { current().green.color() }
pub fn orange() -> Color { current().orange.color() }
pub fn yellow() -> Color { current().yellow.color() }
pub fn magenta() -> Color { current().magenta.color() }
pub fn red() -> Color { current().red.color() }
pub fn purple() -> Color { current().purple.color() }
pub fn white_mana() -> Color { current().white_mana.color() }
pub fn blue_mana() -> Color { current().blue_mana.color() }
pub fn black_mana() -> Color { current().black_mana.color() }
pub fn red_mana() -> Color { current().red_mana.color() }
pub fn green_mana() -> Color { current().green_mana.color() }

pub fn base() -> Style {
    Style::default().fg(text()).bg(bg())
}
pub fn panel() -> Style {
    Style::default().fg(text()).bg(panel_color())
}
pub fn title() -> Style {
    Style::default()
        .fg(orange())
        .bg(panel_color())
        .add_modifier(Modifier::BOLD)
}
pub fn selected() -> Style {
    Style::default()
        .fg(yellow())
        .bg(panel_alt_color())
        .add_modifier(Modifier::BOLD)
}
