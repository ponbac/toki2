use super::*;

pub fn render_template_selection(frame: &mut Frame, app: &App, body: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3), // Search input
            Constraint::Min(0),    // Template list
            Constraint::Length(3), // Controls
        ])
        .split(body);

    // Search input box
    let (search_text, template_cursor_col) = if app.template_search_input.value.is_empty() {
        if app.selection_list_focused {
            ("Type to search...".to_string(), None)
        } else {
            (String::new(), Some(0u16))
        }
    } else if app.selection_list_focused {
        (app.template_search_input.value.clone(), None)
    } else {
        let (before, after) = app.template_search_input.split_at_cursor();
        let col = before.chars().count() as u16;
        (format!("{}{}", before, after), Some(col))
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
    if let Some(col) = template_cursor_col {
        frame.set_cursor_position((chunks[0].x + 2 + col, chunks[0].y + 1));
    }

    // Template list
    let items: Vec<ListItem> = app
        .filtered_templates
        .iter()
        .enumerate()
        .map(|(i, template)| {
            let selected = i == app.filtered_template_index;
            let desc_style = if selected {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::White)
            };
            let sub_style = Style::default().fg(Color::DarkGray);

            let line1 = Line::from(Span::styled(template.description.clone(), desc_style));
            let line2 = Line::from(Span::styled(
                format!("{}: {}", template.project, template.activity),
                sub_style,
            ));

            ListItem::new(vec![line1, line2])
        })
        .collect();

    // Show count: filtered / total
    let title = if app.template_search_input.value.is_empty() {
        format!(" Templates ({}) ", app.templates.len())
    } else {
        format!(
            " Templates ({}/{}) ",
            app.filtered_templates.len(),
            app.templates.len()
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
        Span::raw(": Apply  "),
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
