use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TrackingMode {
    /// Track the full text content of the page (normalized)
    FullPage,
    /// Track a specific price pattern (regex + heuristics)
    Price {
        /// Optional specific selector to narrow down price search
        selector: Option<String>,
    },
    /// Track availability based on keywords (e.g., "In Stock", "Sold Out")
    Availability {
        /// Custom keywords for "In Stock" (defaults will be used if empty)
        in_stock_keywords: Vec<String>,
        /// Custom keywords for "Out of Stock" (defaults will be used if empty)
        out_of_stock_keywords: Vec<String>,
    },
    /// Track presence/absence of specific keywords
    Keyword {
        keywords: Vec<String>,
    },
    /// Track a specific HTML section by CSS selector
    HtmlSection {
        selector: String,
    },
}

impl Default for TrackingMode {
    fn default() -> Self {
        Self::FullPage
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Watch {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    pub mode: TrackingMode,
    pub interval_seconds: u64,
    
    pub last_checked: Option<DateTime<Utc>>,
    pub last_success: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    
    /// The last value we successfully extracted, used for comparison
    pub last_value: Option<String>,
    /// The value from the check before the last one
    #[serde(default)]
    pub previous_value: Option<String>,
    
    /// Whether there is a change that hasn't been "seen" by the user yet
    #[serde(default)]
    pub has_unread_change: bool,

    #[serde(default)]
    pub total_checks: u64,
    #[serde(default)]
    pub total_successes: u64,

    /// History of the most recent error messages
    #[serde(default)]
    pub error_log: Vec<(DateTime<Utc>, String)>,
}

impl Watch {
    pub fn new(name: String, url: String, mode: TrackingMode) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            url,
            mode,
            interval_seconds: 3600, // Default to 1 hour
            last_checked: None,
            last_success: None,
            last_error: None,
            last_value: None,
            previous_value: None,
            has_unread_change: false,
            total_checks: 0,
            total_successes: 0,
            error_log: Vec::new(),
        }
    }

    pub fn add_error(&mut self, error: String) {
        self.last_error = Some(error.clone());
        self.error_log.push((Utc::now(), error));
        // Keep only the last 10 errors
        if self.error_log.len() > 10 {
            self.error_log.remove(0);
        }
    }
}
