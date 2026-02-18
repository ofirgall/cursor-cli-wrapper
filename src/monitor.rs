use crate::state::{self, VimMode};
use regex::bytes::Regex as BytesRegex;
use regex::Regex;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

const DEBOUNCE_TO_IDLE: Duration = Duration::from_millis(200);
const DEBOUNCE_TO_BUSY: Duration = Duration::from_secs(1);

/// Regex matching the vim NORMAL mode cursor styling:
/// ESC[100m {any char} ESC[49m
static NORMAL_MODE_RE: LazyLock<BytesRegex> =
    LazyLock::new(|| BytesRegex::new(r"\x1b\[100m.\x1b\[49m").unwrap());

/// Regex matching the vim INSERT mode cursor styling:
/// ESC[7m {any char} ESC[27m
static INSERT_MODE_RE: LazyLock<BytesRegex> =
    LazyLock::new(|| BytesRegex::new(r"\x1b\[7m.\x1b\[27m").unwrap());

static BUSY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\u{2B22}\u{2B21}].*\.{1,3}").unwrap());

/// Check whether the (ANSI-stripped) text contains a busy indicator.
///
/// Detects the hexagon spinner icons that cursor-agent uses for loading
/// states: filled `⬢` (U+2B22) and hollow `⬡` (U+2B21).  These
/// characters only appear on the status line during active
/// generation/thinking and are absent once the agent finishes.
fn is_busy(text: &str) -> bool {
    text.lines().any(|line| BUSY_RE.is_match(line))
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
    /// When the current uninterrupted streak of busy chunks started.
    busy_since: Option<Instant>,
    last_vim_mode: VimMode,
}

impl OutputMonitor {
    pub fn new() -> Self {
        Self {
            state: AgentState::Idle,
            last_busy_seen: Instant::now(),
            busy_since: None,
            last_vim_mode: VimMode::Insert,
        }
    }

    /// Scan a raw PTY output chunk for busy patterns and vim mode changes.
    /// Strips ANSI escape codes before matching.
    pub fn process_chunk(&mut self, raw: &[u8]) -> ChunkResult {
        // Detect vim mode changes from cursor styling escape sequences.
        let vim_mode_changed = self.detect_vim_mode(raw);

        let stripped = strip_ansi_escapes::strip(raw);
        let text = String::from_utf8_lossy(&stripped);

        let entered_busy = if is_busy(&text) {
            self.last_busy_seen = Instant::now();
            if self.state == AgentState::Busy {
                false
            } else {
                let since = *self.busy_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= DEBOUNCE_TO_BUSY {
                    self.state = AgentState::Busy;
                    true
                } else {
                    false
                }
            }
        } else {
            self.busy_since = None;
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
        if self.state == AgentState::Busy && self.last_busy_seen.elapsed() > DEBOUNCE_TO_IDLE {
            self.state = AgentState::Idle;
            self.busy_since = None;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Generating states (from shots/generating/) --

    #[test]
    fn generating_filled_hexagon_three_dots() {
        // shots/generating/1.txt
        assert!(is_busy("  ⬢ Generating..."));
    }

    #[test]
    fn generating_hollow_hexagon_one_dot() {
        // shots/generating/2.txt
        assert!(is_busy("  ⬡ Generating."));
    }

    #[test]
    fn generating_filled_hexagon_no_dots() {
        // shots/generating/3.txt
        assert!(!is_busy("  ⬢ Generating"));
    }

    // -- Thinking states (from shots/thinking/) --

    #[test]
    fn thinking_hollow_hexagon_three_dots() {
        // shots/thinking/1.txt
        assert!(is_busy("  ⬡ Thinking...  202 tokens"));
    }

    #[test]
    fn thinking_filled_hexagon_one_dot() {
        // shots/thinking/2.txt
        assert!(is_busy("  ⬢ Thinking.    202 tokens"));
    }

    #[test]
    fn thinking_hollow_hexagon_no_dots() {
        // shots/thinking/3.txt
        assert!(!is_busy("  ⬡ Thinking     202 tokens"));
    }

    // -- Done / idle state (from shots/done/) --

    #[test]
    fn done_state_is_not_busy() {
        // shots/done/1.txt: normal response text, no hexagons
        let done_text = "  I think you're saying that \"this\" — the current AI interaction \
                         you're having right now — is \"a prompt that's running\"";
        assert!(!is_busy(done_text));
    }

    #[test]
    fn plain_text_is_not_busy() {
        assert!(!is_busy("Generating..."));
        assert!(!is_busy("Hello world"));
        assert!(!is_busy(""));
    }
}
