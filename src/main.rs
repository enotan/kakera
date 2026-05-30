use dioxus::prelude::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        main {
            h1 { "Hello, Kakera" }
            p { "Your visual novel library starts here." }
        }
    }
}