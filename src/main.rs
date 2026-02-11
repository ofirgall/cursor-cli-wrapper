use tokio::io;
use tokio::process::Command;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let mut child = match Command::new("cursor-agent")
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            eprintln!("failed to spawn cursor-agent: {e}");
            std::process::exit(1);
        }
    };

    let mut child_stdin = child.stdin.take().expect("piped stdin");
    let mut child_stdout = child.stdout.take().expect("piped stdout");
    let mut child_stderr = child.stderr.take().expect("piped stderr");

    let _stdin_relay = tokio::spawn(async move {
        let mut stdin = io::stdin();
        // Ignore error — child may exit and close its stdin before we finish
        let _ = io::copy(&mut stdin, &mut child_stdin).await;
    });

    let stdout_relay = tokio::spawn(async move {
        let mut stdout = io::stdout();
        let _ = io::copy(&mut child_stdout, &mut stdout).await;
    });

    let stderr_relay = tokio::spawn(async move {
        let mut stderr = io::stderr();
        let _ = io::copy(&mut child_stderr, &mut stderr).await;
    });

    let status = child.wait().await.expect("failed to wait on cursor-agent");

    // Wait for output relay tasks to flush remaining data
    let _ = stdout_relay.await;
    let _ = stderr_relay.await;

    // Exit immediately — the stdin relay may be stuck on a blocking read
    // that cannot be cancelled, so we bypass tokio runtime shutdown.
    std::process::exit(status.code().unwrap_or(1));
}
