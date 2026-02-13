use crate::state::{self, VimMode};
use std::time::{Duration, Instant};

const DEBOUNCE: Duration = Duration::from_millis(200);

/// Cursor styling that indicates vim NORMAL mode in the input box.
/// The sequence is: ESC[22m (normal intensity, ending the dim arrow) immediately
/// followed by ESC[100m (bright black / gray background on the cursor character).
const NORMAL_MODE_SIG: &[u8] = b"\x1b[22m\x1b[100m";

/// Cursor styling that indicates vim INSERT mode in the input box.
/// The sequence is: ESC[22m immediately followed by ESC[7m (reverse video).
const INSERT_MODE_SIG: &[u8] = b"\x1b[22m\x1b[7m";

/// Check whether the (ANSI-stripped) text contains a busy indicator.
///
/// Detects the hexagon spinner icons that cursor-agent uses for loading
/// states: filled `⬢` (U+2B22) and hollow `⬡` (U+2B21).  These
/// characters only appear on the status line during active
/// generation/thinking and are absent once the agent finishes.
fn is_busy(text: &str) -> bool {
    // FIXME: detect dots as well
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

        let stripped = strip_ansi_escapes::strip(raw);
        let text = String::from_utf8_lossy(&stripped);

        let entered_busy = if is_busy(&text) {
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
        let new_mode = if raw.windows(NORMAL_MODE_SIG.len()).any(|w| w == NORMAL_MODE_SIG) {
            Some(VimMode::Normal)
        } else if raw.windows(INSERT_MODE_SIG.len()).any(|w| w == INSERT_MODE_SIG) {
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
        assert!(is_busy("  ⬢ Generating"));
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
        assert!(is_busy("  ⬡ Thinking     202 tokens"));
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
