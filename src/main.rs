mod models;
mod storage;

use models::VisualNovel;
use storage::load_library;

use dioxus::prelude::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let library_count = match load_library() {
        Ok(library) => library.len(),
        Err(error) => {
            println!("Could not load library: {error}");
            0
        }
    };

    let example_vn = VisualNovel {
        id: 1,
        title: "Tsukihime".to_string(),
        cover_url: None,
        description: None,
        notes: String::new(),
        routes: Vec::new(),
        play_sessions: Vec::new(),
    };

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
        }
    }
}