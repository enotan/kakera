use dioxus::prelude::*;

#[component]
pub fn SettingsView(
    discord_rich_presence_enabled: bool,
    data_dir_text: String,
    on_discord_rich_presence_change: EventHandler<bool>,
    on_open_data_folder: EventHandler<()>
) -> Element {
    rsx! {
        section { class: "settings-panel",
            h2 { "Settings" }

            div { class: "settings-section",
                h3 { "Discord Rich Presence" }

                label { class: "setting-row",
                    span { "Enable Rich Presence" }

                    input {
                        class: "setting-checkbox",
                        r#type: "checkbox",
                        checked: discord_rich_presence_enabled,

                        onchange: move |event| {
                            on_discord_rich_presence_change.call(event.checked());
                        },
                    } 
                }
                
                p { class: "setting-help",
                    "Show the VN being you're playing on your Discord profile."
                }
            }  

            div { class: "settings-section",
                h3 { "Data" }

                div { class: "setting-row",
                    span { "Data folder" }
                    code { class: "setting-path", "{data_dir_text}" }
                }

                button {
                    class: "fp-button",
                    onclick: move |_| {
                        on_open_data_folder.call(());
                    },

                    "Open data folder"
                }
            }
        }
    }
}