mod launcher;
mod models;
mod storage;
mod views;
mod vndb;
mod covers;

use launcher::launch_executable;
use models::{LaunchMode, PlaySession, StoryRoute, VisualNovel};
use storage::{add_play_session_to_library, load_library, save_library};
use views::{AddVnForm, DetailView, LibraryView, NewVN};
use covers::cache_cover_image;

use chrono::Utc;
use dioxus::prelude::*;
use std::thread;
use std::time::Instant;

const MAIN_CSS: Asset = asset!("/assets/main.css");

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

    let mut search_query = use_signal(String::new);
    let mut show_add_form = use_signal(|| false);

    let search_text = search_query.read().to_lowercase();

    let filtered_vns: Vec<VisualNovel> = visual_novels
        .read()
        .iter()
        .filter(|visual_novel| visual_novel.title.to_lowercase().contains(&search_text))
        .cloned()
        .collect();

    rsx! {

        document::Link { rel: "stylesheet", href: MAIN_CSS }

        main { class: "app-frame",

            //the side bar to the left
            aside { class: "sidebar",

                //will be replaced with a cool logo later
                div { class: "logo", "Kakera" }

                nav { class: "sidebar-nav",

                    button { class: "nav-item active", "Library" }
                    //these currently do nothing
                    button { class: "nav-item", "Statistics" }
                    button { class: "nav-item", "Settings" }
                }
            }

            section { class: "main-area",

                //the bar at the top
                header { class: "topbar",

                    //search bar
                    input {
                        class: "search-input",
                        placeholder: "Search visual novels...",
                        value: "{search_query}",

                        oninput: move |event| {
                            search_query.set(event.value());
                        },
                    }

                    //add vn button
                    button {
                        class: "icon-button",
                        onclick: move |_| {
                            let next_value = !*show_add_form.read();
                            show_add_form.set(next_value);
                        },

                        "+"
                    }
                }

                div { class: "app-layout",

                    section { class: "library-column",

                        //when pressing the + to add a vn
                        if *show_add_form.read() {
                            AddVnForm {
                                on_add: move |new_vn: NewVN| {
                                    let next_id = visual_novels
                                        .read()
                                        .iter()
                                        .map(|visual_novel| visual_novel.id)
                                        .max()
                                        .unwrap_or(0)
                                        + 1;

                                    let cover_url = new_vn.cover_url.clone();

                                    let new_vn = VisualNovel {
                                        id: next_id,
                                        title: new_vn.title,
                                        cover_url: new_vn.cover_url,
                                        description: new_vn.description,
                                        cover_path: None,
                                        executable_path: None,
                                        launch_mode: LaunchMode::default(),
                                        wine_prefix: None,
                                        wine_locale: None,
                                        launch_arguments: String::new(),
                                        notes: String::new(),
                                        routes: Vec::new(),
                                        play_sessions: Vec::new(),
                                    };

                                    visual_novels.write().push(new_vn);

                                    if let Some(cover_url) = cover_url {
                                        let mut visual_novels_for_cover = visual_novels;

                                        spawn(async move {
                                            match cache_cover_image(next_id, cover_url).await {
                                                Ok(cover_path) => {
                                                    for visual_novel in visual_novels_for_cover
                                                        .write()
                                                        .iter_mut()
                                                    {
                                                        if visual_novel.id == next_id {
                                                            visual_novel.cover_path = Some(cover_path.clone());
                                                        }
                                                    }

                                                    let save_result = save_library(
                                                        visual_novels_for_cover.read().clone(),
                                                    );

                                                    if let Err(error) = save_result {
                                                        println!("Could not save cached cover path: {error}");
                                                    }
                                                }

                                                Err(error) => {
                                                    println!("Could not cache cover image: {error}");
                                                }
                                            }
                                        });
                                    }

                                    let save_result = save_library(visual_novels.read().clone());

                                    if let Err(error) = save_result {
                                        println!("Could not save library: {error}");
                                    }
                                },
                            }
                        }

                        LibraryView {
                            visual_novels: filtered_vns,
                            on_select: move |id| {
                                selected_vn_id.set(Some(id));

                            },
                        }
                    }

                    //the detail side bar (on the right)
                    aside { class: "detail-column",
                        //a sidebar that shows details for the vn
                        if let Some(visual_novel) = selected_vn {
                            DetailView {
                                visual_novel,
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

                                //on adding route
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

                                //when toggling a route as completed / uncompleted
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

                                //when changing the vn path
                                on_executable_path_change: move |(id, path): (u64, String)| {
                                    let executable_path = if path.is_empty() { None } else { Some(path) };
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

                                //when launching a vn
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
                                                    match launch_executable(
                                                        path,
                                                        visual_novel.launch_mode,
                                                        visual_novel.wine_prefix,
                                                        visual_novel.wine_locale,
                                                        visual_novel.launch_arguments,
                                                    ) {
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

                                                                        let save_result = add_play_session_to_library(
                                                                            visual_novel_id,
                                                                            play_session,
                                                                        );
                                                                        match save_result {
                                                                            Ok(()) => {
                                                                                println!(
                                                                                    "VN {visual_novel_id} closed after {duration_seconds} seconds and was saved.",
                                                                                );
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

                                //when changing launch mode
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
                                },

                                //when changing wine prefix
                                on_wine_prefix_change: move |(id, prefix): (u64, String)| {
                                    let wine_prefix = if prefix.is_empty() {
                                        None
                                    } else {
                                        Some(prefix)
                                    };

                                    for visual_novel in visual_novels.write().iter_mut() {
                                        if visual_novel.id == id {
                                            visual_novel.wine_prefix = wine_prefix.clone();
                                        }
                                    }

                                    let save_result = save_library(visual_novels.read().clone());

                                    if let Err(error) = save_result {
                                        println!("Could not save Wine prefix: {error}");
                                    }
                                },

                                //when changing wine locale
                                on_wine_locale_change: move |(id, locale): (u64, String)| {
                                    let wine_locale = if locale.is_empty() {
                                        None
                                    } else {
                                        Some(locale)
                                    };

                                    for visual_novel in visual_novels.write().iter_mut() {
                                        if visual_novel.id == id {
                                            visual_novel.wine_locale = wine_locale.clone();
                                        }
                                    }

                                    let save_result = save_library(visual_novels.read().clone());

                                    if let Err(error) = save_result {
                                        println!("Could not save Wine locale: {error}");
                                    }
                                },

                                //when adding wine launch arguments
                                on_launch_arguments_change: move |(id, arguments): (u64, String)| {
                                    for visual_novel in visual_novels.write().iter_mut() {
                                        if visual_novel.id == id {
                                            visual_novel.launch_arguments = arguments.clone();
                                        }
                                    }

                                    let save_result = save_library(visual_novels.read().clone());

                                    if let Err(error) = save_result {
                                        println!("Could not save launch argumnets: {error}");
                                    }
                                },

                                //when deleting a vn
                                on_delete: move |id| {
                                    visual_novels.write().retain(|visual_novel| {
                                        visual_novel.id != id
                                    });

                                    selected_vn_id.set(None);

                                    let save_result = save_library(visual_novels.read().clone());

                                    if let Err(error) = save_result {
                                        println!("Could not delete VN: {error}");
                                    }
                                },
                            }
                        } else {
                            //displayed when no vn is selected
                            section { class: "empty-detail-panel",

                                h2 { "No VN selected" }
                                p {
                                    "Choose a visual novel from the library to view notes, routes, launch settings, and play sessions."
                                }
                            }
                        }
                    }
                }

            }

        }
    }
}
