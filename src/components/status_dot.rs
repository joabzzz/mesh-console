use dioxus::prelude::*;

#[derive(Clone, PartialEq)]
#[allow(dead_code)]
pub enum DotState {
    Online,
    Offline,
    Loading,
}

#[component]
pub fn StatusDot(state: DotState) -> Element {
    let class = match state {
        DotState::Online => "status-dot status-dot--online",
        DotState::Offline => "status-dot status-dot--offline",
        DotState::Loading => "status-dot status-dot--loading",
    };
    rsx! {
        span { class: "{class}" }
    }
}
