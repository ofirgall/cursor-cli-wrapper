use cursor_cli_wrapper::config;

fn main() {
    let cfg = config::Config::load();
    let title = config::resolve_placeholders(&cfg.general.notification_title);
    let body = config::resolve_placeholders(&cfg.general.notification_body);

    let status = std::process::Command::new("notify-send")
        .args([&title, &body])
        .status();

    if let Err(e) = status {
        eprintln!("Failed to run notify-send: {e}");
        std::process::exit(1);
    }
}
