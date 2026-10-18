use crate::{Cancel, Error, ErrorKind};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

pub(crate) struct Output {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub(crate) fn run(directory: Option<&Path>, args: &[OsString], cancel: &Cancel) -> Result<Output, Error> {
    run_with_input(directory, args, &[], cancel)
}

pub(crate) fn run_with_input(directory: Option<&Path>, args: &[OsString], input: &[u8], cancel: &Cancel) -> Result<Output, Error> {
    if cancel.is_cancelled() {
        return Err(Error::new(ErrorKind::Cancelled, "Git scan cancelled"));
    }
    let mut command = crate::read_only::command(directory, args, !input.is_empty());
    let mut child = command.spawn().map_err(|error| Error::io("Could not start Git", error))?;
    let (sender, receiver) = mpsc::channel();
    let input_writer = child.stdin.take().map(|mut stdin| {
        let input = input.to_owned();
        let sender = sender.clone();
        thread::spawn(move || {
            let result = stdin.write_all(&input)
                .map(|_| Vec::new())
                .map_err(|error| Error::io("Could not write Git input", error));
            let _ = sender.send((2, result));
        })
    });
    let pipes: [Box<dyn Read + Send>; 2] = [Box::new(child.stdout.take().unwrap()), Box::new(child.stderr.take().unwrap())];
    let readers: Vec<_> = pipes.into_iter().enumerate().map(|(index, pipe)| {
        let sender = sender.clone();
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = pipe.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)
                .map_err(|error| Error::io("Could not read Git output", error))
                .and_then(|_| if bytes.len() > 16 * 1024 * 1024 {
                    Err(Error::new(ErrorKind::OutputLimit, "Git output exceeded 16 MiB"))
                } else { Ok(bytes) });
            let _ = sender.send((index, result));
        })
    }).collect();
    drop(sender);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut buffers: [Option<Vec<u8>>; 2] = [None, None];
    let mut input_complete = input_writer.is_none();
    let result = (|| {
        let mut streams_remaining = readers.len() + usize::from(!input_complete);
        let mut disconnected = false;
        loop {
            if cancel.is_cancelled() { return Err(Error::new(ErrorKind::Cancelled, "Git scan cancelled")); }
            if Instant::now() >= deadline { return Err(Error::new(ErrorKind::Timeout, "Git command timed out after 10 seconds")); }
            let wait = deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(20));
            if streams_remaining > 0 && !disconnected {
                match receiver.recv_timeout(wait) {
                    Ok((index, result)) => {
                        if index < 2 {
                            buffers[index] = Some(result?);
                        } else {
                            result?;
                            input_complete = true;
                        }
                        streams_remaining -= 1;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => { disconnected = true; }
                }
            } else {
                cancel.wait(wait);
            }
            while let Ok((index, bytes)) = receiver.try_recv() {
                if index < 2 {
                    if buffers[index].is_none() {
                        buffers[index] = Some(bytes?);
                        streams_remaining = streams_remaining.saturating_sub(1);
                    }
                } else if !input_complete {
                    bytes?;
                    input_complete = true;
                    streams_remaining = streams_remaining.saturating_sub(1);
                }
            }
            if let Some(status) = child.try_wait().map_err(|error| Error::io("Could not wait for Git", error))? {
                if streams_remaining == 0 && buffers.iter().all(Option::is_some) {
                    return Ok(Output { code: status.code().unwrap_or(-1),
                        stdout: buffers[0].take().unwrap(), stderr: buffers[1].take().unwrap() });
                }
            }
        }
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    } else {
        for reader in readers { let _ = reader.join(); }
        if let Some(writer) = input_writer { let _ = writer.join(); }
    }
    result
}

pub(crate) fn checked(directory: Option<&Path>, args: &[OsString], cancel: &Cancel) -> Result<Vec<u8>, Error> {
    let output = run(directory, args, cancel)?;
    success(output, &[0])
}

pub(crate) fn success(output: Output, accepted: &[i32]) -> Result<Vec<u8>, Error> {
    if accepted.contains(&output.code) { return Ok(output.stdout); }
    let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(Error::new(ErrorKind::Git, if message.is_empty() {
        format!("Git failed with exit code {}", output.code)
    } else { message }))
}

pub(crate) fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}
