use crate::models::{TrackingMode, Watch};
use anyhow::Result;
use chrono::Utc;
use scraper::{Html, Selector};

pub async fn check_watch(watch: &mut Watch) -> Result<()> {
    watch.last_checked = Some(Utc::now());

    let client = reqwest::Client::builder()
        .user_agent("PageWatchTUI/0.1.0")
        .build()?;

    let response = client.get(&watch.url).send().await?;
    
    if !response.status().is_success() {
        let error_msg = format!("HTTP {}", response.status());
        watch.last_error = Some(error_msg.clone());
        return Err(anyhow::anyhow!(error_msg));
    }

    let html_content = response.text().await?;
    let document = Html::parse_document(&html_content);

    let extracted_value = match &watch.mode {
        TrackingMode::FullPage => {
            // Simple text extraction: get all text nodes from body
            let selector = Selector::parse("body").unwrap();
            if let Some(body) = document.select(&selector).next() {
                body.text().collect::<Vec<_>>().join(" ").trim().to_string()
            } else {
                html_content // Fallback to raw HTML if no body
            }
        }
        _ => {
            "Extraction mode not yet implemented".to_string()
        }
    };

    watch.last_value = Some(extracted_value);
    watch.last_success = Some(Utc::now());
    watch.last_error = None;

    Ok(())
}
