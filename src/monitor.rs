use crate::state::{self, VimMode};
use regex::bytes::Regex;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

const DEBOUNCE: Duration = Duration::from_secs(1);

/// Regex matching the vim NORMAL mode cursor styling:
/// ESC[100m {any char} ESC[49m
static NORMAL_MODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\[100m.\x1b\[49m").unwrap());

/// Regex matching the vim INSERT mode cursor styling:
/// ESC[7m {any char} ESC[27m
static INSERT_MODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\[7m.\x1b\[27m").unwrap());

/// Regex matching two consecutive Braille Pattern chars (U+2800–U+28FF).
/// In UTF-8 each is \xe2[\xa0-\xa3][\x80-\xbf].
static BRAILLE_SPINNER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?-u)\xe2[\xa0-\xa3][\x80-\xbf]\xe2[\xa0-\xa3][\x80-\xbf]").unwrap()
});

/// Check whether the raw PTY output contains a busy indicator.
///
/// Detects (on ANSI-stripped text):
/// - Braille spinner (two consecutive braille-pattern chars) — current Cursor indicator.
/// - Bullet spinner `•` (U+2022) — previous Cursor indicator.
/// - Hexagon spinners `⬢` (U+2B22) / `⬡` (U+2B21) — legacy indicator.
fn is_busy(raw: &[u8]) -> bool {
    if BRAILLE_SPINNER_RE.is_match(raw) {
        return true;
    }
    let stripped = strip_ansi_escapes::strip(raw);
    let text = String::from_utf8_lossy(&stripped);
    text.contains('\u{2B22}') || text.contains('\u{2B21}')
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentState {
    Idle,
    Busy,
}

/// Result of processing a single PTY output chunk.
pub struct ChunkResult {
    /// `true` when the agent first enters the Busy state (Idle -> Busy).
    pub entered_busy: bool,
    /// Set when the vim mode changed compared to the previous chunk.
    pub vim_mode_changed: Option<VimMode>,
}

pub struct OutputMonitor {
    state: AgentState,
    last_busy_seen: Instant,
    last_vim_mode: VimMode,
}

impl OutputMonitor {
    pub fn new() -> Self {
        Self {
            state: AgentState::Idle,
            last_busy_seen: Instant::now(),
            last_vim_mode: VimMode::Insert,
        }
    }

    /// Scan a raw PTY output chunk for busy patterns and vim mode changes.
    /// Strips ANSI escape codes before matching.
    pub fn process_chunk(&mut self, raw: &[u8]) -> ChunkResult {
        // Detect vim mode changes from cursor styling escape sequences.
        let vim_mode_changed = self.detect_vim_mode(raw);

        let entered_busy = if is_busy(raw) {
            let entered = self.state == AgentState::Idle;
            self.state = AgentState::Busy;
            self.last_busy_seen = Instant::now();
            entered
        } else {
            false
        };

        ChunkResult {
            entered_busy,
            vim_mode_changed,
        }
    }

    /// Detect vim mode transitions from the raw cursor styling sequences
    /// that the Cursor Agent input box emits.
    ///
    /// Returns `Some(mode)` when the mode *changes*, `None` otherwise.
    fn detect_vim_mode(&mut self, raw: &[u8]) -> Option<VimMode> {
        let new_mode = if NORMAL_MODE_RE.is_match(raw) {
            Some(VimMode::Normal)
        } else if INSERT_MODE_RE.is_match(raw) {
            Some(VimMode::Insert)
        } else {
            None
        };

        if let Some(mode) = new_mode {
            state::set_vim_mode(mode);
            if mode != self.last_vim_mode {
                self.last_vim_mode = mode;
                return Some(mode);
            }
        }
        None
    }

    /// Returns `true` (once) when the agent transitions from Busy to Idle,
    /// i.e. no busy pattern has been seen for the debounce duration.
    pub fn check_transition(&mut self) -> bool {
        if self.state == AgentState::Busy && self.last_busy_seen.elapsed() > DEBOUNCE {
            self.state = AgentState::Idle;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Braille spinner (current Cursor indicator) --

    #[test]
    fn braille_spinner_generating() {
        // ESC[32m ⡕⡰ ESC[39m  ESC[1m Generating ESC[22m
        let raw = b" \x1b[32m\xe2\xa1\x95\xe2\xa1\xb0\x1b[39m \x1b[1mGenerating\x1b[22m";
        assert!(is_busy(raw));
    }

    #[test]
    fn bare_braille_spinner_generating() {
        assert!(is_busy("⡕⡰ Generating".as_bytes()));
    }

    // -- Bullet spinner (previous Cursor indicator) --

    #[test]
    fn bullet_generating() {
        assert!(is_busy("  • Generating...".as_bytes()));
    }

    #[test]
    fn bullet_with_ansi_generating() {
        let raw = b"  \x1b[32m\xe2\x80\xa2\x1b[39m \x1b[1mGenerating...\x1b[22m";
        assert!(is_busy(raw));
    }

    // -- Hexagon spinners (legacy indicator) --

    #[test]
    fn legacy_filled_hexagon() {
        assert!(is_busy("  ⬢ Generating...".as_bytes()));
    }

    #[test]
    fn legacy_hollow_hexagon() {
        assert!(is_busy("  ⬡ Thinking...  202 tokens".as_bytes()));
    }

    // -- Idle / done states --

    #[test]
    fn done_state_is_not_busy() {
        let done_text = b"  I was saying that the current AI interaction \
                         you're having right now is a prompt that's running";
        assert!(!is_busy(done_text));
    }

    #[test]
    fn plain_text_is_not_busy() {
        assert!(!is_busy(b"Hello world"));
        assert!(!is_busy(b""));
    }
}
