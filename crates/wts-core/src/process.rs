use crate::{Error, ErrorKind};
use std::ffi::OsString;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
pub struct Cancel {
    state: Arc<(Mutex<bool>, Condvar)>,
}

impl Cancel {
    pub fn new() -> Self { Self::default() }

    pub fn cancel(&self) {
        let (lock, wake) = &*self.state;
        *lock.lock().unwrap_or_else(|error| error.into_inner()) = true;
        wake.notify_all();
    }

    pub fn is_cancelled(&self) -> bool {
        *self.state.0.lock().unwrap_or_else(|error| error.into_inner())
    }

    pub(crate) fn wait(&self, duration: Duration) {
        let (lock, wake) = &*self.state;
        let guard = lock.lock().unwrap_or_else(|error| error.into_inner());
        let _ = wake.wait_timeout_while(guard, duration, |cancelled| !*cancelled);
    }
}

pub(crate) struct Output {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub(crate) fn run(directory: Option<&Path>, args: &[OsString], cancel: &Cancel) -> Result<Output, Error> {
    if cancel.is_cancelled() {
        return Err(Error::new(ErrorKind::Cancelled, "Git scan cancelled"));
    }
    let mut command = Command::new("git");
    command.args(["-c", "core.fsmonitor=false"]);
    if let Some(directory) = directory { command.arg("-C").arg(directory); }
    command.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    for variable in ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_NAMESPACE", "GIT_PREFIX",
        "GIT_CONFIG", "GIT_CONFIG_PARAMETERS", "GIT_CONFIG_COUNT", "GIT_CEILING_DIRECTORIES",
        "GIT_IMPLICIT_WORK_TREE", "GIT_GRAFT_FILE", "GIT_NO_REPLACE_OBJECTS", "GIT_REPLACE_REF_BASE", "GIT_SHALLOW_FILE"] {
        command.env_remove(variable);
    }
    command.env("GIT_OPTIONAL_LOCKS", "0").env("GIT_TERMINAL_PROMPT", "0").env("LC_ALL", "C");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|error| Error::io("Could not start Git", error))?;
    let (sender, receiver) = mpsc::channel();
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
    let result = (|| {
        loop {
            if cancel.is_cancelled() { return Err(Error::new(ErrorKind::Cancelled, "Git scan cancelled")); }
            if Instant::now() >= deadline { return Err(Error::new(ErrorKind::Timeout, "Git command timed out after 10 seconds")); }
            while let Ok((index, bytes)) = receiver.try_recv() { buffers[index] = Some(bytes?); }
            if let Some(status) = child.try_wait().map_err(|error| Error::io("Could not wait for Git", error))? {
                if buffers.iter().all(Option::is_some) {
                    return Ok(Output { code: status.code().unwrap_or(-1),
                        stdout: buffers[0].take().unwrap(), stderr: buffers[1].take().unwrap() });
                }
            }
            cancel.wait(Duration::from_millis(20));
        }
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    } else {
        for reader in readers { let _ = reader.join(); }
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
