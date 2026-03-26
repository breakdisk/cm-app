use serde::Deserialize;

/// Query parameters for paginated list endpoints.
/// Usage: `Query(params): Query<ListParams>`
#[derive(Debug, Deserialize)]
pub struct ListParams {
    #[serde(default = "default_page")]
    pub page: u64,
    #[serde(default = "default_per_page")]
    pub per_page: u64,
    pub q: Option<String>,
    pub sort_by: Option<String>,
    pub sort_dir: Option<SortDir>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortDir { Asc, Desc }

impl ListParams {
    pub fn offset(&self) -> i64 {
        ((self.page.saturating_sub(1)) * self.clamp_per_page()) as i64
    }

    /// Cap per_page at 100 regardless of what the client sends.
    pub fn clamp_per_page(&self) -> u64 {
        self.per_page.clamp(1, 100)
    }
}

fn default_page()     -> u64 { 1 }
fn default_per_page() -> u64 { 20 }
