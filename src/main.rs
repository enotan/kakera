mod models;
mod storage;
mod views;

use models::VisualNovel;
use storage::load_library;
use views::LibraryView;

use dioxus::prelude::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let saved_library = match load_library() {
        Ok(library) => library,
        Err(error) => {
            println!("Could not load library: {error}");
            Vec::new()
        }
    };

    let library_count = saved_library.len();

    let example_vn = VisualNovel {
        id: 1,
        title: "Tsukihime".to_string(),
        cover_url: None,
        description: None,
        notes: String::new(),
        routes: Vec::new(),
        play_sessions: Vec::new(),
    };

    let visual_novels = use_signal(move || saved_library);

    let description_text = match example_vn.description.clone() {
        Some(description) => description,
        None => "No description yet".to_string(),
    };

    rsx! {
        main {
            h1 { "Hello, Kakera" }
            p { "Your visual novel library starts here." }

            p { "Loaded VNs: {library_count}" }

            h2 { "Example VN" }
            p { "{example_vn.title}" }
            p { "{description_text}" }

            LibraryView {
                visual_novels: visual_novels.read().clone()
            }
        }
    }
}
