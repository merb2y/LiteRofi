//! Usage history persistence mirroring `source/history.c`.

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

/// Controls whether/how history entries are recorded (`config.disable_history`,
/// `config.max_history_size`, `config.ignored_prefixes`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryConfig {
    pub disabled: bool,
    pub max_size: usize,
    pub ignored_prefixes: String,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            disabled: false,
            max_size: 25,
            ignored_prefixes: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HistoryElement {
    index: i64,
    name: String,
}

fn read_elements(path: &Path) -> io::Result<Vec<HistoryElement>> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let reader = BufReader::new(file);
    let mut elements = Vec::new();
    for line_res in reader.lines() {
        let line = line_res?;
        if line.is_empty() {
            continue;
        }
        let Some((num_part, name_part)) = line.split_once(' ') else {
            continue;
        };
        if name_part.is_empty() {
            continue;
        }
        if let Ok(index) = num_part.trim().parse::<i64>() {
            elements.push(HistoryElement {
                index,
                name: name_part.to_string(),
            });
        }
    }
    Ok(elements)
}

fn write_elements(path: &Path, mut list: Vec<HistoryElement>, max_size: usize) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let mut file = File::create(path)?;
    if list.is_empty() {
        return Ok(());
    }
    // Sort descending by hit count/rank, matching `b->index - a->index` stably.
    list.sort_by_key(|a| std::cmp::Reverse(a.index));
    let min_value = list[list.len() - 1].index;
    let take_len = list.len().min(max_size);
    for elem in list.into_iter().take(take_len) {
        writeln!(file, "{} {}", elem.index - min_value, elem.name)?;
    }
    Ok(())
}

/// Returns recorded history entries in priority order (`history_get_list`).
pub fn history_get_list(path: &Path, disabled: bool) -> io::Result<Vec<String>> {
    if disabled {
        return Ok(Vec::new());
    }
    let elems = read_elements(path)?;
    Ok(elems.into_iter().map(|e| e.name).collect())
}

/// Records or increments `entry` in the history file (`history_set`).
pub fn history_set(path: &Path, entry: &str, cfg: &HistoryConfig) -> io::Result<()> {
    if cfg.disabled {
        return Ok(());
    }
    for prefix in cfg.ignored_prefixes.split(';') {
        let trimmed = prefix.trim_start();
        if !trimmed.is_empty() && entry.starts_with(trimmed) {
            return Ok(());
        }
    }

    let mut list = read_elements(path)?;
    if let Some(existing) = list.iter_mut().find(|e| e.name == entry) {
        existing.index += 1;
    } else {
        list.push(HistoryElement {
            index: 1,
            name: entry.to_string(),
        });
    }
    write_elements(path, list, cfg.max_size)
}

/// Removes `entry` from the history file (`history_remove`).
pub fn history_remove(path: &Path, entry: &str, disabled: bool, max_size: usize) -> io::Result<()> {
    if disabled {
        return Ok(());
    }
    let mut list = read_elements(path)?;
    if let Some(pos) = list.iter().position(|e| e.name == entry) {
        list.swap_remove(pos);
        write_elements(path, list, max_size)?;
    }
    Ok(())
}
