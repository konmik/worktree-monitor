use crate::process::{self, args};
use crate::render_buffer::{CellFlags, Color, RenderBuffer, display_width};
use crate::rendering::{render_name, GREEN, TEAL};
use crate::{Cancel, Error, ErrorKind};
use std::path::Path;

pub(crate) fn remote_tips(tree: &Path, cancel: &Cancel) -> Result<Vec<String>, Error> {
    let output = process::checked(Some(tree), &args(&["for-each-ref", "--format=%(objectname)", "refs/remotes/"]), cancel)?;
    let text = std::str::from_utf8(&output).map_err(|_| Error::new(ErrorKind::InvalidOutput, "Git remote references are not valid UTF-8"))?;
    let mut tips: Vec<_> = text.lines().map(str::to_owned).collect();
    tips.sort();
    tips.dedup();
    Ok(tips)
}

fn count(tree: &Path, commit: &str, excluded: impl Iterator<Item = impl AsRef<str>>, cancel: &Cancel) -> Result<u64, Error> {
    let mut input = format!("{commit}\n");
    for reference in excluded { input.push_str(&format!("^{}\n", reference.as_ref())); }
    let output = process::run_with_input(Some(tree), &args(&["rev-list", "--count", "--stdin"]), input.as_bytes(), cancel)?;
    let output = process::success(output, &[0])?;
    std::str::from_utf8(&output).ok().and_then(|text| text.trim().parse().ok())
        .ok_or_else(|| Error::new(ErrorKind::InvalidOutput, "Git commit count is invalid"))
}

pub(crate) fn pending(tree: &Path, commit: &str, base: Option<&str>, remotes: &[String], cancel: &Cancel) -> Result<u64, Error> {
    let unpushed = count(tree, commit, remotes.iter(), cancel)?;
    let Some(base) = base else { return Ok(unpushed); };
    let unmerged = count(tree, commit, std::iter::once(base), cancel)?;
    let overlap = count(tree, commit, remotes.iter().map(String::as_str).chain(std::iter::once(base)), cancel)?;
    Ok(unpushed + (unmerged - overlap))
}

pub(crate) fn indicator(count: u64) -> Option<String> {
    (count > 0).then(|| format!("[^{count}]"))
}

pub(crate) fn render_heading(value: &str, count: u64, columns: usize, refreshing: bool, dragging: bool, color: Color) -> RenderBuffer {
    let Some(indicator) = indicator(count).filter(|text| display_width(text) <= columns) else {
        return render_name(value, columns, false, refreshing, color);
    };
    let column = columns - display_width(&indicator);
    let name = render_name(value, column.saturating_sub(1), false, refreshing, color);
    let mut row = RenderBuffer::new(columns, 1, crate::rendering::MUTED);
    row.blit(0, 0, &name);
    row.write_styled(column, 0, &indicator, if dragging { GREEN } else { TEAL }, CellFlags { stressed: true, ..Default::default() });
    row
}

#[cfg(test)]
#[path = "rendering.test.rs"]
mod tests;
