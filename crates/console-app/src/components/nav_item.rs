use dioxus::prelude::*;

#[component]
pub fn NavItem(label: String, active: bool, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            class: if active { "nav-item nav-item--active" } else { "nav-item" },
            onclick: move |e| onclick.call(e),
            span { class: "nav-label", "{label}" }
        }
    }
}
