use serde::Deserialize;
use std::path::PathBuf;

fn default_notification_title() -> String {
    "Cursor Agent".to_string()
}

fn default_notification_body() -> String {
    "Done".to_string()
}

fn default_notification_urgency() -> Urgency {
    Urgency::Normal
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Urgency {
    Low,
    Normal,
    Critical,
}

impl Urgency {
    pub fn as_str(self) -> &'static str {
        match self {
            Urgency::Low => "low",
            Urgency::Normal => "normal",
            Urgency::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub general: General,

    #[serde(default)]
    pub hooks: Hooks,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: General::default(),
            hooks: Hooks::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Hooks {
    #[serde(default, rename = "status-change")]
    pub status_change: Option<String>,

    /// Command to run when ESC is pressed while the agent is in vim NORMAL mode.
    #[serde(default, rename = "esc-in-normal")]
    pub esc_in_normal: Option<String>,

    /// Command to run when the vim mode changes (e.g. insert -> normal).
    /// The placeholder `{vim_mode}` is replaced with the new mode name
    /// (`normal` or `insert`).
    #[serde(default, rename = "vim-mode-change")]
    pub vim_mode_change: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct General {
    #[serde(default = "default_notification_title", rename = "notification-title")]
    pub notification_title: String,

    #[serde(default = "default_notification_body", rename = "notification-body")]
    pub notification_body: String,

    #[serde(default = "default_notification_urgency", rename = "notification-urgency")]
    pub notification_urgency: Urgency,

    #[serde(default, rename = "notification-app-name")]
    pub notification_app_name: Option<String>,

    #[serde(default, rename = "notification-icon")]
    pub notification_icon: Option<String>,
}

impl Default for General {
    fn default() -> Self {
        Self {
            notification_title: default_notification_title(),
            notification_body: default_notification_body(),
            notification_urgency: default_notification_urgency(),
            notification_app_name: None,
            notification_icon: None,
        }
    }
}

impl General {
    /// Build the full `notify-send` argument list from the config,
    /// resolving placeholders in title and body.
    pub fn notify_send_args(&self) -> Vec<String> {
        let mut args = vec![
            "-u".to_string(),
            self.notification_urgency.as_str().to_string(),
        ];
        if let Some(ref app_name) = self.notification_app_name {
            args.push("--app-name".to_string());
            args.push(app_name.clone());
        }
        if let Some(ref icon) = self.notification_icon {
            args.push("--icon".to_string());
            args.push(icon.clone());
        }
        args.push(resolve_placeholders(&self.notification_title));
        args.push(resolve_placeholders(&self.notification_body));
        args
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

    pub(crate) fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("cursor-cli-wrapper").join("config.toml"))
    }
}

/// Watch the config file for changes and reload when valid.
///
/// Polls the file's modification time every 2 seconds. If the file changes
/// and the new contents parse successfully, the shared config is updated.
/// Invalid configs are silently ignored (the previous config is kept).
pub async fn watch_config(shared: std::sync::Arc<std::sync::RwLock<Config>>) {
    let Some(path) = Config::config_path() else {
        return;
    };

    let mut last_modified = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok();

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
    interval.tick().await; // first tick is immediate, skip it

    loop {
        interval.tick().await;

        let current_modified = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok();

        if current_modified != last_modified {
            last_modified = current_modified;

            if let Ok(contents) = std::fs::read_to_string(&path) {
                if let Ok(new_cfg) = toml::from_str::<Config>(&contents) {
                    if let Ok(mut cfg) = shared.write() {
                        *cfg = new_cfg;
                    }
                }
            }
        }
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
