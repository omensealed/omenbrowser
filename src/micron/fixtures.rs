use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::micron::parser::{parse_micron, TextStyle};
use crate::micron::render::{render_document, ControlRef, RenderedRow};

pub const REGRESSION_WIDTHS: &[usize] = &[40, 60, 71, 80];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MicronFixtureReport {
    pub path: PathBuf,
    pub width: usize,
    pub rows: usize,
    pub links: usize,
    pub controls: usize,
    pub styled_runs: usize,
    pub suspected_style_spill: Vec<String>,
}

pub fn render_fixture_report(
    path: impl AsRef<Path>,
    width: usize,
) -> std::io::Result<MicronFixtureReport> {
    let path = path.as_ref();
    let markup = std::fs::read_to_string(path)?;
    Ok(render_markup_report(path.to_path_buf(), &markup, width))
}

pub fn render_markup_report(path: PathBuf, markup: &str, width: usize) -> MicronFixtureReport {
    let document = parse_micron(markup);
    let rows = render_document(&document, width);
    let visible_text = rows
        .iter()
        .map(RenderedRow::text)
        .collect::<Vec<_>>()
        .join("\n");
    let links = rows
        .iter()
        .flat_map(|row| row.cells.iter())
        .filter(|cell| cell.link.is_some())
        .count();
    let controls = rows
        .iter()
        .flat_map(|row| row.cells.iter())
        .filter(|cell| cell.control.is_some())
        .count();
    let styled_runs = rows.iter().map(count_style_runs).sum();

    MicronFixtureReport {
        path,
        width,
        rows: rows.len(),
        links,
        controls,
        styled_runs,
        suspected_style_spill: suspected_style_spill_lines(&visible_text),
    }
}

fn count_style_runs(row: &RenderedRow) -> usize {
    if row.cells.is_empty() {
        return 0;
    }

    let mut runs = 1;
    let mut previous_style = &row.cells[0].style;
    let mut previous_link = row.cells[0].link.as_ref();
    let mut previous_control = row.cells[0].control.as_ref();
    for cell in row.cells.iter().skip(1) {
        if &cell.style != previous_style
            || cell.link.as_ref() != previous_link
            || !same_control_capture_key(cell.control.as_ref(), previous_control)
        {
            runs += 1;
            previous_style = &cell.style;
            previous_link = cell.link.as_ref();
            previous_control = cell.control.as_ref();
        }
    }
    runs
}

fn same_control_capture_key(left: Option<&ControlRef>, right: Option<&ControlRef>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => {
            left.name == right.name
                && left.kind == right.kind
                && left.length == right.length
                && left.masked == right.masked
        }
        (None, None) => true,
        _ => false,
    }
}

fn suspected_style_spill_lines(text: &str) -> Vec<String> {
    text.lines()
        .filter(|line| {
            line.contains("`F")
                || line.contains("`B")
                || line.contains("`T")
                || line.contains("`g")
                || line.contains("`!")
                || line.contains("`_")
        })
        .map(str::to_string)
        .collect()
}

#[allow(dead_code)]
fn _assert_style_is_serializable(_: &TextStyle) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/micron")
    }

    fn micron_fixture_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();
        collect_mu_paths(&fixture_root(), &mut paths);
        paths.sort();
        paths
    }

    fn collect_mu_paths(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_mu_paths(&path, out);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("mu") {
                out.push(path);
            }
        }
    }

    #[test]
    fn micron_fixture_corpus_renders_at_python_relevant_widths() {
        let paths = micron_fixture_paths();
        assert!(!paths.is_empty(), "expected at least one Micron fixture");

        for path in paths {
            for width in REGRESSION_WIDTHS {
                let report = render_fixture_report(&path, *width).expect("fixture renders");
                assert!(report.rows > 0, "{} rendered no rows", path.display());
            }
        }
    }

    #[test]
    fn fixture_reports_expose_links_controls_styles_and_spill() {
        let path = fixture_root().join("python_mock_index.mu");
        let report = render_fixture_report(&path, 80).expect("fixture report");

        assert!(report.links > 0);
        assert!(report.styled_runs > 1);
        assert!(report.suspected_style_spill.is_empty());
    }
}
