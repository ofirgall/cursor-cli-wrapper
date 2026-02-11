# Notifications

## Overview

The wrapper monitors cursor-agent's PTY output and sends a desktop notification
via `notify-send` when the agent finishes a loading/busy phase.

## How It Works

The output monitor tracks a simple state machine:

- **Idle** — agent is waiting for user input
- **Busy** — agent is actively working (generating, thinking, reading, etc.)

### Detection

Each chunk of PTY output is stripped of ANSI escape codes and scanned for the
**dot-animation pattern** that cursor-agent uses for all loading states.

The detector looks for a capitalized word immediately followed by 1-3 trailing
dots, e.g.:

| Example          | Matched? |
|------------------|----------|
| `Generating.`    | Yes      |
| `Thinking..`     | Yes      |
| `Reading...`     | Yes      |
| `(Thinking)`     | No — no trailing dots |
| `hello.`         | No — not capitalized  |

This approach automatically handles any current or future loading state without
needing to know the state names in advance.

### Transition & Debounce

When the agent is busy and no dot-animation pattern has been seen for 500ms (the
debounce window), the monitor fires a transition to Idle and triggers:

```
notify-send "Cursor Agent" "Done"
```

The debounce prevents false positives caused by partial screen redraws where the
pattern might momentarily be absent between chunks.

A 1-second read timeout on the PTY relay ensures the transition check runs even
if cursor-agent stops sending output after finishing.

## Requirements

- `notify-send` must be available on `$PATH` (provided by `libnotify` /
  `libnotify-bin` on most Linux distributions).
