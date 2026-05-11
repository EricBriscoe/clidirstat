//! Auto-adapting palette for the treemap.
//!
//! Detects whether the terminal has a dark or light background using
//! `terminal-colorsaurus` (OSC 11 + DA1 fallback). Honors `NO_COLOR`. Each
//! file category exposes RGB triplets tuned for the active mode: brighter,
//! ~70 % L on dark backgrounds; deeper, ~45 % L on light backgrounds so
//! tiles stay legible without searing.

use std::time::Duration;

use crate::fs_categories::Category;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    /// Terminal has a dark background (the common case).
    Dark,
    /// Terminal has a light background.
    Light,
    /// `NO_COLOR=1` was set or the user passed `--theme nocolor`: render in
    /// grayscale.
    NoColor,
}

impl Theme {
    /// Auto-detect from the live terminal. Falls back to `Dark` on error or
    /// timeout. Honors `NO_COLOR`.
    pub fn detect() -> Self {
        if std::env::var_os("NO_COLOR").is_some() {
            return Theme::NoColor;
        }
        let mut opts = terminal_colorsaurus::QueryOptions::default();
        opts.timeout = Duration::from_millis(100);
        match terminal_colorsaurus::theme_mode(opts) {
            Ok(terminal_colorsaurus::ThemeMode::Light) => Theme::Light,
            Ok(terminal_colorsaurus::ThemeMode::Dark) => Theme::Dark,
            Err(_) => Theme::Dark, // default for unsupported terminals
        }
    }

    /// Background fill for canvas pixels not covered by any tile. Matches
    /// the terminal so the treemap "blends" rather than printing a hard
    /// dark patch onto a light terminal.
    pub fn canvas_bg(self) -> (u8, u8, u8) {
        match self {
            Theme::Dark => (20, 20, 24),
            Theme::Light => (240, 240, 244),
            Theme::NoColor => (28, 28, 28),
        }
    }

    /// Selection-outline color, picked to contrast with both dark and light
    /// category fills under the active theme.
    pub fn outline(self) -> (u8, u8, u8) {
        match self {
            Theme::Dark => (255, 255, 255),
            Theme::Light => (16, 16, 20),
            Theme::NoColor => (255, 255, 255),
        }
    }

    /// Per-category RGB triplet for the active theme.
    pub fn rgb(self, cat: Category) -> (u8, u8, u8) {
        match self {
            Theme::Dark => dark_rgb(cat),
            Theme::Light => light_rgb(cat),
            Theme::NoColor => mono_rgb(cat),
        }
    }
}

/// Bright, ~70 % lightness hues for dark terminals.
fn dark_rgb(cat: Category) -> (u8, u8, u8) {
    match cat {
        Category::Code => (88, 166, 255),    // sky blue
        Category::Image => (218, 112, 214),  // orchid
        Category::Video => (255, 99, 71),    // coral red
        Category::Audio => (175, 95, 215),   // violet
        Category::Docs => (240, 215, 130),   // pale yellow
        Category::Archive => (255, 140, 60), // orange
        Category::Binary => (255, 135, 117), // salmon
        Category::Data => (135, 175, 95),    // sage green
        Category::Cache => (110, 110, 110),  // dim grey
        Category::Other => (149, 145, 100),  // muted khaki
    }
}

/// Deeper, ~45 % lightness hues for light terminals.
fn light_rgb(cat: Category) -> (u8, u8, u8) {
    match cat {
        Category::Code => (32, 96, 196),    // deep blue
        Category::Image => (160, 56, 158),  // mulberry
        Category::Video => (200, 50, 32),   // tomato
        Category::Audio => (118, 56, 178),  // deep violet
        Category::Docs => (170, 130, 30),   // dark goldenrod
        Category::Archive => (192, 88, 16), // burnt orange
        Category::Binary => (188, 64, 50),  // brick red
        Category::Data => (74, 124, 48),    // forest green
        Category::Cache => (170, 170, 170), // light grey (de-emphasised)
        Category::Other => (110, 100, 60),  // dark khaki
    }
}

/// Grayscale fallback for `NO_COLOR`. Hue is encoded as a lightness step so
/// categories remain distinguishable, just dimmer.
fn mono_rgb(cat: Category) -> (u8, u8, u8) {
    let level: u8 = match cat {
        Category::Code => 200,
        Category::Image => 180,
        Category::Video => 220,
        Category::Audio => 160,
        Category::Docs => 230,
        Category::Archive => 140,
        Category::Binary => 120,
        Category::Data => 170,
        Category::Cache => 80,
        Category::Other => 100,
    };
    (level, level, level)
}
