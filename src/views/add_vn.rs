use crate::vndb::{VndbSearchResult, search_vns};
use dioxus::prelude::*;

///the data sent back to the app when adding a vn from vndb
#[derive(Debug, Clone, PartialEq)]
pub struct NewVN {
    pub title: String,
    pub cover_url: Option<String>,
    pub cover_path: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
}

///the form to add a visual novel
#[component]
pub fn AddVnForm(on_add: EventHandler<NewVN>, on_close: EventHandler<()>) -> Element {
    let mut title = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut cover_path = use_signal(|| None::<String>);
    let mut cover_url = use_signal(|| None::<String>);

    let description_text = description.read().clone();
    let cover_path_text = cover_path.read().clone();

    let mut tags_text = use_signal(String::new);
    let current_tags_text = tags_text.read().clone();

    let mut search_results: Signal<Vec<VndbSearchResult>> = use_signal(Vec::new);
    let mut search_message = use_signal(String::new);

    let mut title_has_error = use_signal(|| false);
    let mut loaded_result_id = use_signal(|| None::<String>);

    let message_text = search_message.read().clone();
    let results_to_show = search_results.read().clone();

    rsx! {
        section { class: "add-vn-panel",

            h2 { "Add VN" }

            button {
                class: "add-vn-close",
                aria_label: "Close add VN form",

                onclick: move |_| {
                    on_close.call(());
                },

                "×"
            }

            //title input
            label {
                "Title"

                input {
                    class: if *title_has_error.read() { "field-invalid" } else { "" },
                    value: "{title}",

                    oninput: move |event| {
                        let value = event.value();
                        if !value.trim().is_empty() {
                            title_has_error.set(false);
                        }

                        title.set(value);
                    },
                }
            }

            //description input
            label {
                "Description"

                textarea {
                    value: "{description_text}",

                    oninput: move |event| {
                        description.set(event.value());
                    },
                }
            }

            //add tags
            label {
                "Tags"

                input {
                    value: "{current_tags_text}",
                    placeholder: "backlog, mystery, eroge",

                    oninput: move |event| {
                        tags_text.set(event.value());
                    },

                }
            }

            //button to pick image
            button {
                class: "fp-button",

                onclick: move |_| {
                    let picked_file = rfd::FileDialog::new()
                        .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
                        .pick_file();

                    if let Some(path) = picked_file {
                        cover_path.set(Some(path.to_string_lossy().to_string()));
                        cover_url.set(None);
                    }
                },

                "Choose cover image"
            }

            if let Some(path) = cover_path_text {
                p { "Cover: {path}" }
            }

            //button to add a vn
            button {
                onclick: move |_| {
                    let typed_title = title.read().trim().to_string();

                    if typed_title.is_empty() {
                        title_has_error.set(true);
                        search_message.set("A title is required".to_string());
                        return;
                    }

                    on_add
                        .call(NewVN {
                            title: typed_title,
                            cover_url: cover_url.read().clone(),
                            cover_path: cover_path.read().clone(),
                            description: if description.read().is_empty() {
                                None
                            } else {
                                Some(description.read().clone())
                            },
                            tags: parse_tags(tags_text.read().clone()),
                        });
                    title.set(String::new());
                    description.set(String::new());
                    cover_path.set(None);
                    cover_url.set(None);
                    loaded_result_id.set(None);
                    tags_text.set(String::new());
                },

                "Add to library"
            }

            button {
                onclick: move |_| {
                    let query = title.read().clone();

                    if query.trim().is_empty() {
                        title_has_error.set(true);
                        search_message.set("Type a title before searching VNDB".to_string());
                        return;
                    }

                    loaded_result_id.set(None);

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

            p { "{message_text}" }

            div {
                div { class: "vndb-results-grid",
                    for result in results_to_show {
                        {
                            let result_title = result.title.clone();
                            let result_description = result.description.clone();
                            let result_cover_url = result.image.clone().and_then(|image| image.url);
                            let result_id = result.id.clone();
                            let result_is_loaded = loaded_result_id.read().as_ref() == Some(&result.id);

                            rsx! {
                                article { class: "vndb-result-card",
                                    div { class: "vndb-result-cover-frame",

                                        if let Some(cover_src) = result_cover_url.clone() {
                                            img {
                                                class: "vndb-result-cover",
                                                src: "{cover_src}",
                                                alt: "Cover art for {result.title}",
                                            }
                                        } else {
                                            div { class: "vndb-result-cover-placeholder", "No cover" }
                                        }
                                    }

                                    div { class: "vndb-result-body",
                                        h3 { "{result.title}" }
                                        p { "VNDB ID: {result.id}" }

                                        button {
                                            class: if result_is_loaded { "vndb-result-button loaded" } else { "vndb-result-button" },

                                            onclick: move |_| {
                                                title.set(result_title.clone());
                                                description.set(result_description.clone().unwrap_or_default());
                                                cover_url.set(result_cover_url.clone());
                                                cover_path.set(None);
                                                loaded_result_id.set(Some(result_id.clone()));
                                                search_message.set(format!("Loaded {}.", result_title));
                                            },

                                            if result_is_loaded {
                                                "Loaded"
                                            } else {
                                                "Use"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

///turns comma separated tags into a vector
fn parse_tags(text: String) -> Vec<String> {
    text.split(",")
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect()
}
