# PageWatchTUI

A lightweight, terminal-based website change tracker built with Rust. Monitor prices, availability, or specific content on any webpage with real-time notifications and automatic background checks.

## 🚀 Features

- **Dashboard View**: A professional table-based interface showing Name, Mode, Last Success, and current Value at a glance.
- **Background Checks**: Non-blocking asynchronous checks using `reqwest` and `tokio`. The UI remains fully responsive while fetching data.
- **Multiple Tracking Modes**: Specialized extraction logic for Prices, Availability, Keywords, and specific HTML sections.
- **Real-time Filtering**: Instantly search through your watch list by name or URL.
- **Bulk Actions**: Select multiple watches using `Space` to check or delete them in one go.
- **Desktop Notifications**: Get alerted via system notifications as soon as a change is detected.
- **Persistent Storage**: All configurations and check histories are automatically saved to `watches.json`.
- **Automatic Checking**: Configurable per-watch intervals for periodic background updates.
- **Detailed Analytics**: View full text, success rates, and a history of recent errors for every watch.

## 🛠 Tracking Modes

1.  **Full Page Text**: Tracks the entire text content of the page (normalized).
2.  **Price**: Uses regex and heuristics to extract currency values (e.g., $19.99, 50€).
3.  **Availability**: Monitors for stock status keywords like "In Stock" or "Sold Out".
4.  **Specific Keyword(s)**: Alert you when a specific list of keywords appear or disappear.
5.  **HTML Section**: Advanced mode using CSS selectors to track a specific element on the page.

## ⌨️ Keybindings

### Main List
- `q`: Quit application
- `n`: Add new watch
- `e`: Edit selected watch
- `c`: Trigger check for selected (or bulk selected) watches
- `d`: Delete selected (or bulk selected) watches
- `Space`: Toggle multi-selection for bulk actions
- `/`: Enter search mode to filter list
- `s`: Cycle sorting (Name, Last Checked, Success Rate)
- `p`: Toggle global pause/resume for automatic checks
- `Enter`: View full details of the selected watch
- `Up/Down`: Navigate the list

### In Popups / Forms
- `Enter`: Advance to next field or submit
- `Esc`: Cancel or go back
- `Up/Down`: Cycle through options (in Mode selection)

## 📦 Installation

Ensure you have Rust and Cargo installed.

```bash
# Clone the repository
git clone https://github.com/yourusername/PageWatchTUI
cd PageWatchTUI

# Build and run
cargo run --release
```

