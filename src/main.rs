mod models;

use models::VisualNovel;

use dioxus::prelude::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
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

            h2 { "Example VN" }
            p { "{example_vn.title}" }
            p { "{description_text}" }
        }
    }
}