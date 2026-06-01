use dioxus::prelude::*;
use crate::vndb::{
    search_vns,
    VndbSearchResult,
};

///the data sent back to the app when adding a vn from vndb
#[derive(Debug, Clone, PartialEq)]
pub struct NewVN {
    pub title: String,
    pub cover_url: Option<String>,
    pub description: Option<String>,
}

///the form to add a visual novel
#[component]
pub fn AddVnForm(on_add: EventHandler<NewVN>) -> Element {
    let mut title = use_signal(String::new);
    let mut search_results: Signal<Vec<VndbSearchResult>> = use_signal(Vec::new);
    let mut search_message = use_signal(String::new);

    let typed_title = title.read().clone();
    let message_text = search_message.read().clone();
    let results_to_show = search_results.read().clone();

    rsx! {
        section {
            h2 { "Add VN" }

            label {
                "Title"

                input {
                    value: "{title}",

                    oninput: move |event| {
                        title.set(event.value());
                    }
                }
            }

            button {
                onclick: move |_| {
                    let typed_title = title.read().clone();

                    if !typed_title.is_empty() {
                        on_add.call(NewVN {
                            title: typed_title,
                            cover_url: None,
                            description: None,
                        });
                        title.set(String::new());
                    }
                },

                "Add"
            }

            button {
                onclick: move |_| {
                    let query = title.read().clone();

                    if query.is_empty() {
                        search_message.set("Type a title before searching VNDB".to_string());
                        return;
                    }

                    search_message.set("Searching VNDB...".to_string());

                    spawn(async move {
                        match search_vns(query).await {
                            Ok(results) => {
                                search_message.set(format!("Found {} result(s)", results.len()));
                                search_results.set(results);
                            }
                            Err(error) => {
                                search_message.set(format!("VNDB search failed: {error}"));
                            }
                        }
                    });
                },

                "Search VNDB"
            }

            p { "Typed title: {typed_title}" }

            p { "{message_text}" }

            div {
                for result in results_to_show {
                    article {
                        h3 { "{result.title}" }
                        p { "VNDB ID: {result.id}" }

                        button {
                            onclick: move |_| {
                                on_add.call(NewVN {
                                    title: result.title.clone(),
                                    cover_url: result
                                        .image
                                        .clone()
                                        .and_then(|image| image.url),
                                    description: result.description.clone(),
                                });
                            },

                            "Add this result"
                        }
                    }
                }
            }
        }
    }
}

