mod covers;
mod discord_presence;
mod launcher;
mod models;
mod storage;
mod system;
mod views;
mod vn_markup;
mod vndb;
mod wine;

use covers::cache_cover_image;
use discord_presence::DiscordPresence;
use launcher::launch_executable;
use models::{
    AppNotification, AppSettings, LaunchMode, NotificationLevel, PlaySession, StoryRoute,
    VisualNovel, default_umu_game_id,
};
use storage::{
    add_play_session_to_library, kakera_data_dir, load_library, load_settings, save_library,
    save_settings,
};
use system::{find_host_command, open_folder};
use views::{AddVnForm, DetailView, LibraryView, NewVN, SettingsView};

use chrono::Utc;
use dioxus::desktop::{Config, WindowBuilder, icon_from_memory, tao::window::ResizeDirection};
use dioxus::prelude::*;
use std::thread;
use std::time::Instant;

const MAIN_CSS: Asset = asset!("/assets/main.css");
const BG_IMAGE: Asset = asset!("/assets/hikarublur.png");
const LOGO_IMAGE: Asset = asset!("/assets/kakeralogo.png");
const STARS_IMAGE: Asset = asset!("/assets/stars.jpg");

fn main() {
    let config = Config::new().with_window(
        WindowBuilder::new()
            .with_title("Kakera")
            .with_decorations(false),
    );

    let config = match icon_from_memory(include_bytes!("../assets/favicon.ico")) {
        Ok(icon) => config.with_icon(icon),
        Err(error) => {
            eprintln!("Could not load app icon: {error}");
            config
        }
    };

    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .launch(App);
}

#[derive(Debug, Clone, PartialEq)]
enum AppView {
    Library,
    Settings,
}

///for resizing the window on linux
#[component]
fn ResizeHandle(class: String, direction: ResizeDirection) -> Element {
    let window = dioxus::desktop::window();

    rsx! {
        div {
            class: "resize-handle {class}",

            onmousedown: move |_| {
                if let Err(error) = window.drag_resize_window(direction) {
                    eprintln!("Could not begin resizing wndow: {error}");
                }
            },
        }
    }
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

    let saved_settings = match load_settings() {
        Ok(settings) => settings,
        Err(error) => {
            println!("Could not load settings: {error}");
            AppSettings::default()
        }
    };

    let mut vns = use_signal(move || saved_library);
    let mut selected_vn_id = use_signal(|| None::<u64>);
    let selected_vn = match *selected_vn_id.read() {
        Some(id) => vns.read().iter().find(|vn| vn.id == id).cloned(),
        None => None,
    };

    let mut settings = use_signal(move || saved_settings);

    let mut search_query = use_signal(String::new);
    let mut show_add_form = use_signal(|| false);

    let umu_is_available = use_hook(|| {
        std::env::var_os("FLATPAK_ID").is_some()
            || find_host_command("umu-run".to_string()).is_some()
    });

    let mut notification = use_signal(|| {
        if cfg!(target_os = "linux") && !umu_is_available {
            Some(AppNotification {
                level: NotificationLevel::Warning,
                message: "UMU Launcher was not found. Proton launches will not work until UMU is installed."
                    .to_string(),
            })
        } else {
            None
        }
    });

    let mut current_view = use_signal(|| AppView::Library);
    let selected_view = current_view.read().clone();

    let search_text = search_query.read().to_lowercase();

    let filtered_vns: Vec<VisualNovel> = vns
        .read()
        .iter()
        .filter(|vn| vn.title.to_lowercase().contains(&search_text))
        .cloned()
        .collect();

    let desktop = dioxus::desktop::window();
    let drag_win = desktop.clone();
    let min_win = desktop.clone();
    let max_win = desktop.clone();
    let close_win = desktop.clone();

    let data_dir_text = match kakera_data_dir() {
        Ok(path) => path.display().to_string(),
        Err(error) => format!("Could not find data folder: {error}"),
    };

    let wine_runners = use_hook(wine::detect_wine_runners);
    let proton_runners = use_hook(wine::detect_proton_runners);
    let steam_prefixes = use_hook(wine::detect_steam_prefixes);

    rsx! {

        document::Link { rel: "stylesheet", href: MAIN_CSS }

        main {
            class: "app-frame",
            style: "--app-bg-image: url('{BG_IMAGE}'); --stars-image: url('{STARS_IMAGE}');",

            //window resize handles
            ResizeHandle { class: "resize-north", direction: ResizeDirection::North }
            ResizeHandle {
                class: "resize-north-east",
                direction: ResizeDirection::NorthEast,
            }
            ResizeHandle { class: "resize-east", direction: ResizeDirection::East }
            ResizeHandle {
                class: "resize-south-east",
                direction: ResizeDirection::SouthEast,
            }
            ResizeHandle { class: "resize-south", direction: ResizeDirection::South }
            ResizeHandle {
                class: "resize-south-west",
                direction: ResizeDirection::SouthWest,
            }
            ResizeHandle { class: "resize-west", direction: ResizeDirection::West }
            ResizeHandle {
                class: "resize-north-west",
                direction: ResizeDirection::NorthWest,
            }

            div { class: "star-overlay" }
            //titlebar
            div { class: "win-titlebar",

                //window drag region
                div {
                    class: "win-drag-region",

                    onmousedown: move |_| {
                        drag_win.drag();
                    },

                    span { class: "win-title", "Kakera" }
                }

                //win controls
                div { class: "win-controls",

                    button {
                        class: "win-control-button",
                        onclick: move |_| {
                            min_win.set_minimized(true);
                        },

                        "-"
                    }

                    button {
                        class: "win-control-button",
                        onclick: move |_| {
                            max_win.toggle_maximized();
                        },

                        "□"
                    }

                    button {
                        class: "win-control-button close",
                        onclick: move |_| {
                            close_win.close();
                        },

                        "×"
                    }
                }
            }

            if let Some(active_notification) = notification.read().clone() {
                div {
                    class: match active_notification.level {
                        NotificationLevel::Info => "notification-banner info",
                        NotificationLevel::Warning => "notification-banner warning",
                        NotificationLevel::Error => "notification-banner error",
                    },

                    span { "{active_notification.message}" }

                    button {
                        class: "notification-dismiss",
                        title: "Dismiss notification",

                        onclick: move |_| {
                            notification.set(None);
                        },

                        "×"
                    }
                }
            }

            div { class: "app-body",

                //the side bar to the left
                aside { class: "sidebar",

                    //kakera logo
                    div { class: "logo",

                        img {
                            class: "logo-image",
                            src: LOGO_IMAGE,
                            alt: "Kakera",
                        }
                    }

                    nav { class: "sidebar-nav",

                        button {
                            class: if selected_view == AppView::Library { "nav-item active" } else { "nav-item" },

                            onclick: move |_| {
                                current_view.set(AppView::Library);
                            },

                            "Library"
                        }
                        button {
                            class: if selected_view == AppView::Settings { "nav-item active" } else { "nav-item" },

                            onclick: move |_| {
                                current_view.set(AppView::Settings);
                            },

                            "Settings"
                        }
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

                    if selected_view == AppView::Library {
                        div { class: "app-layout",
                            section { class: "library-column",

                                //when pressing the + to add a vn
                                if *show_add_form.read() {
                                    AddVnForm {
                                        on_add: move |new_vn: NewVN| {
                                            let next_id = vns
                                                .read()
                                                .iter()
                                                .map(|vn| vn.id)
                                                .max()
                                                .unwrap_or(0)
                                                + 1;

                                            let cover_url = new_vn.cover_url.clone();

                                            let new_vn = VisualNovel {
                                                id: next_id,
                                                title: new_vn.title,
                                                cover_url: new_vn.cover_url,
                                                description: new_vn.description,
                                                cover_path: new_vn.cover_path,
                                                executable_path: None,
                                                launch_mode: LaunchMode::default(),
                                                wine_binary: None,
                                                wine_prefix: None,
                                                wine_locale: None,
                                                launch_arguments: String::new(),
                                                proton_path: None,
                                                umu_game_id: default_umu_game_id(),
                                                notes: String::new(),
                                                routes: Vec::new(),
                                                active_route: None,
                                                play_sessions: Vec::new(),
                                            };

                                            vns.write().push(new_vn);

                                            if let Some(cover_url) = cover_url {
                                                let mut vns_for_cover = vns;

                                                spawn(async move {
                                                    match cache_cover_image(next_id, cover_url).await {
                                                        Ok(cover_path) => {
                                                            for vn in vns_for_cover
                                                                .write()
                                                                .iter_mut()
                                                            {
                                                                if vn.id == next_id {
                                                                    vn.cover_path = Some(cover_path.clone());
                                                                }
                                                            }
                                                            let save_result = save_library(vns_for_cover.read().clone());
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

                                            let save_result = save_library(vns.read().clone());

                                            if let Err(error) = save_result {
                                                println!("Could not save library: {error}");
                                            }
                                        },
                                    }
                                }

                                LibraryView {
                                    vns: filtered_vns,
                                    on_select: move |id| {
                                        selected_vn_id.set(Some(id));

                                    },
                                }
                            }

                            //the detail side bar (on the right)
                            aside { class: "detail-column",
                                //a sidebar that shows details for the vn
                                if let Some(vn) = selected_vn {
                                    DetailView {
                                        key: "{vn.id}",
                                        vn,
                                        wine_runners: wine_runners.clone(),
                                        proton_runners: proton_runners.clone(),
                                        steam_prefixes: steam_prefixes.clone(),
                                        on_notes_change: move |(id, notes): (u64, String)| {
                                            update_vn_and_save(
                                                &mut vns,
                                                id,
                                                move |vn| {
                                                    vn.notes = notes;
                                                },
                                                "Could not save notes".to_string(),
                                            );
                                        },

                                        //on adding route
                                        on_route_add: move |(id, route_name)| {
                                            let new_route = StoryRoute {
                                                name: route_name,
                                                completed: false,
                                                notes: None,
                                            };

                                            update_vn_and_save(
                                                &mut vns,
                                                id,
                                                move |vn| {
                                                    vn.routes.push(new_route);
                                                },
                                                "Could not save route".to_string(),
                                            );
                                        },

                                        //when toggling a route as completed / uncompleted
                                        on_route_toggle: move |(id, route_name)| {
                                            update_vn_and_save(
                                                &mut vns,
                                                id,
                                                move |vn| {
                                                    for route in vn.routes.iter_mut() {
                                                        if route.name == route_name {
                                                            route.completed = !route.completed;
                                                            break;
                                                        }
                                                    }
                                                },
                                                "Could not save route completion".to_string(),
                                            );
                                        },

                                        //when changing the vn path
                                        on_executable_path_change: move |(id, path): (u64, String)| {
                                            let executable_path = if path.is_empty() { None } else { Some(path) };

                                            update_vn_and_save(
                                                &mut vns,
                                                id,
                                                move |vn| {
                                                    vn.executable_path = executable_path;
                                                },
                                                "Could not save executable path".to_string(),
                                            );
                                        },

                                        //when launching a vn
                                        on_launch: move |id| {
                                            let vn = vns
                                                .read()
                                                .iter()
                                                .find(|vn| vn.id == id)
                                                .cloned();

                                            match vn {
                                                Some(vn) => {
                                                    let presence_vn = vn.clone();

                                                    match vn.executable_path {
                                                        Some(path) => {
                                                            match launch_executable(
                                                                path,
                                                                vn.launch_mode,
                                                                vn.wine_binary,
                                                                vn.wine_prefix,
                                                                vn.wine_locale,
                                                                vn.proton_path,
                                                                vn.umu_game_id,
                                                                vn.launch_arguments,
                                                            ) {
                                                                Ok(mut child) => {
                                                                    let started_time = Utc::now();
                                                                    let started_at = started_time.to_rfc3339();
                                                                    let started_timer = Instant::now();
                                                                    let vn_id = presence_vn.id;

                                                                    let discord_presence = if settings
                                                                        .read()
                                                                        .discord_rich_presence_enabled
                                                                    {
                                                                        match DiscordPresence::start_for_vn(
                                                                            &presence_vn,
                                                                            started_time,
                                                                            settings.read().clone(),
                                                                        ) {
                                                                            Ok(presence) => Some(presence),
                                                                            Err(error) => {
                                                                                println!("Could not start Discord Rich Presence: {error}");
                                                                                None
                                                                            }
                                                                        }
                                                                    } else {
                                                                        None
                                                                    };
                                                                    thread::spawn(move || {
                                                                        let wait_result = child.wait();
                                                                        match wait_result {
                                                                            Ok(_status) => {
                                                                                let duration_seconds = started_timer.elapsed().as_secs();
                                                                                let play_session = PlaySession {
                                                                                    vn_id,
                                                                                    started_at: started_at.clone(),
                                                                                    duration_seconds,
                                                                                    notes: None,
                                                                                };
                                                                                let save_result = add_play_session_to_library(
                                                                                    vn_id,
                                                                                    play_session,
                                                                                );
                                                                                match save_result {
                                                                                    Ok(()) => {
                                                                                        println!(
                                                                                            "VN {vn_id} closed after {duration_seconds} seconds and was saved.",
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
                                                                        if let Some(presence) = discord_presence {
                                                                            if let Err(error) = presence.clear() {
                                                                                println!("Could not clear Discord Rich Presence: {error}");
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
                                            update_vn_and_save(
                                                &mut vns,
                                                id,
                                                move |vn| {
                                                    vn.launch_mode = launch_mode;
                                                },
                                                "Could not save launch mode".to_string(),
                                            );
                                        },

                                        //when changing wine binary
                                        on_wine_binary_change: move |(id, binary): (u64, String)| {
                                            let wine_binary = if binary.is_empty() { None } else { Some(binary) };
                                            update_vn_and_save(
                                                &mut vns,
                                                id,
                                                move |vn| {
                                                    vn.wine_binary = wine_binary;
                                                },
                                                "Could not save Wine binary".to_string(),
                                            );
                                        },

                                        //when changing proton
                                        on_proton_path_change: move |(id, path): (u64, String)| {
                                            let proton_path = if path.is_empty() { None } else { Some(path) };
                                            update_vn_and_save(
                                                &mut vns,
                                                id,
                                                move |vn| {
                                                    vn.proton_path = proton_path;
                                                },
                                                "Could not save Proton path".to_string(),
                                            );
                                        },

                                        //when changing umu game id
                                        on_umu_game_id_change: move |(id, game_id): (u64, String)| {
                                            let game_id = if game_id.trim().is_empty() {
                                                default_umu_game_id()
                                            } else {
                                                game_id
                                            };

                                            update_vn_and_save(
                                                &mut vns,
                                                id,
                                                move |vn| {
                                                    vn.umu_game_id = game_id;
                                                },
                                                "Could not save UMU game ID".to_string(),
                                            );
                                        },

                                        //when changing wine prefix
                                        on_wine_prefix_change: move |(id, prefix): (u64, String)| {
                                            let wine_prefix = if prefix.is_empty() { None } else { Some(prefix) };
                                            update_vn_and_save(
                                                &mut vns,
                                                id,
                                                move |vn| {
                                                    vn.wine_prefix = wine_prefix;

                                                },
                                                "Could not save Wine prefix".to_string(),
                                            );
                                        },

                                        //when changing wine locale
                                        on_wine_locale_change: move |(id, locale): (u64, String)| {
                                            let wine_locale = if locale.is_empty() { None } else { Some(locale) };

                                            update_vn_and_save(
                                                &mut vns,
                                                id,
                                                move |vn| {
                                                    vn.wine_locale = wine_locale;
                                                },
                                                "Could not save Wine locale".to_string(),
                                            );
                                        },

                                        //when adding wine launch arguments
                                        on_launch_arguments_change: move |(id, arguments): (u64, String)| {
                                            update_vn_and_save(
                                                &mut vns,
                                                id,
                                                move |vn| {
                                                    vn.launch_arguments = arguments;
                                                },
                                                "Could not save launch arguments".to_string(),
                                            );
                                        },

                                        //when deleting a vn
                                        on_delete: move |id| {
                                            vns.write().retain(|vn| { vn.id != id });
                                            selected_vn_id.set(None);

                                            save_vns_or_log(&vns, "Could not delete VN".to_string());
                                        },

                                        //when changing desc
                                        on_description_change: move |(id, description): (u64, String)| {
                                            let description = if description.is_empty() { None } else { Some(description) };
                                            update_vn_and_save(
                                                &mut vns,
                                                id,
                                                move |vn| {
                                                    vn.description = description;
                                                },

                                                "Could not save description".to_string(),
                                            );
                                        },

                                        //when changing cover
                                        on_cover_path_change: move |(id, cover_path): (u64, String)| {
                                            update_vn_and_save(
                                                &mut vns,
                                                id,
                                                move |vn| {
                                                    vn.cover_path = Some(cover_path);
                                                },

                                                "Could not save cover image".to_string(),
                                            );
                                        },

                                        //when changing active route
                                        on_active_route_change: move |(id, active_route): (u64, Option<String>)| {
                                            update_vn_and_save(
                                                &mut vns,
                                                id,
                                                move |vn| {
                                                    vn.active_route = active_route;
                                                },
                                                "Could not save active route".to_string(),
                                            );
                                        },

                                        //when deleting route
                                        on_route_delete: move |(id, route_name): (u64, String)| {
                                            update_vn_and_save(
                                                &mut vns,
                                                id,
                                                move |vn| {
                                                    vn.routes.retain(|route| route.name != route_name);

                                                    if vn.active_route == Some(route_name) {
                                                        vn.active_route = None;
                                                    }
                                                },

                                                "Could not delete route".to_string(),
                                            );
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
                    } else {
                        SettingsView {
                            discord_rich_presence_enabled: settings.read().discord_rich_presence_enabled,
                            data_dir_text,

                            on_discord_rich_presence_change: move |enabled| {
                                settings.write().discord_rich_presence_enabled = enabled;

                                let save_result = save_settings(settings.read().clone());

                                if let Err(error) = save_result {
                                    println!("Could not save settings: {error}");
                                }
                            },

                            on_open_data_folder: move |_| {
                                match kakera_data_dir() {
                                    Ok(path) => {
                                        if let Err(error) = open_folder(&path) {
                                            println!("Could not open data folder: {error}");
                                        }
                                    }
                                    Err(error) => {
                                        println!("Could not find data folder: {error}");
                                    }
                                }
                            },

                            discord_status_text: settings.read().discord_status_text.clone(),
                            discord_show_active_route: settings.read().discord_show_active_route,
                            discord_custom_cover_url: settings.read().discord_custom_cover_url.clone(),

                            on_discord_status_text_change: move |status_text| {
                                settings.write().discord_status_text = status_text;
                                save_settings_or_log(&settings);
                            },

                            on_discord_show_active_route_change: move |enabled| {
                                settings.write().discord_show_active_route = enabled;
                                save_settings_or_log(&settings);
                            },

                            on_discord_custom_cover_url_change: move |cover_url| {
                                settings.write().discord_custom_cover_url = cover_url;
                                save_settings_or_log(&settings);
                            },
                        }
                    }

                }

            }

        }
    }
}

///updates one vn then saves the whole library
fn update_vn_and_save(
    vns: &mut Signal<Vec<VisualNovel>>,
    id: u64,
    update: impl FnOnce(&mut VisualNovel),
    error_message: String,
) {
    for vn in vns.write().iter_mut() {
        if vn.id == id {
            update(vn);
            break;
        }
    }

    save_vns_or_log(vns, error_message);
}

///saves the vn library or prints an error
fn save_vns_or_log(vns: &Signal<Vec<VisualNovel>>, error_message: String) {
    let save_result = save_library(vns.read().clone());

    if let Err(error) = save_result {
        println!("{error_message}: {error}");
    }
}

///saves the settings or prints error
fn save_settings_or_log(settings: &Signal<AppSettings>) {
    let save_result = save_settings(settings.read().clone());

    if let Err(error) = save_result {
        println!("Could not save settings: {error}");
    }
}
