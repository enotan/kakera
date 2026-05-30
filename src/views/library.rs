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
                            p { "Routes tracked: {visual_novel.routes.len()}" }
                            p { "Play sessions: {visual_novel.play_sessions.len()}" }
                        }
                    }
                }
            }
        }
    }
}
