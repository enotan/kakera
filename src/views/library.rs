use crate::models::VisualNovel;
use base64::Engine;
use dioxus::prelude::*;
use std::fs;

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

///returns an image source that webview can display for windows users
pub fn cover_source(vn: VisualNovel) -> Option<String> {
    match vn.cover_path {
        Some(path) => local_cover_source(path),
        None => vn.cover_url,
    }
}

///converts a saved local cover path into a webview image source
fn local_cover_source(path: String) -> Option<String> {
    match local_path_to_data_url(&path) {
        Some(data_url) => Some(data_url),
        None => {
            if cfg!(target_os = "windows") {
                None
            } else {
                Some(path)
            }
        }
    }
}

///reads a local image file and turns it into data:image/... url
fn local_path_to_data_url(path: &str) -> Option<String> {
    let image_bytes = fs::read(path).ok()?;
    let mime_type = image_mime_type(path);

    let encoded_image = base64::engine::general_purpose::STANDARD.encode(image_bytes);

    Some(format!("data:{mime_type};base64,{encoded_image}"))
}

///match mime type to file extension
fn image_mime_type(path: &str) -> &'static str {
    let lower_path = path.to_lowercase();

    if lower_path.ends_with(".png") {
        "image/png"
    } else if lower_path.ends_with(".webp") {
        "image/webp"
    } else {
        "image/jpeg"
    }
}
