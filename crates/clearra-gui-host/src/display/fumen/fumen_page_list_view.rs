#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FumenPageListView {
    page_count: usize,
    pages: Vec<String>,
}

impl FumenPageListView {
    pub fn from_payload(payload: Option<&str>) -> Self {
        let pages = payload
            .map(split_pages)
            .unwrap_or_default()
            .into_iter()
            .filter(|page| !page.trim().is_empty())
            .collect::<Vec<_>>();

        Self {
            page_count: pages.len(),
            pages,
        }
    }
}
impl FumenPageListView {
    pub const fn page_count(&self) -> usize {
        self.page_count
    }
}
impl FumenPageListView {
    pub fn pages(&self) -> &[String] {
        &self.pages
    }
}

fn split_pages(payload: &str) -> Vec<String> {
    if payload.contains("===PAGE===") {
        payload
            .split("===PAGE===")
            .map(|page| page.trim().to_owned())
            .collect()
    } else {
        vec![payload.trim().to_owned()]
    }
}
