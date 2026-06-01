mod models;
mod storage;
mod views;
mod vndb;
mod launcher;

use models::{VisualNovel, StoryRoute, LaunchMode, PlaySession};
use storage::{load_library, save_library, add_play_session_to_library};
use views::{AddVnForm, DetailView, LibraryView, NewVN};
use launcher::launch_executable;

use dioxus::prelude::*;
use chrono::Utc;
use std::thread;
use std::time::Instant;

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
                        executable_path: None,
                        launch_mode: LaunchMode::default(),
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
                    },

                    on_executable_path_change: move |(id, path): (u64, String)| {
                        let executable_path = if path.is_empty() {
                            None
                        } else {
                            Some(path)
                        };

                        for visual_novel in visual_novels.write().iter_mut() {
                            if visual_novel.id == id {
                                visual_novel.executable_path = executable_path.clone();
                            }
                        }

                        let save_result = save_library(visual_novels.read().clone());

                        if let Err(error) = save_result {
                            println!("Could not save executable path: {error}");
                        }
                    },

                    on_launch: move |id| {
                        let visual_novel = visual_novels
                            .read()
                            .iter()
                            .find(|visual_novel| visual_novel.id == id)
                            .cloned();

                        match visual_novel {
                            Some(visual_novel) => {
                                match visual_novel.executable_path {
                                    Some(path) => {
                                        match launch_executable(path, visual_novel.launch_mode) {
                                            Ok(mut child) => {
                                                let started_at = Utc::now().to_rfc3339();
                                                let started_timer = Instant::now();
                                                let visual_novel_id = visual_novel.id;

                                                //use new thread so not to freeze the app
                                                thread::spawn(move || {
                                                    let wait_result = child.wait();

                                                    match wait_result {
                                                        Ok(_status) => {
                                                            let duration_seconds =
                                                                started_timer.elapsed().as_secs();

                                                            let play_session = PlaySession {
                                                                visual_novel_id,
                                                                started_at: started_at.clone(),
                                                                duration_seconds,
                                                                notes: None,
                                                            };

                                                            let save_result = add_play_session_to_library(visual_novel_id, play_session);

                                                            match save_result {
                                                                Ok(()) => {
                                                                    println!("VN {visual_novel_id} closed after {duration_seconds} seconds and was saved.");
                                                                }
                                                                
                                                                Err(error) => {
                                                                    println!("Could not save measured play session: {error}");
                                                                }
                                                            }
                                                        }

                                                        Err(error) => {
                                                            println!("Could not monitor VN process: {error}");
                                                        }
                                                    }
                                                });
                                            }

                                            Err(error) => {
                                                println!("Could not launch VN: {error}");
                                            }
                                        }
                                    }

                                    None => {
                                        println!("No executable path saved for this VN.");
                                    }
                                }
                            }

                            None => {
                                println!("Could not find VN with id {id}.");
                            }
                        }
                        
                    },
                
                    on_launch_mode_change: move |(id, launch_mode): (u64, LaunchMode)| {
                        for visual_novel in visual_novels.write().iter_mut() {
                            if visual_novel.id == id {
                                visual_novel.launch_mode = launch_mode.clone();
                            }
                        }

                        let save_result = save_library(visual_novels.read().clone());

                        if let Err(error) = save_result {
                            println!("Could not save launch mode: {error}");
                        }
                    }
                }
            }
        }
    }
}
