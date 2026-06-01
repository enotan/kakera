mod models;
mod storage;
mod views;
mod vndb;

use models::VisualNovel;
use storage::{
    load_library,
    save_library,
};
use views::{
    AddVnForm,
    LibraryView,
    NewVN,
};

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

    let mut visual_novels = use_signal(move || saved_library);


    rsx! {
        main {
            h1 { "Hello, Kakera" }
            p { "Your visual novel library starts here." }

            p { "Loaded VNs: {library_count}" }

            AddVnForm {
                on_add: move |new_vn: NewVN| {
                    let next_id = visual_novels.read().len() as u64 + 1;

                    let new_vn = VisualNovel {
                        id: next_id,
                        title: new_vn.title,
                        cover_url: new_vn.cover_url,
                        description: new_vn.description,
                        notes: String::new(),
                        routes: Vec::new(),
                        play_sessions: Vec::new(),
                    };

                    visual_novels.write().push(new_vn);

                    let save_result = save_library(visual_novels.read().clone());

                    if let Err(error) = save_result {
                        println!("Could not save library: {error}");
                    }
                }
            }

            LibraryView {
                visual_novels: visual_novels.read().clone()
            }
        }
    }
}
