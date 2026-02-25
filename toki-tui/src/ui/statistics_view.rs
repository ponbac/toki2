use super::*;

/// Shared color palette — same order for pie slices and daily bars
pub const PALETTE: [Color; 12] = [
    Color::Blue,
    Color::Green,
    Color::Yellow,
    Color::Magenta,
    Color::Cyan,
    Color::Red,
    Color::LightBlue,
    Color::LightGreen,
    Color::LightYellow,
    Color::LightMagenta,
    Color::LightCyan,
    Color::LightRed,
];

pub fn render_statistics_view(frame: &mut Frame, app: &App, body: Rect) {
    // Outer vertical split: chart area + controls bar
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([Constraint::Min(10), Constraint::Length(3)])
        .split(body);

    // Outer "Statistics" box
    let stats_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::White))
        .title(Span::styled(
            " Statistics ",
            Style::default().fg(Color::White),
        ));
    let stats_inner = stats_block.inner(outer[0]);
    frame.render_widget(stats_block, outer[0]);

    // Horizontal split: pie (50%) | daily bar chart (50%)
    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(stats_inner);

    // Apply 4-char left/right padding to each panel
    let pad = |r: Rect| Rect {
        x: r.x + 4,
        y: r.y,
        width: r.width.saturating_sub(8),
        height: r.height,
    };

    render_pie_panel(frame, app, pad(panels[0]));
    render_daily_panel(frame, app, pad(panels[1]));

    // Controls bar
    let stats_controls = vec![
        Span::styled("S / Esc", Style::default().fg(Color::Yellow)),
        Span::raw(": Back to timer  "),
        Span::styled("Q", Style::default().fg(Color::Yellow)),
        Span::raw(": Quit"),
    ];
    let controls = Paragraph::new(Line::from(stats_controls))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(
                    " Controls ",
                    Style::default().fg(Color::DarkGray),
                ))
                .padding(ratatui::widgets::Padding::horizontal(1)),
        );
    frame.render_widget(controls, outer[1]);
}

fn render_pie_panel(frame: &mut Frame, app: &App, area: Rect) {
    use tui_piechart::{PieChart, PieSlice};

    let stats = &app.weekly_stats_cache;

    if stats.is_empty() {
        let empty = Paragraph::new("No data")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(empty, area);
        return;
    }

    let label_strings: Vec<String> = stats
        .iter()
        .map(|s| {
            let h = s.hours.floor() as u64;
            let m = ((s.hours - h as f64) * 60.0).round() as u64;
            format!("{}: {:02}h:{:02}m", s.label, h, m)
        })
        .collect();

    let slices: Vec<PieSlice> = label_strings
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let color = PALETTE[i % PALETTE.len()];
            PieSlice::new(label.as_str(), stats[i].percentage, color)
        })
        .collect();

    // Pie: square-ish (width/2 for aspect ratio), capped at half the panel height
    let n = stats.len() as u16;
    let legend_rows = n + 1; // one line per entry + 1 top padding line
    let pie_height = (area.width / 2)
        .min(area.height / 2)
        .min(area.height.saturating_sub(legend_rows));

    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(pie_height), Constraint::Min(0)])
        .split(area);

    // Render pie without its built-in legend
    let pie = PieChart::new(slices.clone())
        .show_legend(false)
        .show_percentages(false);
    frame.render_widget(pie, split[0]);

    // Render legend manually, one entry per line, colored
    let total_hours: f64 = stats.iter().map(|s| s.hours).sum();
    let mut legend_lines: Vec<Line> = Vec::new();
    for (i, s) in stats.iter().enumerate() {
        let color = PALETTE[i % PALETTE.len()];
        let pct = if total_hours > 0.0 {
            s.hours / total_hours * 100.0
        } else {
            0.0
        };
        let h = s.hours.floor() as u64;
        let m = ((s.hours - h as f64) * 60.0).round() as u64;
        legend_lines.push(Line::from(vec![
            Span::styled("■ ", Style::default().fg(color)),
            Span::styled(
                format!("{} — {:02}h:{:02}m ({:.0}%)", s.label, h, m, pct),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }
    let legend = Paragraph::new(legend_lines)
        .alignment(Alignment::Center)
        .block(Block::default().padding(ratatui::widgets::Padding::new(0, 0, 1, 0)));
    frame.render_widget(legend, split[1]);
}

fn render_daily_panel(frame: &mut Frame, app: &App, area: Rect) {
    let day_stats = &app.weekly_daily_stats_cache;

    // Find max daily hours for bar scaling
    let max_hours = day_stats
        .iter()
        .map(|d| d.total_hours)
        .fold(0.0_f64, f64::max);

    // bar_cols = area width - 5 (day label "Mon ") - 9 ("  Xh:XXm")
    let bar_cols = (area.width as i32 - 5 - 9).max(1) as usize;

    let mut lines: Vec<Line> = Vec::new();

    let last_index = day_stats.len().saturating_sub(1);
    for (di, day) in day_stats.iter().enumerate() {
        // --- Bar row ---
        let mut spans: Vec<Span> = Vec::new();

        // Day name (4 chars + space)
        spans.push(Span::styled(
            format!("{:<4} ", day.day_name),
            Style::default().fg(Color::White),
        ));

        if day.total_hours <= 0.0 || max_hours <= 0.0 {
            spans.push(Span::styled(
                "─".repeat(bar_cols),
                Style::default().fg(Color::DarkGray),
            ));
            spans.push(Span::raw("         "));
        } else {
            let total_filled = ((day.total_hours / max_hours) * bar_cols as f64).round() as usize;
            let total_filled = total_filled.min(bar_cols);
            let mut remaining_fill = total_filled;

            for (pi, proj) in day.projects.iter().enumerate() {
                if remaining_fill == 0 {
                    break;
                }
                let proj_filled = if pi == day.projects.len() - 1 {
                    remaining_fill
                } else {
                    let cols =
                        ((proj.hours / day.total_hours) * total_filled as f64).round() as usize;
                    cols.min(remaining_fill)
                };
                if proj_filled > 0 {
                    let color = PALETTE[proj.color_index % PALETTE.len()];
                    spans.push(Span::styled(
                        "█".repeat(proj_filled),
                        Style::default().fg(color),
                    ));
                    remaining_fill -= proj_filled;
                }
            }

            if total_filled < bar_cols {
                spans.push(Span::styled(
                    "░".repeat(bar_cols - total_filled),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            let h = day.total_hours.floor() as u64;
            let m = ((day.total_hours - h as f64) * 60.0).round() as u64;
            spans.push(Span::styled(
                format!("  {:>2}h:{:02}m", h, m),
                Style::default().fg(Color::White),
            ));
        }

        lines.push(Line::from(spans));

        // Blank line between bar and its project labels
        lines.push(Line::raw(""));

        // --- One label row per project ---
        for proj in &day.projects {
            let color = PALETTE[proj.color_index % PALETTE.len()];
            let h = proj.hours.floor() as u64;
            let m = ((proj.hours - h as f64) * 60.0).round() as u64;
            lines.push(Line::from(vec![
                Span::raw("     "), // indent to align under bar
                Span::styled("■ ", Style::default().fg(color)),
                Span::styled(
                    format!("{} ({}h:{:02}m)", proj.label, h, m),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }

        // Blank row between days (not after the last one)
        if di < last_index {
            lines.push(Line::raw(""));
        }
    }

    let text = ratatui::text::Text::from(lines);
    let paragraph = Paragraph::new(text)
        // 4-row bottom padding reserves space so the bar chart legend doesn't
        // overlap the compact stats header rendered in the row above the body.
        .block(Block::default().padding(ratatui::widgets::Padding::new(0, 0, 4, 0)));
    frame.render_widget(paragraph, area);
}
