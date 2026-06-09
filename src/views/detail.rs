use crate::models::{LaunchMode, StoryRoute, VisualNovel};
use crate::views::library::cover_source;
use crate::vn_markup::{DescriptionPart, parse_description};
use crate::wine::{ProtonRunner, WineRunner, SteamPrefix};

use dioxus::prelude::*;
use rfd::FileDialog;

///displays details for one selected vn
#[component]
pub fn DetailView(
    vn: VisualNovel,
    wine_runners: Vec<WineRunner>,
    proton_runners: Vec<ProtonRunner>,
    steam_prefixes: Vec<SteamPrefix>,
    on_notes_change: EventHandler<(u64, String)>,
    on_route_add: EventHandler<(u64, String)>,
    on_route_toggle: EventHandler<(u64, String)>,
    on_executable_path_change: EventHandler<(u64, String)>,
    on_launch: EventHandler<u64>,
    on_launch_mode_change: EventHandler<(u64, LaunchMode)>,
    on_wine_binary_change: EventHandler<(u64, String)>,
    on_proton_path_change: EventHandler<(u64, String)>,
    on_umu_game_id_change: EventHandler<(u64, String)>,
    on_delete: EventHandler<u64>,
    on_wine_prefix_change: EventHandler<(u64, String)>,
    on_wine_locale_change: EventHandler<(u64, String)>,
    on_launch_arguments_change: EventHandler<(u64, String)>,
    on_description_change: EventHandler<(u64, String)>,
    on_cover_path_change: EventHandler<(u64, String)>,
    on_active_route_change: EventHandler<(u64, Option<String>)>,
    on_route_delete: EventHandler<(u64, String)>,
) -> Element {
    let mut new_route_name = use_signal(String::new);
    let typed_route_name = new_route_name.read().clone();

    let executable_path_text = match vn.executable_path.clone() {
        Some(path) => path,
        None => String::new(),
    };

    let mut executable_path_draft = use_signal(|| executable_path_text.clone());
    let wine_prefix_text = vn.wine_prefix.clone().unwrap_or_default();
    let wine_locale_text = vn.wine_locale.clone().unwrap_or_default();

    let show_compatibility_settings =
        cfg!(target_os = "linux") && vn.launch_mode != LaunchMode::Native;

    let mut wine_prefix_draft = use_signal(|| wine_prefix_text.clone());
    let mut wine_locale_draft = use_signal(|| wine_locale_text.clone());
    let mut launch_arguments_draft = use_signal(|| vn.launch_arguments.clone());

    let wine_binary_text = vn.wine_binary.clone().unwrap_or_default();
    let mut wine_binary_draft = use_signal(|| wine_binary_text.clone());
    let wine_binary_value = wine_binary_draft.read().clone();

    let proton_path_text = vn.proton_path.clone().unwrap_or_default();
    let mut proton_path_draft = use_signal(|| proton_path_text.clone());
    let proton_path_value = proton_path_draft.read().clone();

    let mut umu_game_id_draft = use_signal(|| vn.umu_game_id.clone());
    let umu_game_id_value = umu_game_id_draft.read().clone();

    let executable_path_value = executable_path_draft.read().clone();
    let wine_prefix_value = wine_prefix_draft.read().clone();
    let wine_locale_value = wine_locale_draft.read().clone();
    let launch_arguments_value = launch_arguments_draft.read().clone();

    let mut notes_draft = use_signal(|| vn.notes.clone());
    let notes_value = notes_draft.read().clone();
    let saved_notes = vn.notes.clone();
    let mut notes_is_editing = use_signal(|| false);

    let mut description_draft = use_signal(|| vn.description.clone().unwrap_or_default());
    let mut description_is_editing = use_signal(|| false);
    let saved_description = vn.description.clone().unwrap_or_default();
    let description_value = description_draft.read().clone();
    let description_parts = parse_description(saved_description.clone());

    let total_playtime_seconds: u64 = vn
        .play_sessions
        .iter()
        .map(|session| session.duration_seconds)
        .sum();

    let total_playtime_text = format_playtime(total_playtime_seconds);

    rsx! {
        section { class: "detail-panel",

            h2 { "{vn.title}" }

            //img cover
            if let Some(cover_src) = cover_source(vn.clone()) {
                img {
                    class: "detail-cover",
                    src: "{cover_src}",
                    alt: "Cover art for {vn.title}",
                }
            }

            button {
                class: "fp-button",

                onclick: move |_| {
                    let picked_file = FileDialog::new()
                        .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
                        .pick_file();

                    if let Some(path) = picked_file {
                        on_cover_path_change.call((vn.id, path.to_string_lossy().to_string()));
                    }
                },

                "Change cover image"
            }

            //desc
            if let Some(_description) = vn.description.clone() {
                h3 { "Description" }

                div { class: if *description_is_editing.read() { "desc-box editing" } else { "desc-box" },

                    button {
                        class: "desc-edit-button",
                        title: "Edit Description",

                        onclick: move |_| {
                            let next_value = !*description_is_editing.read();

                            if next_value {
                                description_draft.set(vn.description.clone().unwrap_or_default());
                            }

                            description_is_editing.set(next_value);
                        },

                        "✎"
                    }

                    if *description_is_editing.read() {
                        textarea {
                            class: "detail-desc-input",
                            value: "{description_value}",

                            oninput: move |event| {
                                description_draft.set(event.value());
                            },

                            onblur: move |_| {
                                on_description_change.call((vn.id, description_draft.read().clone()));
                                description_is_editing.set(false);
                            },
                        }
                    } else if saved_description.is_empty() {
                        div { class: "detail-desc empty", "No description yet." }
                    } else {
                        div { class: "detail-desc",

                            for part in description_parts {
                                DescriptionPartView { part }
                            }
                        }
                    }
                }
            }

            h3 { "Launch" }

            //exec path input
            label {
                "Executable path"

                input {
                    value: "{executable_path_value}",

                    oninput: move |event| {
                        executable_path_draft.set(event.value());
                    },

                    onblur: move |_| {
                        on_executable_path_change.call((vn.id, executable_path_draft.read().clone()));
                    },
                }
            }

            //file picker
            button {
                class: "fp-button",

                onclick: move |_| {
                    let picked_file = FileDialog::new()
                        .add_filter("Executables", &["exe", "bin", "sh", "AppImage"])
                        .add_filter("All Files", &["*"])
                        .pick_file();

                    if let Some(path) = picked_file {
                        let path_text = path.to_string_lossy().to_string();
                        executable_path_draft.set(path_text.clone());
                        on_executable_path_change.call((vn.id, path_text));
                    }
                },

                "Choose executable"
            }

            //launch mode selector
            label { class: "launch-selector",
                "Launch mode"

                select {
                    value: match vn.launch_mode {
                        LaunchMode::Native => "native",
                        LaunchMode::Wine => "wine",
                        LaunchMode::Proton => "proton",
                    },

                    onchange: move |event| {
                        let launch_mode = match event.value().as_str() {
                            "wine" => LaunchMode::Wine,
                            "proton" => LaunchMode::Proton,
                            _ => LaunchMode::Native,
                        };
                        on_launch_mode_change.call((vn.id, launch_mode))
                    },

                    option { value: "native", "Native" }

                    option { value: "wine", "Wine" }

                    option { value: "proton", "Proton" }
                }
            }

            //wine / proton settings area
            if show_compatibility_settings {
                div { class: "wine-settings",

                    h3 { "Wine Settings" }

                    if vn.launch_mode == LaunchMode::Wine {
                        label {
                            "Wine binary / runner"

                            select {
                                onchange: move |event| {
                                    let binary_path = event.value();

                                    wine_binary_draft.set(binary_path.clone());
                                    on_wine_binary_change.call((vn.id, binary_path));
                                },

                                option {
                                    value: "",
                                    selected: wine_binary_value.is_empty(),
                                    "Default Wine from PATH"
                                }

                                for runner in wine_runners.clone() {
                                    option {
                                        value: "{runner.binary_path}",
                                        selected: runner.binary_path == wine_binary_value,
                                        "{runner.name}"
                                    }
                                }

                                if !wine_binary_value.is_empty()
                                    && !wine_runners.iter().any(|runner| runner.binary_path == wine_binary_value)
                                {
                                    option {
                                        value: "{wine_binary_value}",
                                        selected: true,
                                        "Custom runner"
                                    }
                                }
                            }

                            button {
                                class: "fp-button",

                                onclick: move |_| {
                                    let picked_file = FileDialog::new().pick_file();

                                    if let Some(path) = picked_file {
                                        let path_text = path.to_string_lossy().to_string();

                                        wine_binary_draft.set(path_text.clone());
                                        on_wine_binary_change.call((vn.id, path_text));
                                    }
                                },

                                "Choose Wine binary"
                            }
                        }
                    }

                    if vn.launch_mode == LaunchMode::Proton {
                        label {
                            "Proton version"

                            select {
                                value: "{proton_path_value}",

                                onchange: move |event| {
                                    let proton_path = event.value();

                                    proton_path_draft.set(proton_path.clone());
                                    on_proton_path_change.call((vn.id, proton_path));
                                },

                                option {
                                    value: "",
                                    selected: proton_path_value.is_empty(),
                                    "UMU managed default"
                                }

                                for runner in proton_runners.clone() {
                                    option {
                                        value: "{runner.path}",
                                        selected: runner.path == proton_path_value,
                                        "{runner.name}"
                                    }
                                }

                                if !proton_path_value.is_empty()
                                    && !proton_runners.iter().any(|runner| runner.path == proton_path_value)
                                {
                                    option {
                                        value: "{proton_path_value}",
                                        selected: true,
                                        "Custom Proton install"
                                    }
                                }
                            }
                        }

                        button {
                            class: "fp-button",

                            onclick: move |_| {
                                let picked_folder = FileDialog::new().pick_folder();

                                if let Some(path) = picked_folder {
                                    let path_text = path.to_string_lossy().to_string();

                                    proton_path_draft.set(path_text.clone());
                                    on_proton_path_change.call((vn.id, path_text));
                                }
                            },

                            "Choose Proton folder"
                        }

                        label {
                            "UMU game ID"

                            input {
                                placeholder: "umu-default",
                                value: "{umu_game_id_value}",

                                oninput: move |event| {
                                    umu_game_id_draft.set(event.value());
                                },

                                onblur: move |_| {
                                    on_umu_game_id_change.call((vn.id, umu_game_id_draft.read().clone()));
                                },
                            }
                        }
                    }

                    label {
                        "Detected Steam prefix"

                        select {
                            value: "{wine_prefix_value}",

                            onchange: move |event| {
                                let prefix_path = event.value();

                                wine_prefix_draft.set(prefix_path.clone());
                                on_wine_prefix_change.call((vn.id, prefix_path));
                            },

                            option { value: "", "No Steam prefix selected" }

                            for prefix in steam_prefixes.clone() {
                                option { value: "{prefix.path}",
                                    match prefix.game_name {
                                        Some(name) => format!("{name} - Steam {}", prefix.app_id),
                                        None => format!("Steam {}", prefix.app_id),
                                    }
                                }
                            }

                            if !wine_prefix_value.is_empty()
                                && !steam_prefixes.iter().any(|prefix| prefix.path == wine_prefix_value)
                            {
                                option { value: "{wine_prefix_value}", "Custom prefix" }
                            }
                        }
                    }

                    label {
                        "Wine prefix"

                        input {
                            value: "{wine_prefix_value}",

                            oninput: move |event| {
                                wine_prefix_draft.set(event.value());
                            },

                            onblur: move |_| {
                                on_wine_prefix_change.call((vn.id, wine_prefix_draft.read().clone()));
                            },
                        }
                    }

                    button {
                        class: "fp-button",

                        onclick: move |_| {
                            let picked_folder = FileDialog::new().pick_folder();

                            if let Some(path) = picked_folder {
                                let path_text = path.to_string_lossy().to_string();

                                wine_prefix_draft.set(path_text.clone());
                                on_wine_prefix_change.call((vn.id, path.to_string_lossy().to_string()));
                            }
                        },

                        "Choose prefix folder"
                    }

                    label {
                        "Wine locale"

                        input {
                            placeholder: "ja_JP.UTF-8",
                            value: "{wine_locale_value}",

                            oninput: move |event| {
                                wine_locale_draft.set(event.value());
                            },

                            onblur: move |_| {
                                on_wine_locale_change.call((vn.id, wine_locale_draft.read().clone()));
                            },
                        }
                    }

                    label {
                        "Launch arguments"

                        input {
                            placeholder: "--some-flag",
                            value: "{launch_arguments_value}",

                            oninput: move |event| {
                                launch_arguments_draft.set(event.value());
                            },

                            onblur: move |_| {
                                on_launch_arguments_change.call((vn.id, launch_arguments_draft.read().clone()));
                            },
                        }
                    }
                }
            }

            //launch button
            button {
                class: "launch-button",

                disabled: vn.executable_path.is_none(),

                onclick: move |_| {
                    on_launch.call(vn.id);
                },

                "Launch"
            }

            //play sessions
            h3 { "Playtime" }

            div { class: "playtime-summary",
                div {
                    span { class: "stat-label", "Total playtime" }
                    strong { "{total_playtime_text}" }
                }

                div {
                    span { class: "stat-label", "Sessions recorded" }
                    strong { "{vn.play_sessions.len()}" }
                }
            }

            h3 { "Play sessions" }

            ul {
                for session in vn.play_sessions.clone() {
                    li {

                        "{format_started_at(session.started_at.clone())} - {session.duration_seconds} seconds"
                    }
                }
            }

            //notes
            h3 { "Notes" }

            div { class: if *notes_is_editing.read() { "notes-box editing" } else { "notes-box" },

                button {
                    class: "notes-edit-button",
                    title: "Edit notes",

                    onclick: move |_| {
                        let next_value = !*notes_is_editing.read();

                        if next_value {
                            notes_draft.set(vn.notes.clone());
                        }

                        notes_is_editing.set(next_value);
                    },

                    "✎"
                }

                if *notes_is_editing.read() {
                    textarea {
                        class: "notes-input",
                        value: "{notes_value}",

                        oninput: move |event| {
                            notes_draft.set(event.value());
                        },

                        onblur: move |_| {
                            on_notes_change.call((vn.id, notes_draft.read().clone()));
                            notes_is_editing.set(false);
                        },
                    }
                } else if saved_notes.is_empty() {
                    p { class: "notes-text empty", "No notes yet." }
                } else {
                    p { class: "notes-text", "{saved_notes}" }
                }
            }

            h3 { "Routes" }
            p { "Routes tracked: {vn.routes.len()}" }

            label {
                "New route"

                input {
                    value: "{typed_route_name}",

                    oninput: move |event| {
                        new_route_name.set(event.value());
                    },
                }
            }

            button {
                onclick: move |_| {
                    let route_name = new_route_name.read().clone();

                    if !route_name.is_empty() {
                        on_route_add.call((vn.id, route_name));
                        new_route_name.set(String::new());
                    }
                },

                "Add route"
            }

            label {
                "Active route"

                select {
                    value: vn.active_route.clone().unwrap_or_default(),

                    onchange: move |event| {
                        let route_name = event.value();
                        let active_route = if route_name.is_empty() { None } else { Some(route_name) };
                        on_active_route_change.call((vn.id, active_route));
                    },

                    option { value: "", "No active route" }

                    for route in vn.routes.clone() {
                        option { value: "{route.name}", "{route.name}" }
                    }
                }
            }

            div { class: "route-list",

                for route in vn.routes.clone() {
                    RouteItem {
                        vn_id: vn.id,
                        route,
                        on_route_toggle,
                        on_route_delete,
                    }
                }
            }

            button {
                class: "delete-button",

                onclick: move |_| {
                    on_delete.call(vn.id);
                },

                "Delete VN"
            }
        }
    }
}

fn format_started_at(started_at: String) -> String {
    let parsed_time = match chrono::DateTime::parse_from_rfc3339(&started_at) {
        Ok(time) => time,
        Err(_error) => return started_at,
    };

    parsed_time.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn format_playtime(total_seconds: u64) -> String {
    let hours = total_seconds as f64 / 3600.0;

    if hours < 1.0 {
        let minutes = total_seconds / 60;
        format!("{minutes} min")
    } else {
        format!("{hours:.1} hrs")
    }
}

///need a component because dioxus hates match or something
#[component]
fn DescriptionPartView(part: DescriptionPart) -> Element {
    match part {
        DescriptionPart::Text(text) => rsx! {
            span { "{text}" }
        },
        DescriptionPart::Bold(text) => rsx! {
            strong { "{text}" }
        },
        DescriptionPart::Italic(text) => rsx! {
            em { "{text}" }
        },
        DescriptionPart::Link { label, url } => rsx! {
            a { href: "{url}", target: "_blank", "{label}" }
        },
        DescriptionPart::Spoiler(parts) => rsx! {
            SpoilerText { parts }
        },
    }
}

#[component]
fn SpoilerText(parts: Vec<DescriptionPart>) -> Element {
    let mut is_revealed = use_signal(|| false);

    if *is_revealed.read() {
        rsx! {
            span { class: "spoiler-text revealed",
                for part in parts {
                    DescriptionPartView { part }
                }
            }
        }
    } else {
        rsx! {
            button {
                class: "spoiler-text hidden",
                onclick: move |_| {
                    is_revealed.set(true);
                },
                "Spoiler"
            }
        }
    }
}

#[component]
fn RouteItem(
    vn_id: u64,
    route: StoryRoute,
    on_route_toggle: EventHandler<(u64, String)>,
    on_route_delete: EventHandler<(u64, String)>,
) -> Element {
    let route_name_for_toggle = route.name.clone();
    let route_name_for_delete = route.name.clone();

    rsx! {
        label { class: "route-item",
            input {
                class: "route-checkbox",
                r#type: "checkbox",
                checked: route.completed,

                onchange: move |_| {
                    on_route_toggle.call((vn_id, route_name_for_toggle.clone()));
                },
            }

            span { class: "route-name", "{route.name}" }

            button {
                class: "route-delete-button",
                title: "Delete route",

                onclick: move |_| {
                    on_route_delete.call((vn_id, route_name_for_delete.clone()));
                },

                "🗑"
            }
        }
    }
}
