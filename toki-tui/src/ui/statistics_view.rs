use super::*;

pub fn render_statistics_view(frame: &mut Frame, app: &App, body: Rect) {
    use tui_piechart::{PieChart, PieSlice};

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Min(10),   // Pie chart area
            Constraint::Length(3), // Controls
        ])
        .split(body);

    // Surrounding white box with "Statistics" title
    let stats_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::White))
        .title(Span::styled(
            " Statistics ",
            Style::default().fg(Color::White),
        ));
    let chart_inner = stats_block.inner(outer[0]);
    frame.render_widget(stats_block, outer[0]);

    // Add top/bottom padding inside the box to shrink the pie chart
    let padded = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(chart_inner);
    let chart_area = padded[1];

    // --- Pie chart ---
    let stats = app.weekly_project_stats();

    if stats.is_empty() {
        let empty = Paragraph::new("No completed entries this week")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(empty, chart_area);
    } else {
        let palette = [
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

        // Build owned label strings first (PieSlice borrows &str)
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
                let color = palette[i % palette.len()];
                PieSlice::new(label.as_str(), stats[i].percentage, color)
            })
            .collect();

        let pie = PieChart::new(slices)
            .show_legend(true)
            .show_percentages(true);
        frame.render_widget(pie, chart_area);
    }

    // --- Controls ---
    let stats_controls = vec![
        Span::styled("S", Style::default().fg(Color::Yellow)),
        Span::raw("/"),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::raw(": Back to Timer  "),
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
                    " Statistics ",
                    Style::default().fg(Color::DarkGray),
                ))
                .padding(ratatui::widgets::Padding::horizontal(1)),
        );
    frame.render_widget(controls, outer[1]);
}
