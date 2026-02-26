use super::*;

pub fn render_project_selection(frame: &mut Frame, app: &App, body: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3), // Search input
            Constraint::Min(0),    // Project list
            Constraint::Length(3), // Controls
        ])
        .split(body);

    // Search input box
    let search_text = if app.project_search_input.value.is_empty() {
        if app.selection_list_focused {
            "Type to search...".to_string()
        } else {
            "█".to_string()
        }
    } else if app.selection_list_focused {
        app.project_search_input.value.clone()
    } else {
        let (before, after) = app.project_search_input.split_at_cursor();
        format!("{}█{}", before, after)
    };
    let search_border = if app.selection_list_focused {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };
    let search_box = Paragraph::new(search_text)
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Left)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(search_border)
                .title(" Search ")
                .padding(Padding::horizontal(1)),
        );
    frame.render_widget(search_box, chunks[0]);

    // Project list
    let items: Vec<ListItem> = app
        .filtered_projects
        .iter()
        .enumerate()
        .map(|(i, project)| {
            let text = project.name.clone();

            let style = if i == app.filtered_project_index {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(text).style(style)
        })
        .collect();

    // Show count: filtered / total
    let title = if app.project_search_input.value.is_empty() {
        format!(" Projects ({}) ", app.projects.len())
    } else {
        format!(
            " Projects ({}/{}) ",
            app.filtered_projects.len(),
            app.projects.len()
        )
    };

    let list_border = if app.selection_list_focused {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(list_border)
                .title(title)
                .padding(Padding::horizontal(1)),
        )
        .style(Style::default());

    frame.render_widget(list, chunks[1]);

    // Controls
    let controls_text = vec![
        Span::styled("Type", Style::default().fg(Color::Yellow)),
        Span::raw(": Filter  "),
        Span::styled("Tab", Style::default().fg(Color::Yellow)),
        Span::raw(": Focus list  "),
        Span::styled("↑↓/j/k", Style::default().fg(Color::Yellow)),
        Span::raw(": Navigate  "),
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::raw(": Select  "),
        Span::styled("Ctrl+X", Style::default().fg(Color::Yellow)),
        Span::raw(": Clear  "),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::raw(": Cancel"),
    ];

    let controls = Paragraph::new(Line::from(controls_text))
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

    frame.render_widget(controls, chunks[2]);
}

pub fn render_activity_selection(frame: &mut Frame, app: &App, body: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3), // Search input
            Constraint::Min(0),    // Activity list
            Constraint::Length(3), // Controls
        ])
        .split(body);

    // Search input box
    let search_text = if app.activity_search_input.value.is_empty() {
        if app.selection_list_focused {
            "Type to search...".to_string()
        } else {
            "█".to_string()
        }
    } else if app.selection_list_focused {
        app.activity_search_input.value.clone()
    } else {
        let (before, after) = app.activity_search_input.split_at_cursor();
        format!("{}█{}", before, after)
    };
    let search_border = if app.selection_list_focused {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };
    let search_box = Paragraph::new(search_text)
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Left)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(search_border)
                .title(" Search ")
                .padding(Padding::horizontal(1)),
        );
    frame.render_widget(search_box, chunks[0]);

    // Activity list
    let items: Vec<ListItem> = app
        .filtered_activities
        .iter()
        .enumerate()
        .map(|(i, activity)| {
            let style = if i == app.filtered_activity_index {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(activity.name.clone()).style(style)
        })
        .collect();

    // Show count: filtered / total
    let title = if app.activity_search_input.value.is_empty() {
        format!(" Activities ({}) ", app.activities.len())
    } else {
        format!(
            " Activities ({}/{}) ",
            app.filtered_activities.len(),
            app.activities.len()
        )
    };

    let list_border = if app.selection_list_focused {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(list_border)
                .title(title)
                .padding(Padding::horizontal(1)),
        )
        .style(Style::default());

    frame.render_widget(list, chunks[1]);

    // Controls
    let controls_text = vec![
        Span::styled("Type", Style::default().fg(Color::Yellow)),
        Span::raw(": Filter  "),
        Span::styled("Tab", Style::default().fg(Color::Yellow)),
        Span::raw(": Focus list  "),
        Span::styled("↑↓/j/k", Style::default().fg(Color::Yellow)),
        Span::raw(": Navigate  "),
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::raw(": Select  "),
        Span::styled("Ctrl+X", Style::default().fg(Color::Yellow)),
        Span::raw(": Clear  "),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::raw(": Cancel"),
    ];

    let controls = Paragraph::new(Line::from(controls_text))
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

    frame.render_widget(controls, chunks[2]);
}
