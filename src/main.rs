mod models;
mod storage;
mod views;
mod vndb;

use models::{VisualNovel, StoryRoute};
use storage::{load_library, save_library};
use views::{AddVnForm, DetailView, LibraryView, NewVN};

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
    let mut selected_vn_id = use_signal(|| None::<u64>);
    let selected_vn = match *selected_vn_id.read() {
        Some(id) => visual_novels
            .read()
            .iter()
            .find(|visual_novel| visual_novel.id == id)
            .cloned(),
        None => None,
    };
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
                visual_novels: visual_novels.read().clone(),
                on_select: move |id| {
                    selected_vn_id.set(Some(id));


                }
            }

            if let Some(visual_novel) = selected_vn {
                DetailView {
                    visual_novel: visual_novel,
                    on_notes_change: move |(id, notes): (u64, String)| {
                        for visual_novel in visual_novels.write().iter_mut() {
                            if visual_novel.id == id {
                                visual_novel.notes = notes.clone();
                            }
                        }

                        let save_result = save_library(visual_novels.read().clone());

                        if let Err(error) = save_result {
                            println!("Could not save notes: {error}");
                        }
                    },

                    on_route_add: move |(id, route_name)| {
                        let new_route = StoryRoute {
                            name: route_name,
                            completed: false,
                            notes: None,
                        };

                        for visual_novel in visual_novels.write().iter_mut() {
                            if visual_novel.id == id {
                                visual_novel.routes.push(new_route.clone());
                            }
                        }

                        let save_result = save_library(visual_novels.read().clone());

                        if let Err(error) = save_result {
                            println!("Could not save route: {error}");
                        }
                    },
                
                    on_route_toggle: move |(id, route_name)| {
                        for visual_novel in visual_novels.write().iter_mut() {
                            if visual_novel.id == id {
                                for route in visual_novel.routes.iter_mut() {
                                    if route.name == route_name {
                                        route.completed = !route.completed;
                                    }
                                }
                            }
                        }

                        let save_result = save_library(visual_novels.read().clone());

                        if let Err(error) = save_result {
                            println!("Could not save route completion: {error}");
                        }
                    }
                }
            }
        }
    }
}
