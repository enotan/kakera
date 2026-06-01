use crate::models::VisualNovel;
use dioxus::prelude::*;

//displays the vn library as a simple list of cards
#[component]
pub fn LibraryView(visual_novels: Vec<VisualNovel>) -> Element {
    let library_is_empty = visual_novels.is_empty();
    rsx! {
        section {
            h2 { "Library" }

            if library_is_empty {
                p { "No visual novels in your library yet."}
            } else {
                div {
                    for visual_novel in visual_novels {
                        article {
                            h3 { "{visual_novel.title}" }

                            if let Some(cover_url) = visual_novel.cover_url.clone() {
                                img {
                                    src: "{cover_url}",
                                    alt: "Cover art for {visual_novel.title}",
                                    width: "120"
                                }
                            }

                            if let Some(description) = visual_novel.description.clone() {
                                p { "{description}" }
                            }

                            p { "Routes tracked: {visual_novel.routes.len()}" }
                            p { "Play sessions: {visual_novel.play_sessions.len()}" }
                        }
                    }
                }
            }
        }
    }
}
