use crate::models::VisualNovel;
use dioxus::prelude::*;

//displays the vn library as a simple list of cards
#[component]
pub fn LibraryView(vns: Vec<VisualNovel>, on_select: EventHandler<u64>) -> Element {
    let library_is_empty = vns.is_empty();
    rsx! {
        section { class: "library-section",

            h2 { "Library" }

            if library_is_empty {
                p { "No visual novels in your library yet." }
            } else {
                div { class: "vn-grid",

                    for vn in vns {
                        article {

                            class: "vn-card",

                            onclick: move |_| {
                                on_select.call(vn.id);
                            },

                            div { class: "vn-cover-frame",

                                if let Some(cover_src) = cover_source(vn.clone()) {
                                    img {
                                        class: "vn-cover",
                                        src: "{cover_src}",
                                        alt: "Cover art for {vn.title}",
                                    }
                                } else {
                                    div { class: "vn-cover-placeholder", "No cover" }
                                }
                            }

                            h3 { "{vn.title}" }
                        }
                    }
                }
            }
        }
    }
}

///if cover is cached, uses cached image, if not tries vndb
pub fn cover_source(vn: VisualNovel) -> Option<String> {
    match vn.cover_path {
        Some(path) => Some(path),
        None => vn.cover_url,
    }
}
