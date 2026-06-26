use crate::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use shared::StatusState;

/// Main drawing entry point. Sets up the restructured dashboard layout.
pub fn draw(f: &mut Frame, app: &App) {
    // 1. Vertical split: Top Bar (3) and the Content Area
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Top Bar
            Constraint::Min(10),   // Content Area
        ])
        .split(f.size());

    // 2. Draw the Top Bar / Connection Error Bar
    draw_top_bar(f, app, chunks[0]);

    // 3. Horizontal split in Content Area: Main Chunk (60% width) and Logs Panel (40% width)
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40), // Main Chunk (Left)
            Constraint::Percentage(60), // Logs Console (Right)
        ])
        .split(chunks[1]);

    // 4. Vertical split in Main Chunk (Left): Package Details (Top 10) and Package List (Bottom remaining)
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6), // Package Details
            Constraint::Min(5),    // Package List
        ])
        .split(content_chunks[0]);

    draw_detail_card(f, app, main_chunks[0]);
    draw_package_list(f, app, main_chunks[1]);

    // 5. Draw Logs on the right panel
    draw_log_console(f, app, content_chunks[1]);
}

/// Renders the status bar showing daemon state, uptime, and next sync countdown.
fn draw_top_bar(f: &mut Frame, app: &App, area: Rect) {
    if let Some(ref err) = app.error_message {
        let error_banner = Paragraph::new(format!(
            "  ERROR: {}  (Press Esc/q to exit TUI dashboard)",
            err
        ))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Connection Error "),
        )
        .style(
            Style::default()
                .bg(Color::Red)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );
        f.render_widget(error_banner, area);
    } else {
        let state_color = match app.daemon_state.as_str() {
            "Idle" => Color::Green,
            "Syncing" => Color::Yellow,
            "Building" => Color::Cyan,
            _ => Color::DarkGray,
        };

        let uptime_str = format!(
            "{:02}:{:02}:{:02}",
            app.uptime_secs / 3600,
            (app.uptime_secs / 60) % 60,
            app.uptime_secs % 60
        );

        let countdown_str = format!(
            "{:02}:{:02}:{:02}",
            app.countdown_secs / 3600,
            (app.countdown_secs / 60) % 60,
            app.countdown_secs % 60
        );

        let text = vec![Line::from(vec![
            Span::raw(" Daemon State: "),
            Span::styled(
                &app.daemon_state,
                Style::default()
                    .fg(state_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("   │   ", Style::default().fg(Color::DarkGray)),
            Span::raw("Daemon Uptime: "),
            Span::styled(
                uptime_str,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("   │   ", Style::default().fg(Color::DarkGray)),
            Span::raw("Next Sync In: "),
            Span::styled(
                countdown_str,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("   │   ", Style::default().fg(Color::DarkGray)),
            Span::raw("Config Source: "),
            Span::styled(
                home::home_dir()
                    .map(|p| {
                        p.join(".config/auror/config.toml")
                            .to_string_lossy()
                            .into_owned()
                    })
                    .unwrap_or_else(|| "~/.config/auror/config.toml".to_string()),
                Style::default().fg(Color::Gray),
            ),
        ])];

        let top_bar = Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Auror Daemon Monitor "),
        );
        f.render_widget(top_bar, area);
    }
}

/// Renders the list of monitored AUR packages and their statuses.
fn draw_package_list(f: &mut Frame, app: &App, area: Rect) {
    let mut state = ListState::default();
    state.select(Some(app.selected_index));

    let items: Vec<ListItem> = app
        .packages
        .iter()
        .enumerate()
        .map(|(idx, pkg)| {
            let status_style = match &pkg.status {
                StatusState::UpToDate => Style::default().fg(Color::Green),
                StatusState::Outdated => Style::default().fg(Color::Yellow),
                StatusState::Checking => Style::default().fg(Color::Cyan),
                StatusState::Building => Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
                StatusState::Failed(_) => Style::default().fg(Color::Red),
            };

            let status_text = match &pkg.status {
                StatusState::UpToDate => "Up to date",
                StatusState::Outdated => "Outdated",
                StatusState::Checking => "Checking",
                StatusState::Building => "Building",
                StatusState::Failed(_) => "Failed",
            };

            let is_selected = idx == app.selected_index;
            let item_style = if is_selected {
                Style::default()
                    .bg(Color::Rgb(45, 45, 45))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let prefix = if is_selected { "> " } else { "  " };

            // Format line nicely: prefix name status
            let line = Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Yellow)),
                Span::styled(
                    format!("{:<28}", pkg.name),
                    Style::default().fg(Color::White),
                ),
                Span::styled(status_text, status_style),
            ]);

            ListItem::new(line).style(item_style)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Local AUR Repositories "),
        )
        .highlight_style(Style::default().bg(Color::Rgb(45, 45, 45)));

    f.render_stateful_widget(list, area, &mut state);
}

/// Renders detail metadata card of the selected package.
fn draw_detail_card(f: &mut Frame, app: &App, area: Rect) {
    let detail_widget = if let Some(pkg) = app.selected_package() {
        let path = home::home_dir()
            .map(|p| {
                p.join("git/aur")
                    .join(&pkg.name)
                    .to_string_lossy()
                    .into_owned()
            })
            .unwrap_or_else(|| format!("~/git/aur/{}", pkg.name));

        let mut text = vec![
            Line::from(vec![
                Span::styled("Name:          ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    &pkg.name,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("Current Ver:   ", Style::default().fg(Color::DarkGray)),
                Span::styled(&pkg.current_version, Style::default().fg(Color::Green)),
            ]),
            Line::from(vec![
                Span::styled("Upstream Ver:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(&pkg.upstream_version, Style::default().fg(Color::Yellow)),
            ]),
            Line::from(vec![
                Span::styled("Local Dir:     ", Style::default().fg(Color::DarkGray)),
                Span::styled(path, Style::default().fg(Color::Blue)),
            ]),
            Line::from(vec![
                Span::styled("Last Checked:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(&pkg.last_checked, Style::default().fg(Color::Magenta)),
            ]),
            Line::from(""),
        ];

        if let StatusState::Failed(ref err) = pkg.status {
            text.push(Line::from(Span::styled(
                "Status Message:",
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::UNDERLINED),
            )));
            for line in err.lines() {
                text.push(Line::from(Span::styled(
                    line,
                    Style::default().fg(Color::Red),
                )));
            }
        }

        Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Package Info Summary "),
            )
            .wrap(Wrap { trim: false })
    } else {
        Paragraph::new("No monitored package selected.\nUse Up/Down or j/k to navigate packages.")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Package Info Summary "),
            )
    };

    f.render_widget(detail_widget, area);
}

/// Renders the bottom logs and activity feed console.
fn draw_log_console(f: &mut Frame, app: &App, area: Rect) {
    let logs_height = (area.height as usize).saturating_sub(2);
    let total_logs = app.logs.len();

    let start_idx = total_logs.saturating_sub(logs_height);

    let log_lines: Vec<Line> = app.logs[start_idx..]
        .iter()
        .map(|line| style_log_line(line))
        .collect();

    let logs_widget = Paragraph::new(log_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Compilation Logs & Live Activity Stream "),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(logs_widget, area);
}

/// Applies conditional rich styling to a log line.
fn style_log_line(line: &str) -> Line<'_> {
    // 1. Primary makepkg header starting with ==>
    if let Some(text) = line.strip_prefix("==> ") {
        // Classify the makepkg header type to color it appropriately
        let header_color = if text.contains("Finished making")
            || text.contains("Cleaning up")
            || text.contains("Leaving fakeroot")
        {
            Color::Green
        } else if text.contains("Starting")
            || text.contains("Making package")
            || text.contains("Entering fakeroot")
        {
            Color::Blue
        } else if text.contains("Checking")
            || text.contains("Retrieving")
            || text.contains("Validating")
            || text.contains("Extracting")
        {
            Color::Cyan
        } else if text.contains("Error") || text.contains("failed") || text.contains("Failed") {
            Color::Red
        } else if text.contains("Warning") || text.contains("warning") {
            Color::Yellow
        } else {
            Color::Green // default makepkg color
        };

        Line::from(vec![
            Span::styled(
                "==> ",
                Style::default()
                    .fg(header_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                text,
                Style::default()
                    .fg(header_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    }
    // 2. Sub-header starting with "  -> "
    else if let Some(text) = line.strip_prefix("  -> ") {
        let sub_color = if text.contains("Found") || text.contains("Passed") {
            Color::Green
        } else if text.contains("Removing")
            || text.contains("Purging")
            || text.contains("Stripping")
            || text.contains("Compressing")
        {
            Color::DarkGray
        } else if text.contains("Error") || text.contains("failed") || text.contains("Failed") {
            Color::Red
        } else if text.contains("Warning") || text.contains("warning") {
            Color::Yellow
        } else {
            Color::Cyan // default makepkg sub-header color
        };

        Line::from(vec![
            Span::styled(
                "  -> ",
                Style::default().fg(sub_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(text, Style::default().fg(sub_color)),
        ])
    }
    // 3. Daemon package log like [pkg_name] message
    else if line.starts_with('[') {
        if let Some(close_idx) = line.find(']') {
            let pkg_name = &line[1..close_idx];
            let rest = &line[close_idx + 1..];

            let (space, rest_text) = if let Some(stripped) = rest.strip_prefix(' ') {
                (" ", stripped)
            } else {
                ("", rest)
            };

            // Differentiate tag color and rest color based on log message type
            let (tag_color, rest_color) = if rest_text.contains("Error")
                || rest_text.contains("failed")
                || rest_text.contains("Failed")
                || rest_text.contains("Aborting")
            {
                (Color::Red, Color::Red)
            } else if rest_text.contains("warning") || rest_text.contains("Warning") {
                (Color::Yellow, Color::Yellow)
            } else if rest_text.contains("succeeded")
                || rest_text.contains("completed")
                || rest_text.contains("Up-to-date")
            {
                (Color::Green, Color::Green)
            } else if rest_text.contains("Checking")
                || rest_text.contains("Running")
                || rest_text.contains("Starting")
            {
                (Color::Blue, Color::Gray)
            } else {
                (Color::Magenta, Color::Gray) // Default package logging colors
            };

            let mut spans = vec![
                Span::styled("[", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    pkg_name,
                    Style::default().fg(tag_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled("]", Style::default().fg(Color::DarkGray)),
            ];
            if !space.is_empty() {
                spans.push(Span::styled(space, Style::default()));
            }

            spans.push(Span::styled(rest_text, Style::default().fg(rest_color)));
            Line::from(spans)
        } else {
            style_fallback(line)
        }
    } else {
        style_fallback(line)
    }
}

/// Fallback styling logic for lines without distinct header tags.
fn style_fallback(line: &str) -> Line<'_> {
    if line.contains("Error") || line.contains("failed") || line.contains("Failed") {
        Line::from(Span::styled(line, Style::default().fg(Color::Red)))
    } else if line.contains("warning") || line.contains("Warning") {
        Line::from(Span::styled(line, Style::default().fg(Color::Yellow)))
    } else if line.contains("succeeded") || line.contains("completed") {
        Line::from(Span::styled(line, Style::default().fg(Color::Green)))
    } else if line.contains("timer")
        || line.contains("verification")
        || line.contains("pass")
        || line.contains("Daemon")
    {
        // System logs
        Line::from(Span::styled(line, Style::default().fg(Color::Magenta)))
    } else {
        Line::from(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_style_log_line() {
        // ==> Primary makepkg header - building stage
        let line = style_log_line("==> Starting build()...");
        assert_eq!(line.spans.len(), 2);
        assert_eq!(line.spans[0].content, "==> ");
        assert_eq!(line.spans[0].style.fg, Some(Color::Blue));
        assert_eq!(line.spans[1].content, "Starting build()...");

        // ==> Primary makepkg header - success
        let line = style_log_line("==> Finished making: python-mempalace");
        assert_eq!(line.spans[0].style.fg, Some(Color::Green));

        // -> Sub-header
        let line = style_log_line("  -> Checking runtime dependencies...");
        assert_eq!(line.spans.len(), 2);
        assert_eq!(line.spans[0].content, "  -> ");
        assert_eq!(line.spans[0].style.fg, Some(Color::Cyan));
        assert_eq!(line.spans[1].content, "Checking runtime dependencies...");

        // Daemon package log - Checking (blue tag)
        let line = style_log_line("[python-mempalace] Checking upstream version...");
        assert_eq!(line.spans.len(), 5);
        assert_eq!(line.spans[1].content, "python-mempalace");
        assert_eq!(line.spans[1].style.fg, Some(Color::Blue));
        assert_eq!(line.spans[4].style.fg, Some(Color::Gray));

        // Daemon package log - Error (red tag)
        let line = style_log_line("[python-mempalace] Failed to build.");
        assert_eq!(line.spans[1].style.fg, Some(Color::Red));
        assert_eq!(line.spans[4].style.fg, Some(Color::Red));

        // Daemon package log - Success (green tag)
        let line = style_log_line("[python-mempalace] Update completed successfully.");
        assert_eq!(line.spans[1].style.fg, Some(Color::Green));
        assert_eq!(line.spans[4].style.fg, Some(Color::Green));

        // Fallback log - system log (magenta)
        let line = style_log_line("Periodic timer: triggering automatic update check.");
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].style.fg, Some(Color::Magenta));
    }
}
