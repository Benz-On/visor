use std::io::{self, Read, Write};
use std::process::{Command, Output, Stdio};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use wait_timeout::ChildExt;

fn read_pipe<R>(mut pipe: R) -> io::Result<Vec<u8>>
where
    R: Read + Send + 'static,
{
    let mut bytes = Vec::new();
    pipe.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn join_reader(reader: JoinHandle<io::Result<Vec<u8>>>) -> io::Result<Vec<u8>> {
    reader
        .join()
        .map_err(|_| io::Error::other("command output reader stopped unexpectedly"))?
}

/// Runs a child process while draining both output pipes and enforcing a hard deadline.
///
/// Draining concurrently avoids deadlocks when a hardware probe returns more data than
/// an OS pipe can buffer. On timeout, the child is terminated and fully reaped.
pub fn output(
    command: &mut Command,
    stdin_bytes: Option<&[u8]>,
    timeout: Duration,
) -> io::Result<Output> {
    if stdin_bytes.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command.spawn()?;
    if let Some(bytes) = stdin_bytes {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("command input pipe was not created"))?;
        stdin.write_all(bytes)?;
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("command output pipe was not created"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("command error pipe was not created"))?;
    let stdout_reader = thread::spawn(move || read_pipe(stdout));
    let stderr_reader = thread::spawn(move || read_pipe(stderr));

    let status = match child.wait_timeout(timeout)? {
        Some(status) => status,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = join_reader(stdout_reader);
            let _ = join_reader(stderr_reader);
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("external command exceeded {} ms", timeout.as_millis()),
            ));
        }
    };

    Ok(Output {
        status,
        stdout: join_reader(stdout_reader)?,
        stderr: join_reader(stderr_reader)?,
    })
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn terminates_a_stalled_command() {
        let mut command = Command::new("powershell.exe");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Start-Sleep -Seconds 5",
        ]);
        let error = output(&mut command, None, Duration::from_millis(100)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    }
}
