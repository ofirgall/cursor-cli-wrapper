use serde::Deserialize;
use std::path::PathBuf;

fn default_notification_title() -> String {
    "Cursor Agent".to_string()
}

fn default_notification_body() -> String {
    "Done".to_string()
}

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub general: General,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: General::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct General {
    #[serde(default = "default_notification_title", rename = "notification-title")]
    pub notification_title: String,

    #[serde(default = "default_notification_body", rename = "notification-body")]
    pub notification_body: String,
}

impl Default for General {
    fn default() -> Self {
        Self {
            notification_title: default_notification_title(),
            notification_body: default_notification_body(),
        }
    }
}

impl Config {
    /// Load config from `~/.config/cursor-cli-wrapper/config.toml`.
    /// Returns defaults if the file is missing or unparseable.
    pub fn load() -> Self {
        Self::config_path()
            .and_then(|path| std::fs::read_to_string(&path).ok())
            .and_then(|contents| toml::from_str::<Config>(&contents).ok())
            .unwrap_or_default()
    }

    fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("cursor-cli-wrapper").join("config.toml"))
    }
}

/// Replace `{cwd}`, `{git_branch}`, `{git_repo}`, and `{tmux-session}`
/// placeholders in the given template string with their current values.
///
/// Placeholders that cannot be resolved (e.g. not in a git repo) are
/// replaced with an empty string.
pub fn resolve_placeholders(template: &str) -> String {
    let mut result = template.to_string();

    if result.contains("{cwd}") {
        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        result = result.replace("{cwd}", &cwd);
    }

    if result.contains("{git_branch}") {
        let branch = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        result = result.replace("{git_branch}", &branch);
    }

    if result.contains("{git_repo}") {
        let repo = std::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| {
                let path = String::from_utf8_lossy(&o.stdout).trim().to_string();
                std::path::Path::new(&path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
            })
            .unwrap_or_default();
        result = result.replace("{git_repo}", &repo);
    }

    if result.contains("{tmux-session}") {
        let session = std::process::Command::new("tmux")
            .args(["display-message", "-p", "#S"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        result = result.replace("{tmux-session}", &session);
    }

    result
}
