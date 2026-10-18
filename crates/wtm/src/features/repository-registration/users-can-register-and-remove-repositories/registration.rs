use fs2::FileExt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};
use wtm_ui::configuration::{encode, identity, parse, read, resolve};
use wtm_ui::{Cancel, Error, ErrorKind};

fn absolute(path: &Path) -> Result<PathBuf, Error> {
    let path = if path.is_absolute() { path.to_path_buf() } else {
        std::env::current_dir().map_err(|error| Error::io("Cannot resolve current directory", error))?.join(path)
    };
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {},
            Component::ParentDir => { result.pop(); },
            other => result.push(other.as_os_str()),
        }
    }
    Ok(result)
}

pub fn update(config: &Path, path: &Path, add: bool, cancel: &Cancel) -> Result<bool, Error> {
    let config = absolute(config)?;
    let parent = config.parent().ok_or_else(|| Error::config("Configuration has no parent directory"))?;
    fs::create_dir_all(parent).map_err(|error| Error::io("Cannot create configuration directory", error))?;
    let lock = OpenOptions::new().create(true).truncate(false).read(true).write(true)
        .open(config.with_extension("lock")).map_err(|error| Error::io("Cannot open configuration lock", error))?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match lock.try_lock_exclusive() {
            Ok(()) => break,
            Err(error) if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() => {
                if cancel.is_cancelled() { return Err(Error::new(ErrorKind::Cancelled, "Configuration update cancelled")); }
                if Instant::now() >= deadline { return Err(Error::new(ErrorKind::Timeout, "Configuration lock timed out after 5 seconds")); }
                cancel.wait(Duration::from_millis(25));
            }
            Err(error) => return Err(Error::io("Cannot lock configuration", error)),
        }
    }
    let mut repos = parse(&read(&config)?)?;
    let common = match resolve(path, cancel) {
        Ok(common) => common,
        Err(error) if add => return Err(error),
        Err(_) => absolute(path)?,
    };
    let before = repos.clone();
    if add {
        if !repos.iter().any(|repo| identity(repo) == identity(&common)) { repos.push(common); }
    } else {
        let root_git = common.join(".git");
        repos.retain(|repo| identity(repo) != identity(&common) && identity(repo) != identity(&root_git));
    }
    repos.sort_by_key(|path| identity(path));
    if repos == before { return Ok(false); }
    let bytes = encode(repos)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| Error::io("Cannot create temporary configuration", error))?;
    temporary.write_all(&bytes).map_err(|error| Error::io("Cannot write configuration", error))?;
    temporary.flush().map_err(|error| Error::io("Cannot flush configuration", error))?;
    temporary.as_file().sync_all().map_err(|error| Error::io("Cannot sync configuration", error))?;
    temporary.persist(&config).map_err(|error| Error::io("Cannot replace configuration", error.error))?;
    Ok(true)
}
