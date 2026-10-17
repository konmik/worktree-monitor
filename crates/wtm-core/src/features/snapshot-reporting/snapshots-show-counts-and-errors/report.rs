use crate::{Head, Snapshot};
use std::fmt::{self, Write};

impl fmt::Display for Head {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Branch(name) => write!(f, "branch {name}"),
            Self::Detached(commit) => write!(f, "detached {commit}"),
            Self::Unborn(name) => write!(f, "unborn {name}"),
        }
    }
}

impl Snapshot {
    pub fn has_errors(&self) -> bool {
        self.config_error.is_some() || self.repos.iter().any(|repo| match &repo.worktrees {
            Err(_) => true,
            Ok(trees) => trees.iter().any(|tree| tree.result.is_err()),
        })
    }

    pub fn text(&self) -> String {
        let mut text = String::new();
        if let Some(error) = &self.config_error {
            let _ = writeln!(text, "Configuration error: {error}");
            if !self.repos.is_empty() {
                let _ = writeln!(text, "Using last good registrations.");
            }
        }
        if self.repos.is_empty() {
            let _ = writeln!(text, "No registered repositories.");
        }
        for repo in &self.repos {
            let _ = writeln!(text, "Repository: {}", repo.path.display());
            let _ = writeln!(text, "  Git directory: {}", repo.common_dir.display());
            match &repo.worktrees {
                Err(error) => { let _ = writeln!(text, "  ERROR: {error}"); }
                Ok(trees) if trees.is_empty() => { let _ = writeln!(text, "  No working trees (bare repository)."); }
                Ok(trees) => for tree in trees {
                    let _ = writeln!(text, "  {}", tree.path.display());
                    match &tree.result {
                        Ok(stats) => {
                            let _ = writeln!(text, "    {}  {}", stats.head, stats.counts);
                        }
                        Err(error) => { let _ = writeln!(text, "    ERROR: {error}"); }
                    }
                },
            }
        }
        text
    }
}
