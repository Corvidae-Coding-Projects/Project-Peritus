//! Exact native-path and patch continuations packed by canonical JSON bytes.

use super::*;

pub(super) fn render(
    value: &GitDiffObservation,
    budget: u64,
) -> Result<RenderedOutput, GitToolError> {
    let budget = usize::try_from(budget).unwrap_or(usize::MAX).min(64 * 1024);
    let mut page = Page {
        entries: Vec::new(),
        next_entry: value.cursor().entry_offset,
        next_path_byte: value.cursor().path_byte_offset,
        patch_bytes: 0,
    };
    let initial = page.encode(value)?;
    if initial.canonical_bytes().len() > budget {
        return Err(protocol_error());
    }
    // Reserve some transfer space for patch progress. Actual escaped/base64 encoding is measured.
    let path_budget =
        initial.canonical_bytes().len() + (budget - initial.canonical_bytes().len()) / 2;
    for entry in value.entries() {
        let offset = usize::try_from(page.next_path_byte).map_err(|_| protocol_error())?;
        let available = entry.path_bytes().len().checked_sub(offset).ok_or_else(protocol_error)?;
        let mut count = available.min(4096);
        loop {
            let row = path_row(entry, page.next_entry, offset, count)?;
            let next_entry = page.next_entry;
            let next_path_byte = page.next_path_byte;
            page.entries.push(row);
            if count == available {
                page.next_entry += 1;
                page.next_path_byte = 0;
            } else {
                page.next_path_byte += count as u64;
            }
            if page.encode(value)?.canonical_bytes().len()
                <= if page.entries.len() == 1 { budget.saturating_sub(4) } else { path_budget }
            {
                break;
            }
            page.entries.pop();
            page.next_entry = next_entry;
            page.next_path_byte = next_path_byte;
            if count <= 1 {
                count = 0;
                break;
            }
            count /= 2;
        }
        if count == 0 || page.next_path_byte != 0 {
            break;
        }
    }
    let mut low = 0;
    let mut high = value.patch().len();
    while low < high {
        let count = low + (high - low).div_ceil(2);
        page.patch_bytes = count;
        if page.encode(value)?.canonical_bytes().len() <= budget {
            low = count;
        } else {
            high = count - 1;
        }
    }
    page.patch_bytes = low;
    if (page.entries.is_empty() && value.cursor().entry_offset < value.total_entries())
        || (low == 0 && value.cursor().patch_offset < value.total_patch_bytes())
    {
        return Err(protocol_error());
    }
    let truncated = page.next_entry < value.total_entries()
        || value.cursor().patch_offset + (low as u64) < value.total_patch_bytes();
    finish(
        page.encode(value)?,
        format!(
            "Git diff page from {} paths and {} patch bytes; continue using the returned offsets and digest.",
            value.total_entries(),
            value.total_patch_bytes()
        ),
        truncated,
    )
}

struct Page {
    entries: Vec<BoundedJson>,
    next_entry: u64,
    next_path_byte: u64,
    patch_bytes: usize,
}

impl Page {
    fn encode(&self, value: &GitDiffObservation) -> Result<BoundedJson, GitToolError> {
        let next_entry = (self.next_entry < value.total_entries()).then_some(self.next_entry);
        let patch_end = value.cursor().patch_offset + self.patch_bytes as u64;
        let next_patch = (patch_end < value.total_patch_bytes()).then_some(patch_end);
        object(vec![
            ("base", string(value.base().to_string())),
            ("target", string(value.target().to_string())),
            ("digest", string(digest_hex(value.digest()))),
            ("entries", array(self.entries.clone())),
            ("entry_count", exact_number(value.total_entries())),
            ("entry_offset", exact_number(value.cursor().entry_offset)),
            ("next_entry_offset", optional_number(next_entry)),
            ("next_path_byte_offset", exact_number(self.next_path_byte)),
            ("patch_base64", string(STANDARD.encode(&value.patch()[..self.patch_bytes]))),
            ("patch_bytes", exact_number(value.total_patch_bytes())),
            ("patch_offset", exact_number(value.cursor().patch_offset)),
            ("next_patch_offset", optional_number(next_patch)),
            ("truncated", Ok(BoundedJson::boolean(next_entry.is_some() || next_patch.is_some()))),
        ])
    }
}

fn path_row(
    entry: &peritus_git::DiffEntry,
    index: u64,
    offset: usize,
    count: usize,
) -> Result<BoundedJson, GitToolError> {
    let end = offset + count;
    object(vec![
        ("change", string(change_name(entry.change()).to_owned())),
        ("entry_index", exact_number(index)),
        (
            "path",
            string(
                entry
                    .path()
                    .chars()
                    .take(128)
                    .map(|c| if c.is_control() { '�' } else { c })
                    .collect(),
            ),
        ),
        ("path_bytes_base64", string(STANDARD.encode(&entry.path_bytes()[offset..end]))),
        ("path_bytes", exact_number(entry.path_bytes().len() as u64)),
        ("path_byte_offset", exact_number(offset as u64)),
        (
            "next_path_byte_offset",
            optional_number((end < entry.path_bytes().len()).then_some(end as u64)),
        ),
    ])
}

fn exact_number(value: u64) -> Result<BoundedJson, GitToolError> {
    i64::try_from(value).map(integer).map_err(|_| protocol_error())
}

fn optional_number(value: Option<u64>) -> Result<BoundedJson, GitToolError> {
    value.map_or_else(|| Ok(BoundedJson::null()), exact_number)
}
