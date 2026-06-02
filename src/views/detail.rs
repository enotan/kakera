use crate::models::{LaunchMode, VisualNovel};
use dioxus::prelude::*;
use rfd::FileDialog;

///displays details for one selected vn
#[component]
pub fn DetailView(
    visual_novel: VisualNovel,
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

    let executable_path_text = match visual_novel.executable_path.clone() {
        Some(path) => path,
        None => String::new(),
    };

    let wine_prefix_text = visual_novel.wine_prefix.clone().unwrap_or_default();
    let wine_locale_text = visual_novel.wine_locale.clone().unwrap_or_default();

    let show_wine_settings = cfg!(target_os = "linux") && visual_novel.launch_mode == LaunchMode::Wine;

    rsx! {
        section { class: "detail-panel",

            h2 { "{visual_novel.title}" }

            if let Some(cover_url) = visual_novel.cover_url.clone() {
                img {
                    class: "detail-cover",
                    src: "{cover_url}",
                    alt: "Cover art for {visual_novel.title}",
                }
            }

            if let Some(description) = visual_novel.description.clone() {
                p {
                    class: "detail-description", 
                    "{description}"
                }
            }

            h3 { "Launch" }

            //exec path input
            label {
                "Executable path"

                input {
                    value: "{executable_path_text}",

                    oninput: move |event| {
                        on_executable_path_change.call((visual_novel.id, event.value()));
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
                            .call((visual_novel.id, path.to_string_lossy().to_string()));
                    }
                },

                "Choose executable"
            }

            //launch mode selector
            label {
                class: "launch-selector",
                "Launch mode"

                select {
                    value: match visual_novel.launch_mode {
                        LaunchMode::Native => "native",
                        LaunchMode::Wine => "wine",
                    },

                    onchange: move |event| {
                        let launch_mode = match event.value().as_str() {
                            "wine" => LaunchMode::Wine,
                            _ => LaunchMode::Native,
                        };
                        on_launch_mode_change.call((visual_novel.id, launch_mode))
                    },

                    option { value: "native", "Native" }

                    option { value: "wine", "Wine" }
                }
            }

            if show_wine_settings {
                div {
                    class: "wine-settings",

                    h3 { "Wine Settings" }

                    label {
                        "Wine prefix"

                        input {
                            value: "{wine_prefix_text}",

                            oninput: move |event| {
                                on_wine_prefix_change.call((
                                    visual_novel.id,
                                    event.value(),
                                ));
                            },
                        }
                    } 

                    button {
                        class: "fp-button",

                        onclick: move |_| {
                            let picked_folder = FileDialog::new().pick_folder();

                            if let Some(path) = picked_folder {
                                on_wine_prefix_change.call((
                                    visual_novel.id,
                                    path.to_string_lossy().to_string(),
                                ));
                            }
                        },

                        "Choose prefix folder"
                    }

                    label {
                        "Wine locale"

                        input {
                            placeholder: "ja_JP.UTF-8",
                            value: "{wine_locale_text}",

                            oninput: move |event| {
                                on_wine_locale_change.call((
                                    visual_novel.id,
                                    event.value(),
                                ));
                            },
                        }
                    }

                    label {
                        "Launch arguments"

                        input {
                            placeholder: "--some-flag",
                            value: "{visual_novel.launch_arguments}",

                            oninput: move |event| {
                                on_launch_arguments_change.call((
                                    visual_novel.id,
                                    event.value(),
                                ));
                            },
                        }
                    }
                }
            }

            button {
                class: "launch-button",

                disabled: visual_novel.executable_path.is_none(),

                onclick: move |_| {
                    on_launch.call(visual_novel.id);
                },

                "Launch"
            }

            h3 { "Play sessions" }
            p { "Sessions recorded: {visual_novel.play_sessions.len()}" }

            ul {
                for session in visual_novel.play_sessions.clone() {
                    li {

                        "{format_started_at(session.started_at.clone())} - {session.duration_seconds} seconds"
                    }
                }
            }

            h3 { "Notes" }

            textarea {
                value: "{visual_novel.notes}",

                oninput: move |event| {
                    on_notes_change.call((visual_novel.id, event.value()));
                },
            }

            h3 { "Routes" }
            p { "Routes tracked: {visual_novel.routes.len()}" }

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
                        on_route_add.call((visual_novel.id, route_name));
                        new_route_name.set(String::new());
                    }
                },

                "Add route"
            }

            div {
                class: "route-list",

                for route in visual_novel.routes.clone() {
                    label {
                        class: "route-item",

                        input {
                            class: "route-checkbox",
                            r#type: "checkbox",
                            checked: route.completed,

                            onchange: move |_| {
                                on_route_toggle.call((visual_novel.id, route.name.clone()));
                            },
                        }
                        
                        span {
                            class: "route-name",
                            "{route.name}"
                        }
                    }
                }
            }

            button {
                class: "delete-button",

                onclick: move |_| {
                    on_delete.call(visual_novel.id);
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
