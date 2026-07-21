mod app;
mod library;
mod settings;
mod theme;

fn main() -> iced::Result {
    iced::application(app::new, app::update, app::view)
        .title("618 Player")
        .theme(app::theme)
        .subscription(app::subscription)
        .window(settings::window())
        .run()
}
