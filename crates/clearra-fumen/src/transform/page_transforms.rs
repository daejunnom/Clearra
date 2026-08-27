//! Legacy helpers for Clearra metadata stored in Fumen comment pages.
//!
//! These helpers do not transform decoded v115 geometry. Product document
//! commands use `ActualFumenDocumentTransform` instead.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CombineTransform;

impl CombineTransform {
    pub fn combine_pages(chunks: &[Vec<String>]) -> Vec<String> {
        chunks.iter().flatten().cloned().collect()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SplitTransform;

impl SplitTransform {
    pub fn split_pages(pages: &[String]) -> Vec<Vec<String>> {
        pages.iter().cloned().map(|page| vec![page]).collect()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MirrorTransform;

impl MirrorTransform {
    pub fn transform_page_comment(page: &str) -> String {
        transform_marker_field(page, "mirror_policy", "field-mirror")
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GrayoutTransform;

impl GrayoutTransform {
    pub fn transform_page_comment(page: &str) -> String {
        transform_marker_field(page, "grayout_normalized", "true")
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PageShiftTransform;

impl PageShiftTransform {
    pub fn shift_pages(pages: &[String], offset: isize) -> Vec<String> {
        if pages.is_empty() {
            return Vec::new();
        }
        let normalized = offset.rem_euclid(pages.len() as isize) as usize;
        pages
            .iter()
            .cycle()
            .skip(normalized)
            .take(pages.len())
            .cloned()
            .collect()
    }
}

fn transform_marker_field(page: &str, key: &str, value: &str) -> String {
    let marker = format!("{key}=");
    let mut replaced = false;
    let mut lines = page
        .lines()
        .map(|line| {
            if line.starts_with(&marker) {
                replaced = true;
                format!("{key}={value}")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>();
    if !replaced {
        lines.push(format!("{key}={value}"));
    }
    lines.join("\n")
}
