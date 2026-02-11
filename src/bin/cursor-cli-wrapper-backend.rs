use cursor_cli_wrapper::{config, state};

fn print_usage() {
    eprintln!("Usage: cursor-cli-wrapper-backend <command>");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  --notify            Send a test notification");
    eprintln!("  --status <value>    Set tmux status (INPROGRESS, WAITING, or empty to clear)");
}

fn cmd_notify() {
    let cfg = config::Config::load();
    let args = cfg.general.notify_send_args();

    let status = std::process::Command::new("notify-send")
        .args(&args)
        .status();

    if let Err(e) = status {
        eprintln!("Failed to run notify-send: {e}");
        std::process::exit(1);
    }
}

fn cmd_status(value: &str) {
    let cfg = config::Config::load();
    state::set_tmux_status(value, cfg.hooks.status_change.as_deref());
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(|s| s.as_str()) {
        Some("--notify") => cmd_notify(),
        Some("--status") => {
            let value = args.get(1).map(|s| s.as_str()).unwrap_or("");
            cmd_status(value);
        }
        _ => {
            print_usage();
            std::process::exit(1);
        }
    }
}
