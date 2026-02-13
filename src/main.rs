use cursor_cli_wrapper::{config, monitor, state};
use std::io::IsTerminal;
use std::os::fd::AsRawFd;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::fs::OpenOptions;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::signal::unix::{SignalKind, signal};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let (pty, pts) = pty_process::open().unwrap_or_else(|e| {
        eprintln!("failed to create pty: {e}");
        std::process::exit(1);
    });

    // Match the PTY size to the real terminal
    if let Ok((cols, rows)) = crossterm::terminal::size() {
        let _ = pty.resize(pty_process::Size::new(rows, cols));
    }

    // Save raw fd for SIGWINCH resize (valid as long as pty halves live)
    let pty_raw_fd = pty.as_raw_fd();

    // FIXME: support overriding the cursor-agent path via an env var
    //        (e.g. CURSOR_AGENT_PATH)
    let cursor_agent_bin = dirs::home_dir()
        .expect("could not determine home directory")
        .join(".local/bin/cursor-agent");

    let mut child = pty_process::Command::new(&cursor_agent_bin)
        .args(&args)
        .spawn(pts)
        .unwrap_or_else(|e| {
            eprintln!("failed to spawn {}: {e}", cursor_agent_bin.display());
            std::process::exit(1);
        });

    let (mut pty_reader, mut pty_writer) = pty.into_split();

    // Enable raw mode so keypresses are forwarded immediately
    let is_tty = std::io::stdin().is_terminal();
    if is_tty {
        crossterm::terminal::enable_raw_mode().unwrap_or_else(|e| {
            eprintln!("failed to enable raw mode: {e}");
            std::process::exit(1);
        });
    }

    // Forward terminal resize (SIGWINCH) to the PTY
    tokio::spawn(async move {
        if let Ok(mut sigwinch) = signal(SignalKind::window_change()) {
            while sigwinch.recv().await.is_some() {
                if let Ok((cols, rows)) = crossterm::terminal::size() {
                    let ws = libc::winsize {
                        ws_row: rows,
                        ws_col: cols,
                        ws_xpixel: 0,
                        ws_ypixel: 0,
                    };
                    unsafe {
                        libc::ioctl(pty_raw_fd, libc::TIOCSWINSZ, &ws);
                    }
                }
            }
        }
    });

    // Load config into shared state and spawn file watcher
    let cfg = Arc::new(RwLock::new(config::Config::load()));
    {
        let cfg = Arc::clone(&cfg);
        tokio::spawn(config::watch_config(cfg));
    }

    state::set_tmux_status("IDLE", cfg.read().unwrap().hooks.status_change.as_deref());

    // Optionally dump all raw stdin input to a file (for debugging keypresses)
    let mut input_dump_file = match std::env::var("CURSOR_WRAPPER_INPUT_DUMP_FILE") {
        Ok(path) if !path.is_empty() => Some(
            OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&path)
                .await
                .unwrap_or_else(|e| {
                    eprintln!("failed to open input dump file {path}: {e}");
                    std::process::exit(1);
                }),
        ),
        _ => None,
    };

    // Relay stdin -> PTY
    let stdin_cfg = Arc::clone(&cfg);
    let _stdin_task = tokio::spawn(async move {
        let mut stdin = io::stdin();
        let mut buf = [0u8; 4096];
        loop {
            let n = match stdin.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            const ALT_I: &[u8] = b"\x1bi";
            const ESC: u8 = 0x1b;

            let data = &buf[..n];
            let cfg_snapshot = stdin_cfg.read().unwrap().clone();

            // Dump raw input to file when configured
            if let Some(ref mut f) = input_dump_file {
                let _ = f.write_all(data).await;
                let _ = f.flush().await;
            }

            // Detect Alt+I and reset status to IDLE
            if data.windows(ALT_I.len()).any(|w| w == ALT_I) {
                state::set_tmux_status("IDLE", cfg_snapshot.hooks.status_change.as_deref());
            }

            // Detect standalone ESC while in vim NORMAL mode and fire hook.
            // A lone ESC is a single byte (not part of an escape sequence
            // like Alt+key or arrow keys which arrive as multi-byte reads).
            if n == 1
                && data[0] == ESC
                && state::get_vim_mode() == state::VimMode::Normal
            {
                if let Some(ref cmd) = cfg_snapshot.hooks.esc_in_normal {
                    state::run_hook(cmd);
                }
            }
            if pty_writer.write_all(data).await.is_err() {
                break;
            }
        }
    });

    // Optionally dump all raw PTY output to a file (like tmux pipe-pane)
    let mut dump_file = match std::env::var("CURSOR_WRAPPER_DUMP_FILE") {
        Ok(path) if !path.is_empty() => Some(
            OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&path)
                .await
                .unwrap_or_else(|e| {
                    eprintln!("failed to open dump file {path}: {e}");
                    std::process::exit(1);
                }),
        ),
        _ => None,
    };

    // Relay PTY -> stdout, with output monitoring for notifications
    let stdout_cfg = Arc::clone(&cfg);
    let stdout_task = tokio::spawn(async move {
        let mut stdout = io::stdout();
        let mut buf = [0u8; 4096];
        let mut monitor = monitor::OutputMonitor::new();

        loop {
            // Use a timeout so we can check for state transitions
            // even when no new data arrives from the PTY.
            let result =
                tokio::time::timeout(Duration::from_secs(1), pty_reader.read(&mut buf)).await;

            match result {
                Ok(Ok(0)) | Ok(Err(_)) => break,
                Ok(Ok(n)) => {
                    let chunk = &buf[..n];
                    let result = monitor.process_chunk(chunk);
                    if result.entered_busy {
                        let hook = stdout_cfg.read().unwrap().hooks.status_change.clone();
                        state::set_tmux_status("INPROGRESS", hook.as_deref());
                    }
                    if let Some(mode) = result.vim_mode_changed {
                        let hook = stdout_cfg.read().unwrap().hooks.vim_mode_change.clone();
                        if let Some(cmd) = hook {
                            let cmd = cmd.replace("{vim_mode}", mode.as_str());
                            state::run_hook(&cmd);
                        }
                    }

                    if stdout.write_all(chunk).await.is_err() {
                        break;
                    }
                    let _ = stdout.flush().await;

                    // Dump raw output to file when configured
                    if let Some(ref mut f) = dump_file {
                        let _ = f.write_all(chunk).await;
                        let _ = f.flush().await;
                    }
                }
                Err(_timeout) => {
                    // No data for 1s — just check for transitions below
                }
            }

            if monitor.check_transition() {
                // Agent finished generating/thinking — fire notification
                let cfg_snapshot = stdout_cfg.read().unwrap().clone();
                state::set_tmux_status("WAITING", cfg_snapshot.hooks.status_change.as_deref());
                let args = cfg_snapshot.general.notify_send_args();
                let _ = tokio::process::Command::new("notify-send")
                    .args(&args)
                    .spawn();
            }
        }
    });

    let status = child.wait().await.unwrap_or_else(|e| {
        if is_tty {
            let _ = crossterm::terminal::disable_raw_mode();
        }
        eprintln!("failed to wait on cursor-agent: {e}");
        std::process::exit(1);
    });

    // Wait for remaining output to flush
    let _ = stdout_task.await;

    if is_tty {
        let _ = crossterm::terminal::disable_raw_mode();
    }

    // Clear tmux status on exit
    state::set_tmux_status("", cfg.read().unwrap().hooks.status_change.as_deref());

    std::process::exit(status.code().unwrap_or(1));
}
