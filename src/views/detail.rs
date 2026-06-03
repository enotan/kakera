use crate::models::{LaunchMode, VisualNovel};
use crate::views::library::cover_source;
use dioxus::prelude::*;
use rfd::FileDialog;

///displays details for one selected vn
#[component]
pub fn DetailView(
    vn: VisualNovel,
    on_notes_change: EventHandler<(u64, String)>,
    on_route_add: EventHandler<(u64, String)>,
    on_route_toggle: EventHandler<(u64, String)>,
    on_executable_path_change: EventHandler<(u64, String)>,
    on_launch: EventHandler<u64>,
    on_launch_mode_change: EventHandler<(u64, LaunchMode)>,
    on_delete: EventHandler<u64>,
    on_wine_prefix_change: EventHandler<(u64, String)>,
    on_wine_locale_change: EventHandler<(u64, String)>,
    on_launch_arguments_change: EventHandler<(u64, String)>,
) -> Element {
    let mut new_route_name = use_signal(String::new);
    let typed_route_name = new_route_name.read().clone();

    let executable_path_text = match vn.executable_path.clone() {
        Some(path) => path,
        None => String::new(),
    };

    let wine_prefix_text = vn.wine_prefix.clone().unwrap_or_default();
    let wine_locale_text = vn.wine_locale.clone().unwrap_or_default();

    let show_wine_settings = cfg!(target_os = "linux") && vn.launch_mode == LaunchMode::Wine;

    let mut wine_prefix_draft = use_signal(|| wine_prefix_text.clone());
    let mut wine_locale_draft = use_signal(|| wine_locale_text.clone());
    let mut launch_arguments_draft = use_signal(|| vn.launch_arguments.clone());

    let wine_prefix_value = wine_prefix_draft.read().clone();
    let wine_locale_value = wine_locale_draft.read().clone();
    let launch_arguments_value = launch_arguments_draft.read().clone();

    let mut notes_draft = use_signal(|| vn.notes.clone());
    let notes_value = notes_draft.read().clone();

    rsx! {
        section { class: "detail-panel",

            h2 { "{vn.title}" }

            if let Some(cover_src) = cover_source(vn.clone()) {
                img {
                    class: "detail-cover",
                    src: "{cover_src}",
                    alt: "Cover art for {vn.title}",
                }
            }

            if let Some(description) = vn.description.clone() {
                p { class: "detail-description", "{description}" }
            }

            h3 { "Launch" }

            //exec path input
            label {
                "Executable path"

                input {
                    value: "{executable_path_text}",

                    oninput: move |event| {
                        on_executable_path_change.call((vn.id, event.value()));
                    },
                }
            }

            //file picker
            button {
                class: "fp-button",

                onclick: move |_| {
                    let picked_file = FileDialog::new()
                        .add_filter("Executables", &["exe", "bin", "sh", "AppImage"])
                        .pick_file();

                    if let Some(path) = picked_file {
                        on_executable_path_change
                            .call((vn.id, path.to_string_lossy().to_string()));
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
                    },

                    onchange: move |event| {
                        let launch_mode = match event.value().as_str() {
                            "wine" => LaunchMode::Wine,
                            _ => LaunchMode::Native,
                        };
                        on_launch_mode_change.call((vn.id, launch_mode))
                    },

                    option { value: "native", "Native" }

                    option { value: "wine", "Wine" }
                }
            }

            //wine settings area
            if show_wine_settings {
                div { class: "wine-settings",

                    h3 { "Wine Settings" }

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
            h3 { "Play sessions" }
            p { "Sessions recorded: {vn.play_sessions.len()}" }

            ul {
                for session in vn.play_sessions.clone() {
                    li {

                        "{format_started_at(session.started_at.clone())} - {session.duration_seconds} seconds"
                    }
                }
            }

            //notes
            h3 { "Notes" }

            textarea {
                value: "{notes_value}",

                oninput: move |event| {
                    notes_draft.set(event.value());
                },

                onblur: move |_| {
                    on_notes_change.call((vn.id, notes_draft.read().clone()));
                },
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

            div { class: "route-list",

                for route in vn.routes.clone() {
                    label { class: "route-item",

                        input {
                            class: "route-checkbox",
                            r#type: "checkbox",
                            checked: route.completed,

                            onchange: move |_| {
                                on_route_toggle.call((vn.id, route.name.clone()));
                            },
                        }

                        span { class: "route-name", "{route.name}" }
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
