use crate::models::VisualNovel;
use dioxus::prelude::*;

//displays the vn library as a simple list of cards
#[component]
pub fn LibraryView(visual_novels: Vec<VisualNovel>, on_select: EventHandler<u64>) -> Element {
    let library_is_empty = visual_novels.is_empty();
    rsx! {
        section { class: "library-section",

            h2 { "Library" }

            if library_is_empty {
                p { "No visual novels in your library yet." }
            } else {
                div { class: "vn-grid",

                    for visual_novel in visual_novels {
                        article {

                            class: "vn-card",

                            onclick: move |_| {
                                on_select.call(visual_novel.id);
                            },

                            div {
                                class: "vn-cover-frame",

                                if let Some(cover_src) = cover_source(visual_novel.clone()) {
                                    img {
                                        class: "vn-cover",
                                        src: "{cover_src}",
                                        alt: "Cover art for {visual_novel.title}",
                                    }
                                } else {
                                    div {
                                        class: "vn-cover-placeholder",
                                        "No cover"
                                    }
                                }
                            }

                            h3 { "{visual_novel.title}" }
                        }
                    }
                }
            }
        }
    }
}


///if cover is cached, uses cached image, if not tries vndb
pub fn cover_source(visual_novel: VisualNovel) -> Option<String> {
    match visual_novel.cover_path {
        Some(path) => Some(path),
        None => visual_novel.cover_url,
    }
}