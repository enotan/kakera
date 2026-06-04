use dioxus::prelude::*;

#[component]
pub fn SettingsView(
    discord_rich_presence_enabled: bool,
    on_discord_rich_presence_change: EventHandler<bool>,
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
        }
    }
}