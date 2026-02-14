use crate::models::{TrackingMode, Watch};
use anyhow::Result;
use chrono::Utc;
use scraper::{Html, Selector};
use regex::Regex;

pub async fn check_watch(watch: &mut Watch) -> Result<()> {
    watch.last_checked = Some(Utc::now());
    watch.total_checks += 1;

    let client = reqwest::Client::builder()
        .user_agent("PageWatchTUI/0.1.0")
        .build()?;

    let response = client.get(&watch.url).send().await?;
    
    if !response.status().is_success() {
        let error_msg = format!("HTTP {}", response.status());
        watch.add_error(error_msg.clone());
        return Err(anyhow::anyhow!(error_msg));
    }

    let html_content = response.text().await.map_err(|e| {
        let msg = format!("Failed to get text: {}", e);
        watch.add_error(msg.clone());
        e
    })?;
    let document = Html::parse_document(&html_content);

    // Use a Result to capture either the extracted value or an error message to log
    let result: std::result::Result<String, String> = match &watch.mode {
        TrackingMode::FullPage => {
            let selector = Selector::parse("body").unwrap();
            if let Some(body) = document.select(&selector).next() {
                Ok(body.text().collect::<Vec<_>>().join(" ").trim().to_string())
            } else {
                Ok(html_content.clone())
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

            let price_re = Regex::new(r"([$€£¥₹]\s?\d+([.,]\d+)?)|(\d+([.,]\d+)?\s?([$€£¥]|USD|EUR|GBP|JPY))").unwrap();
            
            if let Some(mat) = price_re.find(&text_to_search) {
                Ok(mat.as_str().trim().to_string())
            } else {
                Ok("Price not found".to_string())
            }
        }
        TrackingMode::Availability { in_stock_keywords, out_of_stock_keywords } => {
            let body_selector = Selector::parse("body").unwrap();
            let text = document.select(&body_selector)
                .next()
                .map(|el| el.text().collect::<Vec<_>>().join(" ").to_lowercase())
                .unwrap_or_else(|| html_content.to_lowercase());

            let in_stock = if in_stock_keywords.is_empty() {
                vec!["in stock", "available", "add to cart", "buy now"]
            } else {
                in_stock_keywords.iter().map(|s| s.as_str()).collect()
            };

            let out_of_stock = if out_of_stock_keywords.is_empty() {
                vec!["out of stock", "sold out", "currently unavailable", "not available"]
            } else {
                out_of_stock_keywords.iter().map(|s| s.as_str()).collect()
            };

            if in_stock.iter().any(|&k| text.contains(&k.to_lowercase())) {
                Ok("In Stock".to_string())
            } else if out_of_stock.iter().any(|&k| text.contains(&k.to_lowercase())) {
                Ok("Out of Stock".to_string())
            } else {
                Ok("Unknown Status".to_string())
            }
        }
        TrackingMode::Keyword { keywords } => {
            if keywords.is_empty() {
                Err("Keyword tracking mode requires at least one keyword.".to_string())
            } else {
                let body_selector = Selector::parse("body").unwrap();
                let text = document.select(&body_selector)
                    .next()
                    .map(|el| el.text().collect::<Vec<_>>().join(" ").to_lowercase())
                    .unwrap_or_else(|| html_content.to_lowercase());

                let mut missing_keywords = Vec::new();
                for keyword in keywords {
                    if !text.contains(&keyword.to_lowercase()) {
                        missing_keywords.push(keyword.clone());
                    }
                }

                if missing_keywords.is_empty() {
                    Ok(format!("All keywords found: {}", keywords.join(", ")))
                } else {
                    Err(format!("Missing keywords: {}", missing_keywords.join(", ")))
                }
            }
        }
        TrackingMode::HtmlSection { selector } => {
            if let Ok(sel) = Selector::parse(selector) {
                if let Some(element) = document.select(&sel).next() {
                    Ok(element.text().collect::<Vec<_>>().join(" ").trim().to_string())
                } else {
                    Err("Section not found".to_string())
                }
            } else {
                Err("Invalid CSS selector".to_string())
            }
        }
    };

    let extracted_value = match result {
        Ok(val) => val,
        Err(msg) => {
            watch.add_error(msg.clone());
            msg
        }
    };

    if let Some(old_val) = &watch.last_value {
        if old_val != &extracted_value {
            watch.previous_value = Some(old_val.clone());
            watch.has_unread_change = true;
        }
    } else {
        watch.has_unread_change = true;
    }

    watch.last_value = Some(extracted_value);
    watch.last_success = Some(Utc::now());
    watch.total_successes += 1;
    watch.last_error = None;

    Ok(())
}
