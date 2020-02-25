use cursive::event::{Event, Key};
use cursive::traits::*;
use cursive::views::{Dialog, EditView, NamedView, OnEventView, TextArea, TextView};
use cursive::Cursive;
use editor::views::{CodeArea, DefaultHighlighter, Highlighter};
use std::fs::read_to_string;

fn main() {
    let mut siv = Cursive::default();
    // The main dialog will just have a textarea.
    // Its size expand automatically with the content.

    siv.load_theme_file("assets/style.toml").unwrap();

    // siv.add_layer(
    //     Dialog::new()
    //         .title("main.rs")
    //         .content(CodeArea::<DefaultHighlighter>::default().with_name("text")),
    // );

    // Create a dialog with an edit text and a button.
    // The user can either hit the <Ok> button,
    // or press Enter on the edit text.
    siv.add_layer(
        Dialog::new()
            .title("Open file")
            // Padding is (left, right, top, bottom)
            .content(
                EditView::new()
                    // Call `show_popup` when the user presses `Enter`
                    .on_submit(show_popup)
                    // Give the `EditView` a name so we can refer to it later.
                    .with_name("name")
                    // Wrap this in a `ResizedView` with a fixed width.
                    // Do this _after_ `with_name` or the name will point to the
                    // `ResizedView` instead of `EditView`!
                    .fixed_width(20),
            )
            .button("Ok", |s| {
                // This will run the given closure, *ONLY* if a view with the
                // correct type and the given name is found.
                let name = s
                    .call_on_name("name", |view: &mut EditView| {
                        // We can return content from the closure!
                        view.get_content()
                    })
                    .unwrap();

                // Run the next step
                show_popup(s, &name);
            }),
    );

    siv.run();
}

// This will replace the current layer with a new popup.
// If the name is empty, we'll show an error message instead.
fn show_popup(s: &mut Cursive, name: &str) {
    if name.is_empty() {
        // Try again as many times as we need!
        s.add_layer(Dialog::info("Enter a path"));
    } else {
        // Remove the initial popup
        s.pop_layer();
        // And put a new one instead
        s.add_layer(open::<DefaultHighlighter>(name));
    }
}

fn open<H>(file: &str) -> Dialog
where
    H: Highlighter,
{
    let contents = read_to_string(file);
    Dialog::new()
        .title(file)
        .content(CodeArea::<H>::default().open_file(file))
}
