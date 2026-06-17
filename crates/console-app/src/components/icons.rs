//! Minimal monochrome line icons (16px grid, 1.5px stroke, currentColor).
//! Geometry only — no filled shapes, matching the hairline design language.

use dioxus::prelude::*;

/// Models: 2×2 grid.
#[component]
pub fn IconGrid() -> Element {
    rsx! {
        svg {
            view_box: "0 0 16 16",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "1.5",
            rect { x: "2", y: "2", width: "5", height: "5" }
            rect { x: "9", y: "2", width: "5", height: "5" }
            rect { x: "2", y: "9", width: "5", height: "5" }
            rect { x: "9", y: "9", width: "5", height: "5" }
        }
    }
}

/// Configuration: three sliders.
#[component]
pub fn IconSliders() -> Element {
    rsx! {
        svg {
            view_box: "0 0 16 16",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "1.5",
            stroke_linecap: "round",
            line { x1: "2", y1: "4", x2: "14", y2: "4" }
            line { x1: "2", y1: "8", x2: "14", y2: "8" }
            line { x1: "2", y1: "12", x2: "14", y2: "12" }
            circle { cx: "10", cy: "4", r: "1.6", fill: "var(--paper, #fff)" }
            circle { cx: "5", cy: "8", r: "1.6", fill: "var(--paper, #fff)" }
            circle { cx: "12", cy: "12", r: "1.6", fill: "var(--paper, #fff)" }
        }
    }
}

/// Settings: circle with radial ticks (minimal "dial").
#[component]
pub fn IconDial() -> Element {
    rsx! {
        svg {
            view_box: "0 0 16 16",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "1.5",
            stroke_linecap: "round",
            circle { cx: "8", cy: "8", r: "5.2" }
            line { x1: "8", y1: "8", x2: "10.6", y2: "5.4" }
        }
    }
}
