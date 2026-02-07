use crate::models::{TrackingMode, Watch};
use anyhow::Result;
use chrono::Utc;
use scraper::{Html, Selector};
use regex::Regex;

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
            let selector = Selector::parse("body").unwrap();
            if let Some(body) = document.select(&selector).next() {
                body.text().collect::<Vec<_>>().join(" ").trim().to_string()
            } else {
                html_content
            }
        }
        TrackingMode::Price { selector } => {
            let text_to_search = if let Some(sel_str) = selector {
                if let Ok(sel) = Selector::parse(sel_str) {
                    document.select(&sel)
                        .next()
                        .map(|el| el.text().collect::<Vec<_>>().join(" "))
                        .unwrap_or_default()
                } else {
                    String::new()
                }
            } else {
                let body_selector = Selector::parse("body").unwrap();
                document.select(&body_selector)
                    .next()
                    .map(|el| el.text().collect::<Vec<_>>().join(" "))
                    .unwrap_or_else(|| html_content.clone())
            };

            // Regex for price: currency symbol followed by numbers or vice versa
            // Matches $10.99, £ 5, 100.00€, 50 USD, etc.
            let price_re = Regex::new(r"([$€£¥₹]\s?\d+([.,]\d+)?)|(\d+([.,]\d+)?\s?([$€£¥]|USD|EUR|GBP|JPY))").unwrap();
            
            if let Some(mat) = price_re.find(&text_to_search) {
                mat.as_str().trim().to_string()
            } else {
                "Price not found".to_string()
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
