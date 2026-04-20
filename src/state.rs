use std::sync::atomic::{AtomicU8, Ordering};

/// The current vim mode of the Cursor Agent input field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum VimMode {
    Insert = 0,
    Normal = 1,
}

impl VimMode {
    fn from_u8(v: u8) -> Self {
        if v == 1 {
            VimMode::Normal
        } else {
            VimMode::Insert
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            VimMode::Insert => "insert",
            VimMode::Normal => "normal",
        }
    }
}

/// Global atomic storing the current vim mode.
static VIM_MODE: AtomicU8 = AtomicU8::new(0);

/// Update the tracked vim mode.
pub fn set_vim_mode(mode: VimMode) {
    VIM_MODE.store(mode as u8, Ordering::Relaxed);
}

/// Read the current vim mode.
pub fn get_vim_mode() -> VimMode {
    VimMode::from_u8(VIM_MODE.load(Ordering::Relaxed))
}

/// Run a shell command in the foreground, discarding output.
pub fn run_hook(cmd: &str) {
    let _ = std::process::Command::new("sh")
        .args(["-c", cmd])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// Set the tmux user options `@ai-agent-status` and `@ai-agent` on the current
/// window, and run the `[hooks] status-change` command if configured.
///
/// Silently does nothing for tmux if not running inside tmux.
pub fn set_tmux_status(value: &str, hook: Option<&str>) {
    if value.is_empty() {
        // Unset the options so they don't linger
        let _ = std::process::Command::new("tmux")
            .args(["set-option", "-wqu", "@ai-agent-status"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        let _ = std::process::Command::new("tmux")
            .args(["set-option", "-wqu", "@ai-agent"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    } else {
        let _ = std::process::Command::new("tmux")
            .args(["set-option", "-wq", "@ai-agent", "cursor"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        let _ = std::process::Command::new("tmux")
            .args(["set-option", "-wq", "@ai-agent-status", value])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }

    if let Some(cmd) = hook {
        let cmd = cmd.replace("{status}", value);
        run_hook(&cmd);
    }
}
