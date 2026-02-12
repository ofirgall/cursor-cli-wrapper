use std::time::{Duration, Instant};

const DEBOUNCE: Duration = Duration::from_millis(200);

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

pub struct OutputMonitor {
    state: AgentState,
    last_busy_seen: Instant,
}

impl OutputMonitor {
    pub fn new() -> Self {
        Self {
            state: AgentState::Idle,
            last_busy_seen: Instant::now(),
        }
    }

    /// Scan a raw PTY output chunk for busy patterns.
    /// Strips ANSI escape codes before matching.
    ///
    /// Returns `true` when the agent first enters the Busy state
    /// (i.e. transitions from Idle to Busy).
    pub fn process_chunk(&mut self, raw: &[u8]) -> bool {
        let stripped = strip_ansi_escapes::strip(raw);
        let text = String::from_utf8_lossy(&stripped);

        if is_busy(&text) {
            let entered_busy = self.state == AgentState::Idle;
            self.state = AgentState::Busy;
            self.last_busy_seen = Instant::now();
            return entered_busy;
        }
        false
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
