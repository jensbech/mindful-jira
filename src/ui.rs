use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState,
    Table, TableState, Wrap,
};
use ratatui::Frame;

use crate::app::{fuzzy_match, App, Column, HighlightColor, Mode, SortCriteria, HIGHLIGHT_OPTIONS};

const ZEBRA_DARK: Color = Color::Rgb(30, 30, 40);
const HIGHLIGHT_BG: Color = Color::Rgb(55, 55, 80);
const HIGHLIGHT_ORANGE_BG: Color = Color::Rgb(80, 45, 10);
const HIGHLIGHT_GREEN_BG: Color = Color::Rgb(20, 50, 20);
const DIM: Color = Color::Rgb(100, 100, 110);
const ACCENT: Color = Color::Rgb(180, 180, 255);

fn issue_type_icon(issue_type: &str) -> (&'static str, Color) {
    match issue_type {
        "Bug" => ("●", Color::Rgb(229, 73, 58)),
        "Story" | "User Story" => ("◆", Color::Rgb(99, 186, 60)),
        "Task" => ("■", Color::Rgb(75, 173, 232)),
        "Sub-task" | "Subtask" => ("▪", Color::Rgb(75, 173, 232)),
        "Epic" => ("♦", Color::Rgb(144, 78, 226)),
        "Improvement" => ("▲", Color::Rgb(99, 186, 60)),
        "New Feature" | "Feature" => ("✦", Color::Rgb(99, 186, 60)),
        "Spike" => ("◇", Color::Rgb(140, 140, 160)),
        _ => ("•", Color::DarkGray),
    }
}

fn split_at_char_pos(s: &str, pos: usize) -> (&str, &str) {
    let byte_pos = s
        .char_indices()
        .nth(pos)
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    s.split_at(byte_pos)
}

fn visible_input(input: &str, cursor_pos: usize, max_chars: usize) -> String {
    let char_count = input.chars().count();
    if char_count + 1 <= max_chars {
        let (before, after) = split_at_char_pos(input, cursor_pos);
        return format!("{before}|{after}");
    }
    let budget = max_chars.saturating_sub(1);
    let half = budget / 2;
    let mut start = cursor_pos.saturating_sub(half);
    let mut end = start + budget;
    if end > char_count {
        end = char_count;
        start = end.saturating_sub(budget);
    }
    let left_ellipsis = start > 0;
    let right_ellipsis = end < char_count;
    if left_ellipsis {
        start += 1;
    }
    if right_ellipsis && end > start {
        end -= 1;
    }
    let visible: String = input.chars().skip(start).take(end - start).collect();
    let cursor_in_vis = cursor_pos.saturating_sub(start);
    let (before, after) = split_at_char_pos(&visible, cursor_in_vis);
    let mut result = String::new();
    if left_ellipsis {
        result.push('\u{2026}');
    }
    result.push_str(before);
    result.push('|');
    result.push_str(after);
    if right_ellipsis {
        result.push('\u{2026}');
    }
    result
}

pub fn draw(f: &mut Frame, app: &App) {
    let show_search_bar = app.mode == Mode::Searching || !app.search_input.is_empty();
    let constraints = if show_search_bar {
        vec![
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(1),
        ]
    } else {
        vec![Constraint::Min(3), Constraint::Length(1)]
    };
    let chunks = Layout::vertical(constraints).split(f.area());

    draw_table(f, app, chunks[0]);
    if show_search_bar {
        draw_search_bar(f, app, chunks[1]);
        draw_status_bar(f, app, chunks[2]);
    } else {
        draw_status_bar(f, app, chunks[1]);
    }

    match app.mode {
        Mode::ConfirmBrowser => {
            dim_background(f);
            draw_confirm_browser_modal(f, app);
        }
        Mode::FilterEditor | Mode::FilterAdding => {
            dim_background(f);
            draw_filter_modal(f, app);
        }
        Mode::TicketDetail
        | Mode::DetailAddingComment
        | Mode::DetailEditingComment
        | Mode::DetailConfirmDelete
        | Mode::DetailTransition
        | Mode::DetailEditingSummary => {
            dim_background(f);
            draw_detail_modal(f, app);
        }
        Mode::DetailConfirmTransition => {
            dim_background(f);
            draw_detail_modal(f, app);
            draw_confirm_transition_modal(f, app);
        }
        Mode::DetailPRList => {
            dim_background(f);
            draw_detail_modal(f, app);
            draw_pr_list_modal(f, app);
        }
        Mode::EditingLongNote => {
            dim_background(f);
            draw_long_note_modal(f, app);
        }
        Mode::HighlightPicker => {
            dim_background(f);
            draw_highlight_picker_modal(f, app);
        }
        Mode::SortPicker => {
            dim_background(f);
            draw_sort_picker_modal(f, app);
        }
        Mode::ColumnPicker => {
            dim_background(f);
            draw_column_picker_modal(f, app);
        }
        Mode::ConfirmQuit => {
            dim_background(f);
            draw_confirm_quit_modal(f);
        }
        _ => {}
    }

    if app.show_legend {
        draw_legend(f);
    }
}

fn dim_background(f: &mut Frame) {
    let area = f.area();
    let buf = f.buffer_mut();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_fg(Color::Rgb(50, 50, 60));
                cell.set_bg(Color::Rgb(10, 10, 15));
            }
        }
    }
}

fn draw_legend(f: &mut Frame) {
    let entries: &[(&str, &str, Color)] = &[
        ("●", "Bug", Color::Rgb(229, 73, 58)),
        ("◆", "Story", Color::Rgb(99, 186, 60)),
        ("■", "Task", Color::Rgb(75, 173, 232)),
        ("▪", "Sub-task", Color::Rgb(75, 173, 232)),
        ("♦", "Epic", Color::Rgb(144, 78, 226)),
        ("▲", "Improvement", Color::Rgb(99, 186, 60)),
        ("✦", "New Feature", Color::Rgb(99, 186, 60)),
        ("◇", "Spike", Color::Rgb(140, 140, 160)),
    ];

    let width: u16 = 20;
    let height = entries.len() as u16 + 3; // entries + border + title + bottom padding
    let area = f.area();
    let x = area.width.saturating_sub(width + 2);
    let y = area.height.saturating_sub(height + 2);
    let legend_area = Rect::new(x, y, width, height);

    f.render_widget(Clear, legend_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(80, 80, 110)))
        .title(Span::styled(
            " Types ",
            Style::default()
                .fg(Color::Rgb(180, 180, 200))
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(legend_area);
    f.render_widget(block, legend_area);

    let lines: Vec<Line> = entries
        .iter()
        .map(|(sym, label, color)| {
            Line::from(vec![
                Span::styled(format!(" {sym}"), Style::default().fg(*color)),
                Span::styled(
                    format!("  {label}"),
                    Style::default().fg(Color::Rgb(180, 180, 200)),
                ),
            ])
        })
        .collect();

    f.render_widget(Paragraph::new(lines), inner);
}

fn priority_style(priority: &str) -> Style {
    match priority {
        "Blocker" | "Critical" | "Highest" => {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        }
        "Major" | "High" => Style::default().fg(Color::Yellow),
        "Normal" => Style::default().fg(Color::White),
        "Medium" => Style::default().fg(Color::Cyan),
        "Minor" | "Low" | "Trivial" | "Lowest" => Style::default().fg(Color::DarkGray),
        _ => Style::default(),
    }
}

fn status_style(status: &str) -> Style {
    match status {
        "In Progress" => Style::default().fg(Color::Green),
        "In Test" | "In Review" => Style::default().fg(Color::Magenta),
        s if s.starts_with("Selected") => Style::default().fg(Color::Blue),
        "Open" | "Backlog" | "To Do" => Style::default().fg(Color::DarkGray),
        _ => Style::default(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        s.to_string()
    } else if max <= 3 {
        s.chars().take(max).collect()
    } else {
        let t: String = s.chars().take(max - 1).collect();
        format!("{t}…")
    }
}

// ── Main table ──────────────────────────────────────────────

fn draw_table(f: &mut Frame, app: &App, area: Rect) {
    let col_assignee = app.show_all_parents && app.is_column_visible(Column::Assignee);
    let col_reporter = app.is_column_visible(Column::Reporter);
    let col_priority = app.is_column_visible(Column::Priority);
    let col_status = app.is_column_visible(Column::Status);
    let col_resolution = app.is_column_visible(Column::Resolution);
    let col_created = app.is_column_visible(Column::Created);

    const ASSIGNEE_W: u16 = 16;
    const REPORTER_W: u16 = 18;
    const PRIORITY_W: u16 = 10;
    const STATUS_W: u16 = 16;
    const RESOLUTION_W: u16 = 12;
    const CREATED_W: u16 = 12;
    const COL_SPACING: u16 = 2;
    const BORDERS: u16 = 2;
    const HIGHLIGHT_SYM: u16 = 2;

    let mut num_cols: u16 = 2; // Work + My Status always present
    let mut fixed: u16 = BORDERS + HIGHLIGHT_SYM;
    if col_assignee { num_cols += 1; fixed += ASSIGNEE_W; }
    if col_reporter { num_cols += 1; fixed += REPORTER_W; }
    if col_priority { num_cols += 1; fixed += PRIORITY_W; }
    if col_status { num_cols += 1; fixed += STATUS_W; }
    if col_resolution { num_cols += 1; fixed += RESOLUTION_W; }
    if col_created { num_cols += 1; fixed += CREATED_W; }
    fixed += COL_SPACING * (num_cols - 1);

    let remaining = area.width.saturating_sub(fixed);
    let work_w = ((remaining as u32 * 3 / 4) as u16).max(20);
    let notes_w = remaining.saturating_sub(work_w).max(10);

    let work_chars = work_w as usize;
    let notes_chars = notes_w as usize;
    let assignee_chars = ASSIGNEE_W as usize;
    let reporter_chars = REPORTER_W as usize;
    let status_chars = STATUS_W as usize;

    let header_style = Style::default()
        .fg(Color::Rgb(180, 180, 200))
        .add_modifier(Modifier::BOLD);

    let mut header_cells = vec![Cell::from("Work")];
    if col_assignee { header_cells.push(Cell::from("Assignee")); }
    if col_reporter { header_cells.push(Cell::from("Reporter")); }
    if col_priority { header_cells.push(Cell::from("Priority")); }
    if col_status { header_cells.push(Cell::from("Status")); }
    if col_resolution { header_cells.push(Cell::from("Resolution")); }
    if col_created { header_cells.push(Cell::from("Created")); }
    header_cells.push(Cell::from("My Status"));

    let header = Row::new(header_cells)
        .style(header_style)
        .bottom_margin(1);

    let rows: Vec<Row> = app
        .rows
        .iter()
        .enumerate()
        .map(|(i, display_row)| {
            let issue = &display_row.issue;
            let is_parent = display_row.is_context_parent;

            let (icon, icon_color) = issue_type_icon(&issue.issue_type);
            let depth_prefix = if display_row.depth > 0 { "  └ " } else { "" };
            let key_summary = format!("{} {}", issue.key, issue.summary);
            let prefix_len = depth_prefix.chars().count() + icon.chars().count() + 1;

            let note = app.notes.get(&issue.key).cloned().unwrap_or_default();
            let has_long_note = app.long_notes.contains_key(&issue.key);
            let note_prefix = if has_long_note { "\u{270d} " } else { "" };
            let avail = notes_chars.saturating_sub(note_prefix.chars().count());
            let note_text = if app.mode == Mode::EditingNote && i == app.selected {
                format!("{}{}", note_prefix, visible_input(&app.note_input, app.cursor_pos, avail))
            } else {
                format!("{}{}", note_prefix, truncate(&note, avail))
            };

            let reporter_text = truncate(&issue.reporter, reporter_chars);
            let status_text = truncate(&issue.status, status_chars);

            let is_muted = app.muted_keys.contains(&issue.key);
            let base_fg = if is_parent || is_muted { DIM } else { Color::White };
            let base_style = Style::default().fg(base_fg);

            let highlight_color = app.highlighted_keys.get(&issue.key).and_then(|s| HighlightColor::from_str(s));
            let bg = if let Some(color) = highlight_color {
                match color {
                    HighlightColor::Orange => HIGHLIGHT_ORANGE_BG,
                    HighlightColor::Green => HIGHLIGHT_GREEN_BG,
                }
            } else if i % 2 == 1 {
                ZEBRA_DARK
            } else {
                Color::Reset
            };
            let row_style = base_style.bg(bg);

            let p_style = if is_parent || is_muted {
                Style::default().fg(DIM).bg(bg)
            } else {
                priority_style(&issue.priority).bg(bg)
            };
            let s_style = if is_parent || is_muted {
                Style::default().fg(DIM).bg(bg)
            } else {
                status_style(&issue.status).bg(bg)
            };

            let note_style = Style::default().fg(Color::Rgb(140, 200, 255)).bg(bg);

            let ic = if is_muted { DIM } else { icon_color };
            let key_summary_text = truncate(&key_summary, work_chars.saturating_sub(prefix_len));

            // Build Work cell with optional fuzzy match highlighting
            let text_spans = if !app.search_input.is_empty() {
                if let Some(positions) = fuzzy_match(&key_summary, &app.search_input) {
                    // Map positions from key_summary to key_summary_text
                    let max_pos = key_summary_text.chars().count();
                    let highlight_set: std::collections::HashSet<usize> =
                        positions.into_iter().filter(|&p| p < max_pos).collect();
                    let mut spans = Vec::new();
                    let normal_style = base_style.bg(bg);
                    let match_style = Style::default()
                        .fg(Color::Rgb(255, 200, 60))
                        .bg(bg)
                        .add_modifier(Modifier::BOLD);
                    let mut current = String::new();
                    let mut current_is_match = false;
                    for (ci, ch) in key_summary_text.chars().enumerate() {
                        let is_match = highlight_set.contains(&ci);
                        if is_match != current_is_match && !current.is_empty() {
                            let style = if current_is_match { match_style } else { normal_style };
                            spans.push(Span::styled(std::mem::take(&mut current), style));
                        }
                        current.push(ch);
                        current_is_match = is_match;
                    }
                    if !current.is_empty() {
                        let style = if current_is_match { match_style } else { normal_style };
                        spans.push(Span::styled(current, style));
                    }
                    spans
                } else {
                    vec![Span::styled(key_summary_text.clone(), base_style.bg(bg))]
                }
            } else {
                vec![Span::styled(key_summary_text.clone(), base_style.bg(bg))]
            };

            let mut work_spans = vec![
                Span::styled(depth_prefix.to_string(), base_style.bg(bg)),
                Span::styled(icon.to_string(), Style::default().fg(ic).bg(bg)),
                Span::styled(" ".to_string(), base_style.bg(bg)),
            ];
            work_spans.extend(text_spans);
            let work_cell = Cell::from(Line::from(work_spans));
            let mut cells = vec![work_cell];
            if col_assignee {
                let assignee_style = if is_parent || is_muted {
                    Style::default().fg(DIM)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                cells.push(Cell::from(Span::styled(
                    truncate(&issue.assignee, assignee_chars),
                    assignee_style.bg(bg),
                )));
            }
            if col_reporter {
                cells.push(Cell::from(Span::styled(reporter_text, base_style.bg(bg))));
            }
            if col_priority {
                cells.push(Cell::from(Span::styled(issue.priority.clone(), p_style)));
            }
            if col_status {
                cells.push(Cell::from(Span::styled(status_text, s_style)));
            }
            if col_resolution {
                cells.push(Cell::from(Span::styled(issue.resolution.clone(), base_style.bg(bg))));
            }
            if col_created {
                cells.push(Cell::from(Span::styled(
                    issue.created.clone(),
                    Style::default().fg(Color::DarkGray).bg(bg),
                )));
            }
            cells.push(Cell::from(Span::styled(note_text, note_style)));

            Row::new(cells).style(row_style)
        })
        .collect();

    let mut widths = vec![Constraint::Length(work_w)];
    if col_assignee { widths.push(Constraint::Length(ASSIGNEE_W)); }
    if col_reporter { widths.push(Constraint::Length(REPORTER_W)); }
    if col_priority { widths.push(Constraint::Length(PRIORITY_W)); }
    if col_status { widths.push(Constraint::Length(STATUS_W)); }
    if col_resolution { widths.push(Constraint::Length(RESOLUTION_W)); }
    if col_created { widths.push(Constraint::Length(CREATED_W)); }
    widths.push(Constraint::Length(notes_w));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(60, 60, 80)))
        .title(Line::from(vec![
            Span::styled(
                " Mindful Jira ",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("v{} ", env!("CARGO_PKG_VERSION")),
                Style::default().fg(Color::Rgb(80, 80, 100)),
            ),
        ]));

    if rows.is_empty() && !app.search_input.is_empty() {
        let empty_rows: Vec<Row> = vec![Row::new(vec![Cell::from(Span::styled(
            "  No matches",
            Style::default().fg(Color::DarkGray),
        ))])];
        let table = Table::new(empty_rows, widths)
            .header(header)
            .block(block)
            .column_spacing(COL_SPACING);
        f.render_widget(table, area);
    } else {
        let table = Table::new(rows, widths)
            .header(header)
            .block(block)
            .column_spacing(COL_SPACING)
            .row_highlight_style(
                Style::default()
                    .bg(HIGHLIGHT_BG)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        let mut state = TableState::default();
        if !app.rows.is_empty() {
            state.select(Some(app.selected));
        }

        f.render_stateful_widget(table, area, &mut state);
    }
}

// ── Confirm browser modal ────────────────────────────────────

fn draw_confirm_browser_modal(f: &mut Frame, app: &App) {
    let key = app
        .rows
        .get(app.selected)
        .map(|r| r.issue.key.as_str())
        .unwrap_or("");

    let area = f.area();
    let width = 44u16.min(area.width.saturating_sub(4));
    let height = 6u16;
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let modal_area = Rect::new(x, y, width, height);

    f.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            " Open in Browser ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(modal_area);
    f.render_widget(block, modal_area);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  Open {key} in browser?"),
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  y/Enter:Confirm  n/Esc:Cancel",
            Style::default().fg(Color::Rgb(100, 100, 120)),
        )),
    ];

    f.render_widget(Paragraph::new(lines), inner);
}

// ── Confirm quit modal ───────────────────────────────────────

fn draw_confirm_quit_modal(f: &mut Frame) {
    let area = f.area();
    let width = 36u16.min(area.width.saturating_sub(4));
    let height = 6u16;
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let modal_area = Rect::new(x, y, width, height);

    f.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            " Quit ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(modal_area);
    f.render_widget(block, modal_area);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Quit Mindful Jira?",
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  y/Enter:Quit  n/Esc:Cancel",
            Style::default().fg(Color::Rgb(100, 100, 120)),
        )),
    ];

    f.render_widget(Paragraph::new(lines), inner);
}

// ── Highlight picker modal ───────────────────────────────────

fn draw_highlight_picker_modal(f: &mut Frame, app: &App) {
    let current = app.current_highlight();
    // Options: Yellow, Green, Remove (only if currently highlighted)
    let has_highlight = current.is_some();
    let option_count = if has_highlight { HIGHLIGHT_OPTIONS.len() + 1 } else { HIGHLIGHT_OPTIONS.len() };
    let height = (option_count as u16) + 4; // border + title + options + hints

    let area = f.area();
    let width = 36u16.min(area.width.saturating_sub(4));
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let modal_area = Rect::new(x, y, width, height);

    f.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            " Highlight ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(modal_area);
    f.render_widget(block, modal_area);

    let mut lines: Vec<Line> = Vec::new();

    for (i, opt) in HIGHLIGHT_OPTIONS.iter().enumerate() {
        let marker = if i == app.highlight_selected { "▶ " } else { "  " };
        let is_active = current.map(|c| c.as_str() == opt.as_str()).unwrap_or(false);
        let dot_color = match opt {
            HighlightColor::Orange => Color::Rgb(255, 180, 50),
            HighlightColor::Green => Color::Green,
        };
        let label = if is_active {
            format!("{} (active)", opt.label())
        } else {
            opt.label().to_string()
        };
        let fg = if i == app.highlight_selected { Color::White } else { Color::Rgb(180, 180, 180) };
        lines.push(Line::from(vec![
            Span::styled(marker, Style::default().fg(fg)),
            Span::styled("● ", Style::default().fg(dot_color)),
            Span::styled(label, Style::default().fg(fg)),
        ]));
    }

    if has_highlight {
        let i = HIGHLIGHT_OPTIONS.len();
        let marker = if i == app.highlight_selected { "▶ " } else { "  " };
        let fg = if i == app.highlight_selected { Color::White } else { Color::Rgb(180, 180, 180) };
        lines.push(Line::from(vec![
            Span::styled(marker, Style::default().fg(fg)),
            Span::styled("✕ ", Style::default().fg(Color::Rgb(200, 80, 80))),
            Span::styled("Remove highlight", Style::default().fg(fg)),
        ]));
    }

    lines.push(Line::from(Span::styled(
        " Enter:Select  Esc:Cancel",
        Style::default().fg(Color::Rgb(100, 100, 120)),
    )));

    f.render_widget(Paragraph::new(lines), inner);
}

// ── Sort picker modal ────────────────────────────────────────

fn draw_sort_picker_modal(f: &mut Frame, app: &App) {
    let options = SortCriteria::ALL;
    let height = (options.len() as u16) + 4; // border + options + hint

    let area = f.area();
    let width = 40u16.min(area.width.saturating_sub(4));
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let modal_area = Rect::new(x, y, width, height);

    f.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            " Sort ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(modal_area);
    f.render_widget(block, modal_area);

    let mut lines: Vec<Line> = Vec::new();

    for (i, opt) in options.iter().enumerate() {
        let marker = if i == app.sort_selected { "▶ " } else { "  " };
        let is_active = *opt == app.sort_criteria;
        let label = if is_active {
            format!("{} (active)", opt.label())
        } else {
            opt.label().to_string()
        };
        let fg = if i == app.sort_selected {
            Color::White
        } else {
            Color::Rgb(180, 180, 180)
        };
        lines.push(Line::from(vec![
            Span::styled(marker, Style::default().fg(fg)),
            Span::styled(label, Style::default().fg(fg)),
        ]));
    }

    lines.push(Line::from(Span::styled(
        " Enter:Select  Esc:Cancel",
        Style::default().fg(Color::Rgb(100, 100, 120)),
    )));

    f.render_widget(Paragraph::new(lines), inner);
}

// ── Column picker modal ─────────────────────────────────────

fn draw_column_picker_modal(f: &mut Frame, app: &App) {
    let options = Column::ALL;
    let height = (options.len() as u16) + 4;

    let area = f.area();
    let width = 40u16.min(area.width.saturating_sub(4));
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let modal_area = Rect::new(x, y, width, height);

    f.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            " Columns ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(modal_area);
    f.render_widget(block, modal_area);

    let mut lines: Vec<Line> = Vec::new();

    for (i, col) in options.iter().enumerate() {
        let selected = i == app.column_picker_selected;
        let marker = if selected { "▶ " } else { "  " };
        let visible = app.is_column_visible(*col);

        let (icon, icon_color) = if visible {
            ("✓", Color::Green)
        } else {
            ("✕", Color::Red)
        };

        let label = col.label();
        let note = if *col == Column::Assignee && !app.show_all_parents {
            " (needs tree mode)"
        } else {
            ""
        };

        let fg = if selected { Color::White } else { Color::Rgb(180, 180, 180) };
        lines.push(Line::from(vec![
            Span::styled(marker, Style::default().fg(fg)),
            Span::styled(format!("{icon} "), Style::default().fg(icon_color)),
            Span::styled(label.to_string(), Style::default().fg(fg)),
            Span::styled(note.to_string(), Style::default().fg(Color::DarkGray)),
        ]));
    }

    lines.push(Line::from(Span::styled(
        " ↑↓:Navigate  Space:Toggle  Esc:Close",
        Style::default().fg(Color::Rgb(100, 100, 120)),
    )));

    f.render_widget(Paragraph::new(lines), inner);
}

// ── Confirm transition modal ─────────────────────────────────

fn draw_confirm_transition_modal(f: &mut Frame, app: &App) {
    let key = app
        .detail
        .as_ref()
        .map(|d| d.key.as_str())
        .unwrap_or("");
    let target = app
        .transitions
        .get(app.transition_selected)
        .map(|t| t.name.as_str())
        .unwrap_or("");

    let area = f.area();
    let width = 52u16.min(area.width.saturating_sub(4));
    let height = 7u16;
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let modal_area = Rect::new(x, y, width, height);

    f.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            " Confirm Transition ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(modal_area);
    f.render_widget(block, modal_area);

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                key.to_string(),
                Style::default()
                    .fg(ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" \u{2192} ", Style::default().fg(Color::White)),
            Span::styled(
                target.to_string(),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  y/Enter:Confirm  n/Esc:Cancel",
            Style::default().fg(Color::Rgb(100, 100, 120)),
        )),
    ];

    f.render_widget(Paragraph::new(lines), inner);
}

// ── PR list modal ────────────────────────────────────────────

fn draw_pr_list_modal(f: &mut Frame, app: &App) {
    let prs = &app.pr_list;
    let pr_rows = prs.len() as u16;

    let area = f.area();
    // Wide as the detail modal, tall enough for all PRs + padding
    let width = area.width.saturating_sub(6).min(120);
    let inner_h = 1 + pr_rows.max(1) + 1 + 1 + 1; // top pad + rows + spacer + help + bot pad
    let height = (inner_h + 2).min(area.height.saturating_sub(8));
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let modal_area = Rect::new(x, y, width, height);

    // GitHub dark mode palette
    const GH_BG: Color       = Color::Rgb(13, 17, 23);   // #0d1117 canvas-default
    const GH_BG_SEL: Color   = Color::Rgb(22, 27, 34);   // #161b22 canvas-overlay
    const GH_BLUE: Color     = Color::Rgb(88, 166, 255);  // #58a6ff accent
    const GH_TEXT: Color     = Color::Rgb(230, 237, 243); // #e6edf3 fg-default
    const GH_MUTED: Color    = Color::Rgb(139, 148, 158); // #8b949e fg-muted
    const GH_GREEN: Color    = Color::Rgb(63, 185, 80);   // #3fb950 open
    const GH_PURPLE: Color   = Color::Rgb(137, 87, 229);  // #8957e5 merged
    const GH_RED: Color      = Color::Rgb(248, 81, 73);   // #f85149 closed

    f.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(GH_BLUE))
        .style(Style::default().bg(GH_BG))
        .title(Line::from(vec![
            Span::styled("  ", Style::default().bg(GH_BG)),
            Span::styled("⎇", Style::default().fg(GH_BLUE).bg(GH_BG)),
            Span::styled(
                "  Pull Requests  ",
                Style::default().fg(GH_TEXT).bg(GH_BG).add_modifier(Modifier::BOLD),
            ),
        ]));

    let inner = block.inner(modal_area);
    f.render_widget(block, modal_area);

    let pad = "  ";
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from("")); // top padding

    if prs.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("{pad}No pull requests found"),
            Style::default().fg(GH_MUTED).bg(GH_BG),
        )));
    } else {
        for (i, pr) in prs.iter().enumerate() {
            let selected = i == app.pr_list_selected;
            let marker = if selected { "▶ " } else { "  " };
            let (state_label, state_color) = match pr.state.as_str() {
                "open"   => ("● OPEN",   GH_GREEN),
                "closed" => ("✕ CLOSED", GH_RED),
                _        => ("⎇ MERGED", GH_PURPLE),
            };
            let bg = if selected { GH_BG_SEL } else { GH_BG };
            let title_fg = if selected { GH_TEXT } else { Color::Rgb(200, 207, 216) };
            // Reserve space for: pad + marker + "#NNNNN " + "  STATE_LABEL" + "  @user"
            let badge_w  = state_label.len() + 3;
            let user_w   = pr.user.len() + 4;
            let prefix_w = pad.len() + 2 + 7;
            let max_title = (inner.width as usize).saturating_sub(prefix_w + badge_w + user_w);
            let title: String = if pr.title.chars().count() > max_title {
                pr.title.chars().take(max_title.saturating_sub(1)).collect::<String>() + "…"
            } else {
                pr.title.clone()
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{pad}{marker}"),
                    Style::default().fg(GH_BLUE).bg(bg),
                ),
                Span::styled(
                    format!("#{:<5} ", pr.number),
                    Style::default().fg(GH_BLUE).bg(bg).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    title,
                    Style::default().fg(title_fg).bg(bg),
                ),
                Span::styled(
                    format!("  {}", state_label),
                    Style::default().fg(state_color).bg(bg).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  @{}", pr.user),
                    Style::default().fg(GH_MUTED).bg(bg),
                ),
            ]));
        }
    }

    lines.push(Line::from("")); // spacer
    let help = if prs.is_empty() {
        format!("{pad}Esc:Close")
    } else {
        format!("{pad}↑↓:Navigate  Enter:Open in browser  Esc:Close")
    };
    lines.push(Line::from(Span::styled(
        help,
        Style::default().fg(GH_MUTED).bg(GH_BG),
    )));

    f.render_widget(Paragraph::new(lines).style(Style::default().bg(GH_BG)), inner);
}

// ── Filter modal ────────────────────────────────────────────

fn draw_filter_modal(f: &mut Frame, app: &App) {
    let area = f.area();
    let filter_count = app.config.status_filters.len() as u16;

    let adding = app.mode == Mode::FilterAdding;
    let inner_h = filter_count + 2 + if adding { 2 } else { 0 } + 3;
    let height = (inner_h + 2).min(area.height.saturating_sub(4));
    let width = 52u16.min(area.width.saturating_sub(4));

    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let modal_area = Rect::new(x, y, width, height);

    f.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            " Status Filters ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(modal_area);
    f.render_widget(block, modal_area);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    for (i, sf) in app.config.status_filters.iter().enumerate() {
        let selected = i == app.filter_selected && !adding;
        let marker = if selected { "▶ " } else { "  " };

        // excluded = hidden from results → ✕ red
        // not excluded = visible in results → ✓ green
        let (icon, icon_color, name_style) = if sf.excluded {
            (
                "✕",
                Color::Red,
                if selected {
                    Style::default()
                        .fg(Color::Rgb(200, 130, 130))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Rgb(180, 100, 100))
                },
            )
        } else {
            (
                "✓",
                Color::Green,
                if selected {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Rgb(140, 200, 140))
                },
            )
        };

        lines.push(Line::from(vec![
            Span::styled(
                marker.to_string(),
                Style::default().fg(Color::Rgb(140, 200, 255)),
            ),
            Span::styled(format!("{icon} "), Style::default().fg(icon_color)),
            Span::styled(sf.name.clone(), name_style),
        ]));
    }

    if app.config.status_filters.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (no filters)",
            Style::default().fg(Color::DarkGray),
        )));
    }

    if adding {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  New: ", Style::default().fg(Color::Rgb(140, 200, 255))),
            Span::styled(
                visible_input(
                    &app.filter_input,
                    app.cursor_pos,
                    inner.width as usize - 7,
                ),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    lines.push(Line::from(""));

    if adding {
        lines.push(Line::from(Span::styled(
            "  Enter:Confirm  Esc:Cancel",
            Style::default().fg(Color::Rgb(100, 100, 120)),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "  ↑↓:Navigate  Space:Toggle  a:Add  d:Delete",
            Style::default().fg(Color::Rgb(100, 100, 120)),
        )));
        lines.push(Line::from(Span::styled(
            "  Enter:Apply & Refresh  Esc:Close",
            Style::default().fg(Color::Rgb(100, 100, 120)),
        )));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

// ── Ticket detail modal ─────────────────────────────────────

fn draw_detail_modal(f: &mut Frame, app: &App) {
    let detail = match &app.detail {
        Some(d) => d,
        None => return,
    };

    let area = f.area();
    let width = area.width.saturating_sub(6).min(120);
    let height = area.height.saturating_sub(4);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let modal_area = Rect::new(x, y, width, height);

    f.render_widget(Clear, modal_area);

    let (icon, icon_color) = issue_type_icon(&detail.issue_type);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Line::from(vec![
            Span::styled(" ", Style::default()),
            Span::styled(icon.to_string(), Style::default().fg(icon_color)),
            Span::styled(
                format!(" {} ", detail.key),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
        ]));

    let inner = block.inner(modal_area);
    f.render_widget(block, modal_area);

    let input_editing =
        app.mode == Mode::DetailAddingComment || app.mode == Mode::DetailEditingComment;
    let editing_summary = app.mode == Mode::DetailEditingSummary;
    let confirm_deleting = app.mode == Mode::DetailConfirmDelete;
    let picking_transition = app.mode == Mode::DetailTransition;
    let mention_count = app
        .mention
        .as_ref()
        .map(|m| m.candidates.len())
        .unwrap_or(0);
    let mention_rows = if app.mention.is_some() && mention_count > 0 {
        mention_count as u16
    } else {
        0
    };
    let bottom_reserve: u16 = if input_editing || editing_summary {
        4 + if input_editing { mention_rows } else { 0 }
    } else if confirm_deleting {
        2
    } else if picking_transition {
        (app.transitions.len() as u16 + 3).min(inner.height / 2)
    } else {
        1 + if !app.detail_status_msg.is_empty() { 1 } else { 0 }
    };

    let content_height = inner.height.saturating_sub(bottom_reserve);
    let content_area = Rect::new(inner.x, inner.y, inner.width, content_height);
    let bottom_area = Rect::new(
        inner.x,
        inner.y + content_height,
        inner.width,
        bottom_reserve,
    );

    let inner_w = inner.width as usize;

    // Build content lines
    let mut lines: Vec<Line> = Vec::new();

    // Summary
    for sub in word_wrap(&detail.summary, inner_w) {
        lines.push(Line::from(Span::styled(
            sub,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )));
    }
    lines.push(Line::from(""));

    // Description separator
    let rule_w = inner_w.min(80);
    let desc_label = "── Description ";
    let desc_rule_len = rule_w.saturating_sub(desc_label.len());
    lines.push(Line::from(Span::styled(
        format!("{}{}", desc_label, "─".repeat(desc_rule_len)),
        Style::default().fg(ACCENT),
    )));
    lines.push(Line::from(""));

    lines.extend(markdown_to_lines(&detail.description, inner_w));

    lines.push(Line::from(""));

    // Comments separator
    let comment_count = detail.comments.len();
    let comments_label = format!("── Comments ({comment_count}) ");
    let comments_rule_len = rule_w.saturating_sub(comments_label.len());
    lines.push(Line::from(Span::styled(
        format!("{}{}", comments_label, "─".repeat(comments_rule_len)),
        Style::default().fg(ACCENT),
    )));

    if detail.comments.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  No comments",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        let comment_w = inner_w.saturating_sub(4);
        let mut comment_offsets: Vec<usize> = Vec::new();
        for (i, comment) in detail.comments.iter().enumerate() {
            let is_selected = app.detail_comment_selected == Some(i);

            comment_offsets.push(lines.len());
            lines.push(Line::from(""));

            let marker = if is_selected { "▶ " } else { "  " };
            let num_label = format!("#{}", i + 1);

            lines.push(Line::from(vec![
                Span::styled(marker.to_string(), Style::default().fg(ACCENT)),
                Span::styled(
                    num_label,
                    Style::default()
                        .fg(Color::Rgb(180, 180, 200))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  ", Style::default()),
                Span::styled(
                    comment.author.clone(),
                    Style::default()
                        .fg(Color::Rgb(140, 200, 255))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  {}", comment.created),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));

            for md_line in markdown_to_lines(&comment.body, comment_w) {
                let mut prefixed: Vec<Span> =
                    vec![Span::styled("    ".to_string(), Style::default())];
                if is_selected {
                    for span in md_line.spans {
                        let mut style = span.style;
                        if style.fg.is_none()
                            || style.fg == Some(Color::Rgb(200, 200, 210))
                        {
                            style = style.fg(Color::White);
                        }
                        prefixed.push(Span::styled(span.content.into_owned(), style));
                    }
                } else {
                    prefixed.extend(md_line.spans.into_iter().map(|s| {
                        Span::styled(s.content.into_owned(), s.style)
                    }));
                }
                lines.push(Line::from(prefixed));
            }

            if i + 1 < detail.comments.len() {
                lines.push(Line::from(Span::styled(
                    "  ── ── ── ──",
                    Style::default().fg(Color::Rgb(60, 60, 80)),
                )));
            }
        }
        *app.detail_comment_offsets.borrow_mut() = comment_offsets;
    }

    lines.push(Line::from(""));

    // Build link map for mouse click handling
    {
        let mut link_map: Vec<Option<String>> = Vec::with_capacity(lines.len());
        for line in &lines {
            let mut found_url = None;
            for span in &line.spans {
                let text = span.content.as_ref();
                if text.starts_with("http://") || text.starts_with("https://") {
                    found_url = Some(text.to_string());
                    break;
                }
            }
            link_map.push(found_url);
        }
        *app.detail_link_map.borrow_mut() = link_map;
        app.detail_content_y.set(content_area.y);
        app.detail_content_height.set(content_area.height);
    }

    let total_lines = lines.len();

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0));

    f.render_widget(paragraph, content_area);

    // Scrollbar (only if content overflows)
    if total_lines > content_area.height as usize {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .thumb_style(Style::default().fg(ACCENT))
            .track_style(Style::default().fg(Color::Rgb(40, 40, 60)));
        let mut scrollbar_state = ScrollbarState::new(total_lines)
            .position(app.detail_scroll as usize);
        f.render_stateful_widget(scrollbar, content_area, &mut scrollbar_state);
    }

    app.detail_lines.set(total_lines);

    // Bottom area
    let mut bottom_lines: Vec<Line> = Vec::new();

    if input_editing {
        let label = if app.mode == Mode::DetailAddingComment {
            "New comment"
        } else {
            "Edit comment"
        };

        bottom_lines.push(Line::from(Span::styled(
            format!(
                "┌─ {label} {}",
                "─".repeat(inner_w.saturating_sub(label.len() + 4))
            ),
            Style::default().fg(Color::Rgb(100, 100, 140)),
        )));

        // Mention dropdown (between header and input line)
        if let Some(ref mention) = app.mention {
            if !mention.candidates.is_empty() {
                for (i, candidate) in mention.candidates.iter().enumerate() {
                    let selected = i == mention.selected;
                    let marker = if selected { "▶ " } else { "  " };
                    let name_style = if selected {
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Rgb(180, 180, 200))
                    };
                    bottom_lines.push(Line::from(vec![
                        Span::styled(
                            format!("\u{2502} {marker}"),
                            Style::default().fg(Color::Rgb(100, 100, 140)),
                        ),
                        Span::styled(candidate.display_name.clone(), name_style),
                    ]));
                }
                bottom_lines.push(Line::from(Span::styled(
                    format!("├{}", "─".repeat(inner_w.saturating_sub(1))),
                    Style::default().fg(Color::Rgb(100, 100, 140)),
                )));
            }
        }

        bottom_lines.push(Line::from(Span::styled(
            format!(
                "\u{2502} {}",
                visible_input(
                    &app.comment_input,
                    app.cursor_pos,
                    inner_w.saturating_sub(2),
                )
            ),
            Style::default().fg(Color::White),
        )));

        bottom_lines.push(Line::from(Span::styled(
            format!("└{}", "─".repeat(inner_w.saturating_sub(1))),
            Style::default().fg(Color::Rgb(100, 100, 140)),
        )));

        let help_text = if app.mention.is_some() {
            "↑↓:Navigate  Enter/Tab:Select  Esc:Cancel"
        } else {
            "Enter:Submit  Esc:Cancel  @:Mention user"
        };
        bottom_lines.push(Line::from(Span::styled(
            help_text,
            Style::default().fg(Color::Rgb(100, 100, 120)),
        )));
    } else if editing_summary {
        let label = "Edit summary";

        bottom_lines.push(Line::from(Span::styled(
            format!(
                "┌─ {label} {}",
                "─".repeat(inner_w.saturating_sub(label.len() + 4))
            ),
            Style::default().fg(Color::Rgb(100, 100, 140)),
        )));

        bottom_lines.push(Line::from(Span::styled(
            format!(
                "\u{2502} {}",
                visible_input(
                    &app.summary_input,
                    app.cursor_pos,
                    inner_w.saturating_sub(2),
                )
            ),
            Style::default().fg(Color::White),
        )));

        bottom_lines.push(Line::from(Span::styled(
            format!("└{}", "─".repeat(inner_w.saturating_sub(1))),
            Style::default().fg(Color::Rgb(100, 100, 140)),
        )));

        bottom_lines.push(Line::from(Span::styled(
            "Enter:Save  Esc:Cancel",
            Style::default().fg(Color::Rgb(100, 100, 120)),
        )));
    } else if confirm_deleting {
        let idx = app.detail_comment_selected.unwrap_or(0);
        let author = detail
            .comments
            .get(idx)
            .map(|c| c.author.as_str())
            .unwrap_or("?");

        bottom_lines.push(Line::from(vec![
            Span::styled(
                format!("Delete comment #{} by {}? ", idx + 1, author),
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "y:Yes  n:No",
                Style::default().fg(Color::Rgb(180, 180, 200)),
            ),
        ]));
        bottom_lines.push(Line::from(Span::styled(
            "Esc:Cancel",
            Style::default().fg(Color::Rgb(100, 100, 120)),
        )));
    } else if picking_transition {
        let label = "Transition to";
        bottom_lines.push(Line::from(Span::styled(
            format!(
                "┌─ {label} {}",
                "─".repeat(inner_w.saturating_sub(label.len() + 4))
            ),
            Style::default().fg(Color::Rgb(100, 100, 140)),
        )));

        let current_status = app.detail.as_ref().map(|d| d.status.as_str()).unwrap_or("");

        for (i, t) in app.transitions.iter().enumerate() {
            let selected = i == app.transition_selected;
            let is_current = t.to_status == current_status;
            let marker = if selected { "▶ " } else { "  " };
            let mut spans = vec![
                Span::styled(
                    format!("│ {marker}"),
                    Style::default().fg(Color::Rgb(100, 100, 140)),
                ),
                Span::styled(
                    t.name.clone(),
                    if selected {
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Rgb(180, 180, 200))
                    },
                ),
            ];
            if is_current {
                spans.push(Span::styled(
                    " (current)",
                    Style::default().fg(Color::DarkGray),
                ));
            }
            bottom_lines.push(Line::from(spans));
        }

        bottom_lines.push(Line::from(Span::styled(
            format!("└{}", "─".repeat(inner_w.saturating_sub(1))),
            Style::default().fg(Color::Rgb(100, 100, 140)),
        )));
        bottom_lines.push(Line::from(Span::styled(
            "↑↓:Navigate  Enter:Confirm  Esc:Cancel",
            Style::default().fg(Color::Rgb(100, 100, 120)),
        )));
    } else {
        if !app.detail_status_msg.is_empty() {
            let elapsed_ms = app.detail_status_set_at.elapsed().as_millis();
            let is_error = app.detail_status_msg.starts_with("Error");
            let fg = if is_error {
                Color::Rgb(220, 140, 140)
            } else {
                rainbow_color(elapsed_ms, 0.6, 0.7)
            };
            bottom_lines.push(Line::from(Span::styled(
                app.detail_status_msg.clone(),
                Style::default().fg(fg),
            )));
        }
        bottom_lines.push(Line::from(Span::styled(
            "↑↓:Scroll  n/p:Comment  y:Copy  l:Link  c:Add  e:Edit  x:Del  s:Summary  t:Transition  g:PRs  Enter:Browser  Esc:Close",
            Style::default().fg(Color::Rgb(100, 100, 120)),
        )));
    }

    f.render_widget(Paragraph::new(bottom_lines), bottom_area);
}

// ── Markdown-like rendering ─────────────────────────────────

fn word_wrap(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 || text.chars().count() <= max_width {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_w = 0;

    for word in text.split(' ') {
        let word_w = word.chars().count();
        if current.is_empty() {
            current = word.to_string();
            current_w = word_w;
        } else if current_w + 1 + word_w <= max_width {
            current.push(' ');
            current.push_str(word);
            current_w += 1 + word_w;
        } else {
            lines.push(std::mem::take(&mut current));
            current = word.to_string();
            current_w = word_w;
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

fn emit_prefixed_wrapped(
    out: &mut Vec<Line<'static>>,
    prefix_first: Vec<Span<'static>>,
    prefix_cont: Vec<Span<'static>>,
    content: &str,
    width: usize,
) {
    let prefix_w: usize = prefix_first
        .iter()
        .map(|s| s.content.chars().count())
        .sum();
    let avail = width.saturating_sub(prefix_w);
    let wrapped = word_wrap(content, avail);
    for (i, sub) in wrapped.iter().enumerate() {
        let prefix = if i == 0 {
            prefix_first.clone()
        } else {
            prefix_cont.clone()
        };
        let mut spans = prefix;
        spans.extend(parse_inline_markdown(sub));
        out.push(Line::from(spans));
    }
}

fn markdown_to_lines(text: &str, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;

    for raw_line in text.lines() {
        // Code fence toggle
        if raw_line.starts_with("```") {
            in_code_block = !in_code_block;
            let label = if in_code_block {
                let lang = raw_line.strip_prefix("```").unwrap_or("");
                if lang.is_empty() {
                    "───".to_string()
                } else {
                    format!("─── {lang} ───")
                }
            } else {
                "───".to_string()
            };
            lines.push(Line::from(Span::styled(
                label,
                Style::default().fg(Color::Rgb(80, 80, 100)),
            )));
            continue;
        }

        if in_code_block {
            lines.push(Line::from(Span::styled(
                format!("  {raw_line}"),
                Style::default().fg(Color::Rgb(130, 190, 130)),
            )));
            continue;
        }

        // Headings
        if raw_line.starts_with("### ") {
            for sub in word_wrap(&raw_line[4..], width) {
                lines.push(Line::from(Span::styled(
                    sub,
                    Style::default()
                        .fg(Color::Rgb(180, 180, 200))
                        .add_modifier(Modifier::BOLD),
                )));
            }
            continue;
        }
        if raw_line.starts_with("## ") {
            for sub in word_wrap(&raw_line[3..], width) {
                lines.push(Line::from(Span::styled(
                    sub,
                    Style::default()
                        .fg(ACCENT)
                        .add_modifier(Modifier::BOLD),
                )));
            }
            continue;
        }
        if raw_line.starts_with("# ") {
            for sub in word_wrap(&raw_line[2..], width) {
                lines.push(Line::from(Span::styled(
                    sub,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                )));
            }
            continue;
        }

        // Blockquote — may contain nested lists/headings
        if raw_line.starts_with("> ") {
            let inner = &raw_line[2..];
            let bar = || {
                Span::styled(
                    "│ ".to_string(),
                    Style::default().fg(Color::Rgb(80, 130, 180)),
                )
            };
            let bar_cont = || {
                Span::styled(
                    "│ ".to_string(),
                    Style::default().fg(Color::Rgb(80, 130, 180)),
                )
            };

            // Nested bullet list inside blockquote
            if inner.starts_with("  - ") {
                emit_prefixed_wrapped(
                    &mut lines,
                    vec![
                        bar(),
                        Span::styled("• ".to_string(), Style::default().fg(ACCENT)),
                    ],
                    vec![
                        bar_cont(),
                        Span::styled("  ".to_string(), Style::default()),
                    ],
                    &inner[4..],
                    width,
                );
            // Nested numbered list inside blockquote
            } else if let Some((num, item_text)) = try_parse_numbered_item(inner) {
                let num_prefix = format!("{num}. ");
                let pad = " ".repeat(num_prefix.len());
                emit_prefixed_wrapped(
                    &mut lines,
                    vec![
                        bar(),
                        Span::styled(num_prefix, Style::default().fg(ACCENT)),
                    ],
                    vec![
                        bar_cont(),
                        Span::styled(pad, Style::default()),
                    ],
                    item_text,
                    width,
                );
            // Plain blockquote text
            } else {
                emit_prefixed_wrapped(
                    &mut lines,
                    vec![bar()],
                    vec![bar_cont()],
                    inner,
                    width,
                );
            }
            continue;
        }

        // Bullet list
        if raw_line.starts_with("  - ") {
            emit_prefixed_wrapped(
                &mut lines,
                vec![
                    Span::styled("  ".to_string(), Style::default()),
                    Span::styled("• ".to_string(), Style::default().fg(ACCENT)),
                ],
                vec![Span::styled("    ".to_string(), Style::default())],
                &raw_line[4..],
                width,
            );
            continue;
        }

        // Numbered list
        if let Some((num, item_text)) = try_parse_numbered_item(raw_line) {
            let num_prefix = format!("{num}. ");
            let pad = " ".repeat(2 + num_prefix.len());
            emit_prefixed_wrapped(
                &mut lines,
                vec![
                    Span::styled("  ".to_string(), Style::default()),
                    Span::styled(num_prefix, Style::default().fg(ACCENT)),
                ],
                vec![Span::styled(pad, Style::default())],
                item_text,
                width,
            );
            continue;
        }

        // Horizontal rule
        if raw_line.starts_with("────") {
            lines.push(Line::from(Span::styled(
                raw_line.to_string(),
                Style::default().fg(Color::Rgb(60, 60, 80)),
            )));
            continue;
        }

        // Regular text with word wrap + inline formatting
        let wrapped = word_wrap(raw_line, width);
        for sub in &wrapped {
            lines.push(Line::from(parse_inline_markdown(sub)));
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(""));
    }

    lines
}

fn try_parse_numbered_item(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.strip_prefix("  ")?;
    let dot_pos = trimmed.find(". ")?;
    let num = &trimmed[..dot_pos];
    if !num.is_empty() && num.len() <= 4 && num.chars().all(|c| c.is_ascii_digit()) {
        Some((num, &trimmed[dot_pos + 2..]))
    } else {
        None
    }
}

fn parse_inline_markdown(text: &str) -> Vec<Span<'static>> {
    let body_style = Style::default().fg(Color::Rgb(200, 200, 210));

    if text.is_empty() {
        return vec![Span::styled(String::new(), body_style)];
    }

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        let url_pos = [remaining.find("https://"), remaining.find("http://")]
            .into_iter()
            .flatten()
            .min();

        let candidates: Vec<(usize, u8)> = [
            remaining.find("**").map(|p| (p, 0u8)),
            remaining.find('`').map(|p| (p, 1)),
            remaining.find('[').map(|p| (p, 2)),
            url_pos.map(|p| (p, 3)),
        ]
        .into_iter()
        .flatten()
        .collect();

        let nearest = candidates.iter().min_by_key(|(p, _)| *p);

        match nearest {
            None => {
                spans.push(Span::styled(remaining.to_string(), body_style));
                break;
            }
            Some(&(pos, marker_type)) => {
                if pos > 0 {
                    spans.push(Span::styled(remaining[..pos].to_string(), body_style));
                }

                match marker_type {
                    0 => {
                        // **bold**
                        let after = &remaining[pos + 2..];
                        if let Some(end) = after.find("**") {
                            spans.push(Span::styled(
                                after[..end].to_string(),
                                Style::default()
                                    .fg(Color::White)
                                    .add_modifier(Modifier::BOLD),
                            ));
                            remaining = &after[end + 2..];
                        } else {
                            spans.push(Span::styled("**".to_string(), body_style));
                            remaining = after;
                        }
                    }
                    1 => {
                        // `code`
                        let after = &remaining[pos + 1..];
                        if let Some(end) = after.find('`') {
                            spans.push(Span::styled(
                                after[..end].to_string(),
                                Style::default().fg(Color::Rgb(130, 190, 130)),
                            ));
                            remaining = &after[end + 1..];
                        } else {
                            spans.push(Span::styled("`".to_string(), body_style));
                            remaining = after;
                        }
                    }
                    2 => {
                        // [text](url)
                        let after = &remaining[pos + 1..];
                        let mut found = false;
                        if let Some(bracket_end) = after.find("](") {
                            let link_text = &after[..bracket_end];
                            let url_part = &after[bracket_end + 2..];
                            if let Some(paren_end) = url_part.find(')') {
                                let url = &url_part[..paren_end];
                                let link_style = Style::default()
                                    .fg(Color::Rgb(100, 180, 255))
                                    .add_modifier(Modifier::UNDERLINED);
                                if link_text == url || link_text.is_empty() {
                                    spans.push(Span::styled(url.to_string(), link_style));
                                } else {
                                    spans.push(Span::styled(
                                        link_text.to_string(),
                                        link_style,
                                    ));
                                }
                                remaining = &url_part[paren_end + 1..];
                                found = true;
                            }
                        }
                        if !found {
                            spans.push(Span::styled("[".to_string(), body_style));
                            remaining = after;
                        }
                    }
                    3 => {
                        // Bare URL (https:// or http://)
                        let url_text = &remaining[pos..];
                        let end = url_text
                            .find(|c: char| c.is_whitespace())
                            .unwrap_or(url_text.len());
                        let raw_url = &url_text[..end];
                        let url = raw_url.trim_end_matches(|c: char| {
                            matches!(c, '.' | ',' | ')' | ';' | ':' | '!' | '?')
                        });
                        let link_style = Style::default()
                            .fg(Color::Rgb(100, 180, 255))
                            .add_modifier(Modifier::UNDERLINED);
                        spans.push(Span::styled(url.to_string(), link_style));
                        remaining = &remaining[pos + url.len()..];
                    }
                    _ => unreachable!(),
                }
            }
        }
    }

    if spans.is_empty() {
        spans.push(Span::styled(String::new(), body_style));
    }

    spans
}

// ── Long note modal ─────────────────────────────────────────

fn draw_long_note_modal(f: &mut Frame, app: &App) {
    let key = app
        .rows
        .get(app.selected)
        .map(|r| r.issue.key.as_str())
        .unwrap_or("");

    let area = f.area();
    let width = area.width.saturating_sub(10).min(90);
    let height = area.height.saturating_sub(6).min(30);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let modal_area = Rect::new(x, y, width, height);

    f.render_widget(Clear, modal_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Line::from(vec![
            Span::styled(
                format!(" Notes: {key} "),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
        ]));

    let inner = block.inner(modal_area);
    f.render_widget(block, modal_area);

    // Reserve bottom line for help bar
    let help_h: u16 = 1;
    let edit_height = inner.height.saturating_sub(help_h);
    let edit_area = Rect::new(inner.x, inner.y, inner.width, edit_height);
    let help_area = Rect::new(inner.x, inner.y + edit_height, inner.width, help_h);

    // Render the text with cursor
    let text = &app.long_note_input;
    let cursor_pos = app.cursor_pos.min(text.len());

    // Build display lines from text, inserting a visible cursor marker
    let text_lines: Vec<&str> = if text.is_empty() { vec![""] } else { text.split('\n').collect() };

    // Figure out which line and column the cursor is on
    let mut cursor_line = 0usize;
    let mut cursor_col = 0usize;
    {
        let mut pos = 0usize;
        for (i, line) in text_lines.iter().enumerate() {
            let line_end = pos + line.len();
            if cursor_pos <= line_end {
                cursor_line = i;
                cursor_col = cursor_pos - pos;
                break;
            }
            pos = line_end + 1; // +1 for '\n'
            cursor_line = i + 1;
        }
    }

    // Auto-scroll to keep cursor visible
    let visible_h = edit_height as usize;
    let scroll = if visible_h == 0 {
        0
    } else if cursor_line < app.long_note_scroll {
        cursor_line
    } else if cursor_line >= app.long_note_scroll + visible_h {
        cursor_line.saturating_sub(visible_h - 1)
    } else {
        app.long_note_scroll
    };

    let inner_w = inner.width as usize;
    let mut display_lines: Vec<Line> = Vec::new();
    for (i, line_text) in text_lines.iter().enumerate().skip(scroll).take(visible_h) {
        if i == cursor_line {
            // Show cursor on this line
            let before = &line_text[..cursor_col.min(line_text.len())];
            let after = &line_text[cursor_col.min(line_text.len())..];
            let cursor_char = if after.is_empty() { " " } else { &after[..1] };
            let rest = if after.is_empty() { "" } else { &after[1..] };

            let mut spans = Vec::new();
            if !before.is_empty() {
                spans.push(Span::styled(
                    truncate(before, inner_w),
                    Style::default().fg(Color::White),
                ));
            }
            spans.push(Span::styled(
                cursor_char.to_string(),
                Style::default().fg(Color::Black).bg(Color::White),
            ));
            if !rest.is_empty() {
                spans.push(Span::styled(
                    rest.to_string(),
                    Style::default().fg(Color::White),
                ));
            }
            display_lines.push(Line::from(spans));
        } else {
            display_lines.push(Line::from(Span::styled(
                line_text.to_string(),
                Style::default().fg(Color::Rgb(200, 200, 210)),
            )));
        }
    }

    f.render_widget(Paragraph::new(display_lines), edit_area);

    // Help bar
    let has_note = app.long_notes.contains_key(key);
    let indicator = if has_note { " (has saved note)" } else { "" };
    let help_line = Line::from(Span::styled(
        format!("Ctrl+S:Save  Esc:Cancel{indicator}"),
        Style::default().fg(Color::Rgb(100, 100, 120)),
    ));
    f.render_widget(Paragraph::new(help_line), help_area);
}

// ── Color helpers ───────────────────────────────────────────

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> Color {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_prime = h / 60.0;
    let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match h_prime as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        5 => (c, 0.0, x),
        _ => (0.0, 0.0, 0.0),
    };
    let m = l - c / 2.0;
    Color::Rgb(
        ((r1 + m) * 255.0) as u8,
        ((g1 + m) * 255.0) as u8,
        ((b1 + m) * 255.0) as u8,
    )
}

fn rainbow_color(elapsed_ms: u128, saturation: f64, lightness: f64) -> Color {
    let hue = (elapsed_ms % 1500) as f64 * 360.0 / 1500.0;
    hsl_to_rgb(hue, saturation, lightness)
}

// ── Status bar ──────────────────────────────────────────────

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let tree_label = if app.show_all_parents {
        "p:Tree[ON]"
    } else {
        "p:Tree[OFF]"
    };

    let (mode_text, help_text) = match app.mode {
        Mode::Normal => (
            Span::styled(
                " NORMAL ",
                Style::default()
                    .bg(Color::Rgb(60, 60, 120))
                    .fg(Color::White),
            ),
            format!(
                " q:Quit  j/k:Nav  Enter:Open  w:Browser  s:Status  n:Notes  h:Highlight  m:Mute  o:Sort  c:Columns  y:Copy  f:Filter  /:Search  {tree_label}  r:Refresh  ?:Legend "
            ),
        ),
        Mode::Searching => (
            Span::styled(
                " SEARCH ",
                Style::default()
                    .bg(Color::Rgb(180, 160, 40))
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            " Type to filter  ↑↓:Navigate  Enter:Keep filter  Esc:Clear ".to_string(),
        ),
        Mode::ConfirmBrowser => (
            Span::styled(
                " OPEN IN BROWSER ",
                Style::default()
                    .bg(Color::Rgb(180, 130, 50))
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            " y/Enter:Confirm  n/Esc:Cancel ".to_string(),
        ),
        Mode::EditingNote => (
            Span::styled(
                " EDIT STATUS ",
                Style::default()
                    .bg(Color::Green)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            " Enter:Save  Esc:Cancel ".to_string(),
        ),
        Mode::EditingLongNote => (
            Span::styled(
                " EDIT NOTES ",
                Style::default()
                    .bg(Color::Rgb(80, 160, 80))
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            " Ctrl+S:Save  Esc:Cancel  Enter:Newline ".to_string(),
        ),
        Mode::FilterEditor | Mode::FilterAdding => (
            Span::styled(
                " FILTER ",
                Style::default()
                    .bg(Color::Rgb(180, 130, 50))
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            " Editing status filters... ".to_string(),
        ),
        Mode::TicketDetail => (
            Span::styled(
                " DETAIL ",
                Style::default()
                    .bg(Color::Rgb(80, 120, 180))
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            " ↑↓:Scroll  n/N:Select comment  c:Add  e:Edit  x:Del comment  Enter:Browser  Esc:Close "
                .to_string(),
        ),
        Mode::DetailAddingComment => (
            Span::styled(
                " ADD COMMENT ",
                Style::default()
                    .bg(Color::Green)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            " Enter:Submit  Esc:Cancel ".to_string(),
        ),
        Mode::DetailEditingComment => (
            Span::styled(
                " EDIT COMMENT ",
                Style::default()
                    .bg(Color::Rgb(180, 130, 50))
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            " Enter:Submit  Esc:Cancel ".to_string(),
        ),
        Mode::DetailConfirmDelete => (
            Span::styled(
                " DELETE COMMENT ",
                Style::default()
                    .bg(Color::Red)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            " y:Confirm  n/Esc:Cancel ".to_string(),
        ),
        Mode::DetailTransition => (
            Span::styled(
                " TRANSITION ",
                Style::default()
                    .bg(Color::Rgb(80, 160, 80))
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            " ↑↓:Navigate  Enter:Confirm  Esc:Cancel ".to_string(),
        ),
        Mode::DetailConfirmTransition => (
            Span::styled(
                " CONFIRM TRANSITION ",
                Style::default()
                    .bg(Color::Rgb(180, 130, 50))
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            " y/Enter:Confirm  n/Esc:Cancel ".to_string(),
        ),
        Mode::DetailEditingSummary => (
            Span::styled(
                " EDIT SUMMARY ",
                Style::default()
                    .bg(Color::Rgb(180, 130, 50))
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            " Enter:Save  Esc:Cancel ".to_string(),
        ),
        Mode::HighlightPicker => (
            Span::styled(
                " HIGHLIGHT ",
                Style::default()
                    .bg(Color::Rgb(230, 150, 30))
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            " ↑↓:Navigate  Enter:Select  Esc:Cancel ".to_string(),
        ),
        Mode::SortPicker => (
            Span::styled(
                " SORT ",
                Style::default()
                    .bg(Color::Rgb(100, 160, 200))
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            " ↑↓:Navigate  Enter:Select  Esc:Cancel ".to_string(),
        ),
        Mode::ColumnPicker => (
            Span::styled(
                " COLUMNS ",
                Style::default()
                    .bg(Color::Rgb(100, 160, 200))
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
            " ↑↓:Navigate  Space:Toggle  Esc:Close ".to_string(),
        ),
        Mode::DetailPRList => (
            Span::styled(
                " PULL REQUESTS ",
                Style::default()
                    .bg(Color::Rgb(60, 120, 80))
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            " ↑↓:Navigate  Enter:Open in browser  Esc:Close ".to_string(),
        ),
        Mode::ConfirmQuit => (
            Span::styled(
                " QUIT ",
                Style::default()
                    .bg(Color::Rgb(180, 60, 60))
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            " y/Enter:Quit  n/Esc:Cancel ".to_string(),
        ),
    };

    let status_spans: Vec<Span> = if app.status_msg.is_empty() {
        vec![]
    } else {
        let elapsed = app.status_set_at.elapsed();
        let elapsed_ms = elapsed.as_millis();
        let is_error = app.status_msg.starts_with("Error");

        let fg = if is_error {
            Color::Rgb(220, 140, 140)
        } else {
            rainbow_color(elapsed_ms, 0.6, 0.7)
        };
        vec![
            Span::styled(" ", Style::default()),
            Span::styled(app.status_msg.clone(), Style::default().fg(fg)),
        ]
    };

    let mut spans = vec![
        mode_text,
        Span::styled(help_text, Style::default().fg(Color::Rgb(120, 120, 140))),
    ];
    spans.extend(status_spans);
    let line = Line::from(spans);

    f.render_widget(Paragraph::new(line), area);
}

fn draw_search_bar(f: &mut Frame, app: &App, area: Rect) {
    let cursor = if app.mode == Mode::Searching { "│" } else { "" };
    let line = Line::from(vec![
        Span::styled(
            " /",
            Style::default()
                .fg(Color::Rgb(255, 200, 60))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            app.search_input.clone(),
            Style::default().fg(Color::White),
        ),
        Span::styled(cursor.to_string(), Style::default().fg(Color::Rgb(255, 200, 60))),
        Span::styled(
            format!("  ({} matches)", app.rows.len()),
            Style::default().fg(Color::Rgb(100, 100, 120)),
        ),
    ]);

    f.render_widget(Paragraph::new(line).style(Style::default().bg(Color::Rgb(25, 25, 35))), area);
}
