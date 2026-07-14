mod app;
mod library;

fn main() -> iced::Result {
    iced::application(app::new, app::update, app::view)
        .title("Rust Music PoC")
        .theme(app::theme)
        .window_size((960.0, 640.0))
        .centered()
        .run()
}
