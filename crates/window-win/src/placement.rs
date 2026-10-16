use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

pub const VERSION: u32 = 1;
pub const MAX_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self { left, top, right, bottom }
    }

    pub fn width(self) -> i64 { i64::from(self.right) - i64::from(self.left) }

    pub fn height(self) -> i64 { i64::from(self.bottom) - i64::from(self.top) }

    pub fn is_valid(self) -> bool { self.width() > 0 && self.height() > 0 }

    pub fn fits_within(self, outer: Self) -> bool {
        self.is_valid() && outer.is_valid() && self.left >= outer.left && self.top >= outer.top
            && self.right <= outer.right && self.bottom <= outer.bottom
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Monitor {
    pub identity: String,
    pub full_bounds: Rect,
    pub work_area: Rect,
}

impl Monitor {
    pub fn new(identity: impl Into<String>, full_bounds: Rect, work_area: Rect) -> Self {
        Self { identity: identity.into(), full_bounds, work_area }
    }

    fn is_valid(&self) -> bool {
        !self.identity.is_empty() && self.full_bounds.is_valid() && self.work_area.fits_within(self.full_bounds)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ShowState {
    Normal,
    Maximized,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    pub version: u32,
    pub normal: Rect,
    pub monitor: Monitor,
    pub state: ShowState,
}

impl Placement {
    pub fn new(normal: Rect, monitor: Monitor, state: ShowState) -> Self {
        Self { version: VERSION, normal, monitor, state }
    }

    pub fn is_valid(&self) -> bool {
        self.version == VERSION && self.normal.is_valid() && self.monitor.is_valid()
    }

    pub fn restore_for(&self, current: &Monitor) -> Option<RestorePlacement> {
        if !self.is_valid() || !current.is_valid() || self.monitor.identity != current.identity
            || self.monitor.full_bounds.width() != current.full_bounds.width()
            || self.monitor.full_bounds.height() != current.full_bounds.height()
            || self.normal.right <= current.work_area.left || self.normal.left >= current.work_area.right
            || self.normal.bottom <= current.work_area.top || self.normal.top >= current.work_area.bottom {
            return None;
        }
        Some(RestorePlacement { normal: self.normal, state: self.state })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RestorePlacement {
    pub normal: Rect,
    pub state: ShowState,
}

pub fn read(path: &Path) -> io::Result<Option<Placement>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BYTES {
        return Ok(None);
    }
    let placement = match serde_json::from_slice::<Placement>(&bytes) {
        Ok(placement) if placement.is_valid() => placement,
        _ => return Ok(None),
    };
    Ok(Some(placement))
}

pub fn write(path: &Path, placement: &Placement) -> io::Result<()> {
    if !placement.is_valid() {
        return Err(invalid("Invalid window placement"));
    }
    let bytes = serde_json::to_vec_pretty(placement).map_err(|error| io::Error::other(format!("Invalid window placement: {error}")))?;
    if bytes.len() > MAX_BYTES {
        return Err(invalid("Window placement exceeds 1 MiB"));
    }
    let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(&bytes)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map(|_| ()).map_err(|error| error.error)
}

fn invalid(message: impl Into<String>) -> io::Error { io::Error::new(io::ErrorKind::InvalidData, message.into()) }
