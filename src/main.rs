mod covers;
mod discord_presence;
mod launcher;
mod logs;
mod models;
mod storage;
mod system;
mod views;
mod vn_markup;
mod vndb;
mod wine;

use covers::cache_cover_image;
use discord_presence::DiscordPresence;
use launcher::{launch_executable, parse_launch_environment};
use logs::{launch_logs_dir, new_launch_log_path, update_latest_launch_log};
use models::{
    AppNotification, AppSettings, LaunchMode, LibraryFilterMode, LibrarySortMode,
    NotificationLevel, PlaySession, StoryRoute, VisualNovel, default_umu_game_id,
};
use storage::{
    add_play_session_to_library, kakera_data_dir, load_library, load_settings, save_library,
    save_settings,
};
use system::{is_flatpak_document_portal_path, open_folder, umu_launcher_is_available};
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

    let selected_vn_id_value = *selected_vn_id.read();

    let selected_vn = match selected_vn_id_value {
        Some(id) => vns.read().iter().find(|vn| vn.id == id).cloned(),
        None => None,
    };

    let saved_library_sort_mode = saved_settings.library_sort_mode.clone();
    let saved_library_filter_mode = saved_settings.library_filter_mode.clone();

    let mut selected_tag_filter = use_signal(|| None::<String>);
    let selected_tag = selected_tag_filter.read().clone();

    let mut settings = use_signal(move || saved_settings);

    let mut search_query = use_signal(String::new);
    let mut show_add_form = use_signal(|| false);

    let mut library_sort_mode = use_signal(|| saved_library_sort_mode);
    let selected_sort_mode = library_sort_mode.read().clone();

    let mut library_filter_mode = use_signal(|| saved_library_filter_mode);
    let selected_filter_mode = library_filter_mode.read().clone();

    let umu_is_available = use_hook(umu_launcher_is_available);

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

    let mut available_tags: Vec<String> =
        vns.read().iter().flat_map(|vn| vn.tags.clone()).collect();

    available_tags.sort_by_key(|tag| tag.to_lowercase());
    available_tags.dedup_by(|a, b| a.eq_ignore_ascii_case(b));

    let mut filtered_vns: Vec<VisualNovel> = vns
        .read()
        .iter()
        .filter(|vn| {
            let matches_search = vn.title.to_lowercase().contains(&search_text);

            let matches_filter = match selected_filter_mode {
                LibraryFilterMode::All => true,
                LibraryFilterMode::Favourites => vn.is_favourite,
            };

            let matches_tag = match selected_tag.clone() {
                Some(tag) => vn
                    .tags
                    .iter()
                    .any(|vn_tag| vn_tag.eq_ignore_ascii_case(&tag)),
                None => true,
            };

            matches_search && matches_filter && matches_tag
        })
        .cloned()
        .collect();

    match selected_sort_mode {
        LibrarySortMode::Added => {}
        LibrarySortMode::TitleAsc => {
            filtered_vns.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        }
        LibrarySortMode::TitleDesc => {
            filtered_vns.sort_by(|a, b| b.title.to_lowercase().cmp(&a.title.to_lowercase()));
        }
        LibrarySortMode::LastPlayed => {
            filtered_vns.sort_by(|a, b| latest_played_at(b).cmp(&latest_played_at(a)));
        }
        LibrarySortMode::MostPlaytime => {
            filtered_vns.sort_by(|a, b| total_playtime_seconds(b).cmp(&total_playtime_seconds(a)));
        }
    }

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

                section { class: if selected_view == AppView::Library { "main-area" } else { "main-area settings-view" },

                    if selected_view == AppView::Library {
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

                            label { class: "topbar-select-field",
                                span { class: "topbar-select-label", "Sort by" }

                                div { class: "select-shell",

                                    //sort vns
                                    select {
                                        class: "sort-select",

                                        value: match selected_sort_mode {
                                            LibrarySortMode::Added => "added",
                                            LibrarySortMode::TitleAsc => "title-asc",
                                            LibrarySortMode::TitleDesc => "title-desc",
                                            LibrarySortMode::LastPlayed => "last-played",
                                            LibrarySortMode::MostPlaytime => "most-playtime",
                                        },

                                        onchange: move |event| {
                                            let sort_mode = match event.value().as_str() {
                                                "title-asc" => LibrarySortMode::TitleAsc,
                                                "title-desc" => LibrarySortMode::TitleDesc,
                                                "last-played" => LibrarySortMode::LastPlayed,
                                                "most-playtime" => LibrarySortMode::MostPlaytime,
                                                _ => LibrarySortMode::Added,
                                            };

                                            library_sort_mode.set(sort_mode.clone());
                                            settings.write().library_sort_mode = sort_mode;
                                            save_settings_or_log(&settings);
                                        },

                                        option { value: "added", "Added" }
                                        option { value: "title-asc", "A-Z" }
                                        option { value: "title-desc", "Z-A" }
                                        option { value: "last-played", "Last Played" }
                                        option { value: "most-playtime", "Most Playtime" }

                                    }

                                    span { class: "select-chevron", "⌄" }
                                }
                            }

                            div { class: "topbar-select-field",
                                span { class: "topbar-select-label", "View" }

                                div { class: "topbar-segmented-control",
                                    button {
                                        class: if selected_filter_mode == LibraryFilterMode::All { "topbar-segment active" } else { "topbar-segment" },

                                        onclick: move |_| {
                                            let filter_mode = LibraryFilterMode::All;

                                            library_filter_mode.set(filter_mode.clone());
                                            settings.write().library_filter_mode = filter_mode;
                                            save_settings_or_log(&settings);
                                        },

                                        "All"
                                    }

                                    button {
                                        class: if selected_filter_mode == LibraryFilterMode::Favourites { "topbar-segment active" } else { "topbar-segment" },

                                        onclick: move |_| {
                                            let filter_mode = LibraryFilterMode::Favourites;

                                            library_filter_mode.set(filter_mode.clone());
                                            settings.write().library_filter_mode = filter_mode;
                                            save_settings_or_log(&settings);
                                        },

                                        "Favourites"
                                    }
                                }
                            }

                            label { class: "topbar-select-field",
                                span { class: "topbar-select-label", "Tag" }

                                div { class: "select-shell",

                                    //filter by tag
                                    select {
                                        class: "tag-filter-select",

                                        value: selected_tag.clone().unwrap_or_default(),

                                        onchange: move |event| {
                                            let value = event.value();

                                            if value.is_empty() {
                                                selected_tag_filter.set(None);
                                            } else {
                                                selected_tag_filter.set(Some(value));
                                            }
                                        },

                                        option { value: "", "All Tags" }

                                        for tag in available_tags {
                                            option { value: "{tag}", "{tag}" }
                                        }
                                    }

                                    span { class: "select-chevron", "⌄" }
                                }
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
                    }

                    if selected_view == AppView::Library {
                        div { class: "app-layout",
                            section { class: "library-column",

                                //when pressing the + to add a vn
                                if *show_add_form.read() {
                                    div { class: "add-vn-overlay",

                                        AddVnForm {
                                            on_close: move |_| {
                                                show_add_form.set(false);
                                            },

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
                                                    is_favourite: false,
                                                    tags: new_vn.tags,
                                                    cover_path: new_vn.cover_path,
                                                    executable_path: None,
                                                    launch_mode: LaunchMode::default(),
                                                    wine_binary: None,
                                                    wine_prefix: None,
                                                    wine_locale: None,
                                                    launch_arguments: String::new(),
                                                    launch_environment: String::new(),
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

                                                show_add_form.set(false);
                                            },
                                        }
                                    }
                                }

                                LibraryView {
                                    vns: filtered_vns,
                                    selected_vn_id: selected_vn_id_value,

                                    //when selecting a vn
                                    on_select: move |id| {
                                        selected_vn_id.set(Some(id));

                                    },

                                    //when toggling vn as favourite
                                    on_toggle_favourite: move |id| {
                                        update_vn_and_save(
                                            &mut vns,
                                            id,
                                            move |vn| {
                                                vn.is_favourite = !vn.is_favourite;
                                            },
                                            "Could not save favourite".to_string(),
                                        );
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
                                            let trimmed_path = path.trim().to_string();

                                            if is_flatpak_document_portal_path(&trimmed_path) {
                                                notification
                                                    .set(
                                                        Some(AppNotification {
                                                            level: NotificationLevel::Error,
                                                            message: "The executable has been selected through Flatpak's temporary document portal. Move the game into a normal folder such as ~/Games, then type or select the real path."
                                                                .to_string(),
                                                        }),
                                                    );
                                                return;
                                            }
                                            let executable_path = if trimmed_path.is_empty() {
                                                None
                                            } else {
                                                Some(trimmed_path)
                                            };
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
                                                            if vn.launch_mode == LaunchMode::Proton
                                                                && !umu_launcher_is_available()
                                                            {
                                                                notification
                                                                    .set(
                                                                        Some(AppNotification {
                                                                            level: NotificationLevel::Error,
                                                                            message: "Could not launch VN: UMU Launcher was not found. Install umu-launcher to run games with Proton."
                                                                                .to_string(),
                                                                        }),
                                                                    );
                                                                return;
                                                            }
                                                            let launch_log_path = match new_launch_log_path(
                                                                vn.id,
                                                                vn.title.clone(),
                                                            ) {
                                                                Ok(path) => Some(path),
                                                                Err(error) => {
                                                                    notification
                                                                        .set(
                                                                            Some(AppNotification {
                                                                                level: NotificationLevel::Warning,
                                                                                message: format!("Could not create launch log: {error}"),
                                                                            }),
                                                                        );
                                                                    None
                                                                }
                                                            };
                                                            let launch_environment = match parse_launch_environment(
                                                                vn.launch_environment.clone(),
                                                            ) {
                                                                Ok(environment) => environment,
                                                                Err(error) => {
                                                                    notification
                                                                        .set(
                                                                            Some(AppNotification {
                                                                                level: NotificationLevel::Error,
                                                                                message: format!("Could not launch VN: {error}"),
                                                                            }),
                                                                        );
                                                                    return;
                                                                }
                                                            };

                                                            match launch_executable(
                                                                path,
                                                                vn.launch_mode,
                                                                vn.wine_binary,
                                                                vn.wine_prefix,
                                                                vn.wine_locale,
                                                                vn.proton_path,
                                                                vn.umu_game_id,
                                                                vn.launch_arguments,
                                                                launch_environment,
                                                                launch_log_path.clone(),
                                                            ) {
                                                                Ok(mut child) => {
                                                                    let started_time = Utc::now();
                                                                    let started_at = started_time.to_rfc3339();
                                                                    let started_timer = Instant::now();
                                                                    let vn_id = presence_vn.id;
                                                                    notification
                                                                        .set(
                                                                            Some(AppNotification {
                                                                                level: NotificationLevel::Info,
                                                                                message: format!("Launched {}.", presence_vn.title),

                                                                            }),
                                                                        );
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
                                                                        if let Some(log_path) = launch_log_path {
                                                                            if let Err(error) = update_latest_launch_log(&log_path) {
                                                                                println!("Could not update latest launch log: {error}");
                                                                            }
                                                                        }
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
                                                                    notification
                                                                        .set(
                                                                            Some(AppNotification {
                                                                                level: NotificationLevel::Error,
                                                                                message: format!("Could not launch VN: {error}"),
                                                                            }),
                                                                        );
                                                                }
                                                            }
                                                        }
                                                        None => {
                                                            notification
                                                                .set(
                                                                    Some(AppNotification {
                                                                        level: NotificationLevel::Warning,
                                                                        message: "No executable path saved for this VN.".to_string(),
                                                                    }),
                                                                );
                                                        }
                                                    }
                                                }
                                                None => {
                                                    notification
                                                        .set(
                                                            Some(AppNotification {
                                                                level: NotificationLevel::Error,
                                                                message: format!("Could not find VN with id {id}."),
                                                            }),
                                                        );
                                                }
                                            }
                                        },

                                        //when running an exe in the wine prefix
                                        on_run_tool: move |(id, tool_path): (u64, String)| {
                                            let vn = vns
                                                .read()
                                                .iter()
                                                .find(|vn| vn.id == id)
                                                .cloned();

                                            let Some(vn) = vn else {
                                                notification
                                                    .set(
                                                        Some(AppNotification {
                                                            level: NotificationLevel::Error,
                                                            message: "Could not run exe: VN was not found.".to_string(),
                                                        }),
                                                    );

                                                return;
                                            };
                                            if vn.launch_mode == LaunchMode::Proton && !umu_launcher_is_available() {
                                                notification
                                                    .set(
                                                        Some(AppNotification {
                                                            level: NotificationLevel::Error,
                                                            message: "Could not run tool: UMU Launcher was not found. Install umu-launcher to run tools with Proton."
                                                                .to_string(),
                                                        }),
                                                    );
                                                return;
                                            }
                                            let launch_environment = match parse_launch_environment(
                                                vn.launch_environment.clone(),
                                            ) {
                                                Ok(environment) => environment,
                                                Err(error) => {
                                                    notification
                                                        .set(
                                                            Some(AppNotification {
                                                                level: NotificationLevel::Error,
                                                                message: format!("Could not run tool: {error}"),
                                                            }),
                                                        );
                                                    return;
                                                }
                                            };
                                            let launch_log_path = match new_launch_log_path(
                                                vn.id,
                                                format!("{} tool", vn.title),
                                            ) {
                                                Ok(path) => Some(path),
                                                Err(error) => {
                                                    notification
                                                        .set(
                                                            Some(AppNotification {
                                                                level: NotificationLevel::Warning,
                                                                message: format!("Could not create tool log: {error}"),
                                                            }),
                                                        );
                                                    None
                                                }
                                            };
                                            match launch_executable(
                                                tool_path,
                                                vn.launch_mode,
                                                vn.wine_binary,
                                                vn.wine_prefix,
                                                vn.wine_locale,
                                                vn.proton_path,
                                                vn.umu_game_id,
                                                vn.launch_arguments,
                                                launch_environment,
                                                launch_log_path.clone(),
                                            ) {
                                                Ok(mut child) => {
                                                    notification
                                                        .set(
                                                            Some(AppNotification {
                                                                level: NotificationLevel::Info,
                                                                message: format!("Started tool for {}.", vn.title),
                                                            }),
                                                        );
                                                    thread::spawn(move || {
                                                        let wait_result = child.wait();
                                                        if let Some(log_path) = launch_log_path {
                                                            if let Err(error) = update_latest_launch_log(&log_path) {
                                                                println!("Could not update ltatest launch log: {error}");
                                                            }
                                                        }
                                                        if let Err(error) = wait_result {
                                                            println!("Tool process wait failed: {error}");
                                                        }
                                                    });
                                                }
                                                Err(error) => {
                                                    notification
                                                        .set(
                                                            Some(AppNotification {
                                                                level: NotificationLevel::Error,
                                                                message: format!("Could not run tool: {error}"),
                                                            }),
                                                        );

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
                                            let trimmed_prefix = prefix.trim().to_string();

                                            if is_flatpak_document_portal_path(&trimmed_prefix) {
                                                notification
                                                    .set(
                                                        Some(AppNotification {
                                                            level: NotificationLevel::Error,
                                                            message: "The Wine prefix has been selected through Flatpak's temporary document portal. Grant Kakera access to the folder, then reselect the real path."
                                                                .to_string(),
                                                        }),
                                                    );
                                                return;
                                            }
                                            let wine_prefix = if trimmed_prefix.is_empty() {
                                                None
                                            } else {
                                                Some(trimmed_prefix)
                                            };
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

                                        //when editing env vars
                                        on_launch_environment_change: move |(id, environment): (u64, String)| {
                                            update_vn_and_save(
                                                &mut vns,
                                                id,
                                                move |vn| {
                                                    vn.launch_environment = environment;
                                                },
                                                "Could not save env vars".to_string(),
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

                                        //when changing tags
                                        on_tags_change: move |(id, tags): (u64, Vec<String>)| {
                                            update_vn_and_save(
                                                &mut vns,
                                                id,
                                                move |vn| {
                                                    vn.tags = tags;
                                                },
                                                "Could not save tags".to_string(),
                                            );
                                        },

                                        //when changing cover
                                        on_cover_path_change: move |(id, cover_path): (u64, String)| {
                                            let trimmed_cover_path = cover_path.trim().to_string();

                                            if is_flatpak_document_portal_path(&trimmed_cover_path) {
                                                notification
                                                    .set(
                                                        Some(AppNotification {
                                                            level: NotificationLevel::Error,
                                                            message: "The cover image has been selected through Flatpak's temporary document portal. Grant Kakera access to the folder, then reselect the real path."
                                                                .to_string(),
                                                        }),
                                                    );
                                                return;
                                            }
                                            update_vn_and_save(
                                                &mut vns,
                                                id,
                                                move |vn| {
                                                    vn.cover_path = Some(trimmed_cover_path);
                                                },
                                                "Could not save cover image".to_string(),
                                            );
                                        },

                                        //when refreshing cover
                                        on_cover_refresh: move |id| {
                                            let vn = vns
                                            .read()
                                            .iter()
                                            .find(|vn| vn.id == id)
                                            .cloned();

                                            let Some(vn) = vn else {
                                                notification
                                                    .set(
                                                        Some(AppNotification {
                                                            level: NotificationLevel::Error,
                                                            message: "Could not refresh cover: VN was not found.".to_string(),
                                                        }),
                                                    );
                                                return;
                                            };
                                            let Some(cover_url) = vn.cover_url.clone() else {
                                                notification
                                                    .set(
                                                        Some(AppNotification {
                                                            level: NotificationLevel::Warning,
                                                            message: "This VN does not have a VNDB cover URL saved."
                                                                .to_string(),
                                                        }),
                                                    );
                                                return;
                                            };
                                            let mut vns_for_cover = vns;
                                            let mut notification_for_cover = notification;
                                            spawn(async move {
                                                match cache_cover_image(id, cover_url).await {
                                                    Ok(cover_path) => {
                                                        for vn in vns_for_cover.write().iter_mut() {
                                                            if vn.id == id {
                                                                vn.cover_path = Some(cover_path.clone());
                                                                break;
                                                            }
                                                        }
                                                        save_vns_or_log(
                                                            &vns_for_cover,
                                                            "Could not save refreshed cover path.".to_string(),
                                                        );
                                                        notification_for_cover
                                                            .set(
                                                                Some(AppNotification {
                                                                    level: NotificationLevel::Info,
                                                                    message: "Cover refreshed from VNDB".to_string(),
                                                                }),
                                                            );
                                                    }
                                                    Err(error) => {
                                                        notification_for_cover
                                                            .set(
                                                                Some(AppNotification {
                                                                    level: NotificationLevel::Error,
                                                                    message: format!("Could not refresh cover: {error}"),
                                                                }),
                                                            );
                                                    }
                                                }
                                            });
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

                            on_open_logs_folder: move |_| {
                                match launch_logs_dir() {
                                    Ok(path) => {
                                        if let Err(error) = open_folder(&path) {
                                            println!("Could not open logs folder: {error}");
                                            notification
                                                .set(
                                                    Some(AppNotification {
                                                        level: NotificationLevel::Error,
                                                        message: format!("Could not open logs folder: {error}"),
                                                    }),
                                                );
                                        }
                                    }
                                    Err(error) => {
                                        println!("Could not find logs folder: {error}");
                                        notification
                                            .set(
                                                Some(AppNotification {
                                                    level: NotificationLevel::Error,
                                                    message: format!("Could not find logs folder: {error}"),
                                                }),
                                            );
                                    }
                                }
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

fn latest_played_at(vn: &VisualNovel) -> String {
    vn.play_sessions
        .iter()
        .map(|session| session.started_at.clone())
        .max()
        .unwrap_or_default()
}

fn total_playtime_seconds(vn: &VisualNovel) -> u64 {
    vn.play_sessions
        .iter()
        .map(|session| session.duration_seconds)
        .sum()
}
