use serde::Serialize;

/// A distilled document: the LLM-facing digest of one command output / config.
#[derive(Serialize)]
pub struct Doc {
    /// What the input was detected as ("bgp-summary", "route-table", ...).
    pub kind: String,
    /// One-line context (router-id, counts, hostname...).
    pub headline: String,
    /// Things that likely need attention. Always rendered first.
    pub alerts: Vec<String>,
    /// Aggregate facts that replace the bulk of the raw output.
    pub summary: Vec<String>,
    /// Compact tab-separated tables (notable rows only unless --full).
    pub tables: Vec<Table>,
    /// Caveats and explicit elision notes, so the model knows what was omitted.
    pub notes: Vec<String>,
    /// Raw pass-through body (generic mode only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Serialize)]
pub struct Table {
    pub title: String,
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// Human-readable note about rows deliberately left out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub omitted: Option<String>,
}

impl Doc {
    pub fn new(kind: &str, headline: String) -> Self {
        Doc {
            kind: kind.to_string(),
            headline,
            alerts: Vec::new(),
            summary: Vec::new(),
            tables: Vec::new(),
            notes: Vec::new(),
            body: None,
        }
    }
}
