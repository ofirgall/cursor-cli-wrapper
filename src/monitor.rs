use std::time::{Duration, Instant};

const DEBOUNCE: Duration = Duration::from_millis(1500);

/// Check whether the (ANSI-stripped) text contains a busy indicator.
///
/// Instead of matching specific keywords, this detects the universal
/// dot-animation pattern used by cursor-agent for loading states:
/// a capitalized word immediately followed by 1-3 dots (e.g.
/// "Thinking.", "Generating..", "Reading...").
///
/// This automatically handles any current or future loading state
/// without needing to know the state names in advance.
fn is_busy(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Look for an uppercase letter that starts a word
        if chars[i].is_ascii_uppercase() {
            let word_start = i;
            i += 1;

            // Consume lowercase letters
            while i < len && chars[i].is_ascii_lowercase() {
                i += 1;
            }

            // Need at least 2 characters for a real word (e.g. "Reading", not "R")
            if i - word_start < 2 {
                continue;
            }

            // Count trailing dots
            let dot_start = i;
            while i < len && chars[i] == '.' {
                i += 1;
            }
            let dot_count = i - dot_start;

            // Match if we found 1-3 dots followed by whitespace or end-of-string
            if dot_count >= 1
                && dot_count <= 3
                && (i >= len || chars[i].is_ascii_whitespace())
            {
                return true;
            }
        } else {
            i += 1;
        }
    }

    false
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
