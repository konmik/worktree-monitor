use super::Opacity;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::Path;

impl Opacity {
    pub fn read(path: &Path) -> io::Result<Self> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error),
        };
        let mut bytes = Vec::new();
        file.take(1025).read_to_end(&mut bytes)?;
        if bytes.len() > 1024 { return Ok(Self::default()); }
        Ok(serde_json::from_slice::<u8>(&bytes).ok().and_then(Self::new).unwrap_or_default())
    }

    pub fn write(self, path: &Path) -> io::Result<()> {
        let bytes = serde_json::to_vec(&self.0).map_err(io::Error::other)?;
        let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&bytes)?;
        temporary.flush()?;
        temporary.as_file().sync_all()?;
        temporary.persist(path).map(|_| ()).map_err(|error| error.error)
    }
}
