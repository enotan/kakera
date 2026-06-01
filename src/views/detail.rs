use crate::models::VisualNovel;
use dioxus::{desktop::tao::keyboard::Key::New, prelude::*};

///displays details for one selected vn
#[component]
pub fn DetailView(
    visual_novel: VisualNovel,
    on_notes_change: EventHandler<(u64, String)>,
    on_route_add: EventHandler<(u64, String)>,
    on_route_toggle: EventHandler<(u64, String)>,
) -> Element {
    let mut new_route_name = use_signal(String::new);
    let typed_route_name = new_route_name.read().clone();
    rsx! {
        section {
            h2 { "{visual_novel.title}" }

            if let Some(cover_url) = visual_novel.cover_url.clone() {
                img {
                    src: "{cover_url}",
                    alt: "Cover art for {visual_novel.title}",
                    width: "180"
                }
            }

            if let Some(description) = visual_novel.description.clone() {
                p { "{description}" }
            }

            h3 { "Notes" }

            textarea {
                value: "{visual_novel.notes}",

                oninput: move |event| {
                    on_notes_change.call((visual_novel.id, event.value()));
                }
            }

            h3 { "Routes" }
            p { "Routes tracked: {visual_novel.routes.len()}" }

            label {
                "New route"

                input {
                    value: "{typed_route_name}",

                    oninput: move |event| {
                        new_route_name.set(event.value());
                    }
                }
            }

            button {
                onclick: move |_| {
                    let route_name = new_route_name.read().clone();

                    if !route_name.is_empty() {
                        on_route_add.call((visual_novel.id, route_name));
                        new_route_name.set(String::new());
                    }
                },

                "Add route"
            }

            ul {
                for route in visual_novel.routes.clone() {
                    li {
                        label {
                            input {
                                r#type: "checkbox",
                                checked: route.completed,

                                onchange: move |_| {
                                    on_route_toggle.call((
                                        visual_novel.id,
                                        route.name.clone(),
                                    ));
                                }
                            }

                            "{route.name}"
                        }
                    }
                }
            }
        }
    }
}
