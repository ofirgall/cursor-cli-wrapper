/// Set the tmux user option `@cursor-cli-wrapper-status` on the current session
/// and run the `[hooks] status-change` command if configured.
///
/// Silently does nothing for tmux if not running inside tmux.
pub fn set_tmux_status(value: &str, hook: Option<&str>) {
    if value.is_empty() {
        // Unset the option so it doesn't linger
        let _ = std::process::Command::new("tmux")
            .args(["set-option", "-qu", "@cursor-cli-wrapper-status"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    } else {
        let _ = std::process::Command::new("tmux")
            .args(["set-option", "-q", "@cursor-cli-wrapper-status", value])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }

    if let Some(cmd) = hook {
        let cmd = cmd.replace("{status}", value);
        let _ = std::process::Command::new("sh")
            .args(["-c", &cmd])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}
