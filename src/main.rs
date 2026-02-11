use std::io::IsTerminal;
use std::os::fd::AsRawFd;
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

    let mut child = pty_process::Command::new("cursor-agent")
        .args(&args)
        .spawn(pts)
        .unwrap_or_else(|e| {
            eprintln!("failed to spawn cursor-agent: {e}");
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

    // Relay stdin -> PTY
    let _stdin_task = tokio::spawn(async move {
        let mut stdin = io::stdin();
        let mut buf = [0u8; 4096];
        loop {
            let n = match stdin.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            if pty_writer.write_all(&buf[..n]).await.is_err() {
                break;
            }
        }
    });

    // Relay PTY -> stdout
    let stdout_task = tokio::spawn(async move {
        let mut stdout = io::stdout();
        let mut buf = [0u8; 4096];
        loop {
            let n = match pty_reader.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            if stdout.write_all(&buf[..n]).await.is_err() {
                break;
            }
            let _ = stdout.flush().await;
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

    std::process::exit(status.code().unwrap_or(1));
}
