use dioxus::prelude::*;

///the form to add a visual novel
#[component]
pub fn AddVnForm(on_add: EventHandler<String>) -> Element {
    let mut title = use_signal(String::new);

    rsx! {
        section {
            h2 { "Add VN" }

            label {
                "Title"

                input {
                    value: "{title}",

                    oninput: move |event| {
                        title.set(event.value());
                    }
                }
            }

            button {
                onclick: move |_| {
                    let typed_title = title.read().clone();

                    if !typed_title.is_empty() {
                        on_add.call(typed_title);
                        title.set(String::new());
                    }
                },

                "Add"
            }

            p { "Typed title: {title}" }
        }
    }
}

