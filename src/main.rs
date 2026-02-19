mod app;
mod config;
mod jira;
mod notes;
mod ui;

use std::io;
use std::io::Write;
use std::time::Duration;

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    MouseButton, MouseEventKind,
};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::{App, Mode};
use config::Config;

fn char_byte_pos(s: &str, char_pos: usize) -> usize {
    s.char_indices()
        .nth(char_pos)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

fn input_insert(s: &mut String, cursor: &mut usize, c: char) {
    let bp = char_byte_pos(s, *cursor);
    s.insert(bp, c);
    *cursor += 1;
}

fn input_backspace(s: &mut String, cursor: &mut usize) {
    if *cursor > 0 {
        *cursor -= 1;
        let bp = char_byte_pos(s, *cursor);
        s.remove(bp);
    }
}

fn input_delete(s: &mut String, cursor: &mut usize) {
    if *cursor < s.chars().count() {
        let bp = char_byte_pos(s, *cursor);
        s.remove(bp);
    }
}

fn prompt(label: &str, default: &str) -> String {
    if default.is_empty() {
        print!("{label}: ");
    } else {
        print!("{label} [{default}]: ");
    }
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let input = input.trim().to_string();
    if input.is_empty() { default.to_string() } else { input }
}

fn run_setup() {
    let existing = Config::load().ok();

    if let Some(ref config) = existing {
        println!("Existing config found at {}\n", config::config_dir().join("config.json").display());
        println!("  Jira URL:  {}", config.jira_url);
        println!("  Email:     {}", config.email);
        println!("  API token: {}...", &config.api_token[..config.api_token.len().min(8)]);
        println!();

        let choice = prompt("(c)reate new, (d)elete, or (k)eep?", "k");
        match choice.chars().next().unwrap_or('k') {
            'd' => {
                let path = config::config_dir().join("config.json");
                let _ = std::fs::remove_file(&path);
                println!("Config deleted.");
                return;
            }
            'c' => {} // fall through to prompts below
            _ => {
                println!("Config unchanged.");
                return;
            }
        }
    }

    println!("Mindful Jira setup\n");

    let jira_url = prompt("Jira URL", existing.as_ref().map_or("", |c| &c.jira_url));
    let email = prompt("Email", existing.as_ref().map_or("", |c| &c.email));
    let api_token = prompt("API token", existing.as_ref().map_or("", |c| &c.api_token));

    let sort_order = existing.as_ref().and_then(|c| c.sort_order.clone());
    let status_filters = existing
        .map(|c| c.status_filters)
        .unwrap_or_else(config::default_status_filters);

    let config = Config {
        jira_url,
        email,
        api_token,
        status_filters,
        sort_order,
    };
    config.save();

    println!("\nConfig saved to {}", config::config_dir().join("config.json").display());
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("setup") => {
            run_setup();
            return Ok(());
        }
        Some("--version" | "-v") => {
            println!("mindful-jira {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        _ => {}
    }

    let config = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    io::stdout().execute(EnableMouseCapture)?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(config);
    app.init().await;
    app.refresh().await;
    app.status_msg.clear();

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        // Auto-clear stale status messages
        if !app.status_msg.is_empty() && app.status_set_at.elapsed() > Duration::from_secs(5) {
            app.status_msg.clear();
        }
        if !app.detail_status_msg.is_empty()
            && app.detail_status_set_at.elapsed() > Duration::from_secs(5)
        {
            app.detail_status_msg.clear();
        }

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    match app.mode {
                        Mode::ConfirmQuit => match key.code {
                            KeyCode::Char('y') | KeyCode::Enter => break,
                            KeyCode::Char('n') | KeyCode::Esc => app.cancel_quit(),
                            _ => {}
                        },
                        Mode::Normal => match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => app.confirm_quit(),
                            KeyCode::Up | KeyCode::Char('k') => app.move_up(),
                            KeyCode::Down | KeyCode::Char('j') => app.move_down(),
                            KeyCode::Enter => app.open_ticket_detail().await,
                            KeyCode::Char('w') => app.confirm_open_in_browser(),
                            KeyCode::Char('s') => app.start_editing_status(),
                            KeyCode::Char('n') => app.start_editing_long_note(),
                            KeyCode::Char('h') => app.open_highlight_picker(),
                            KeyCode::Char('m') => app.toggle_mute(),
                            KeyCode::Char('y') => app.copy_key_to_clipboard(),
                            KeyCode::Char('f') => app.open_filter_editor(),
                            KeyCode::Char('/') => app.start_search(),
                            KeyCode::Char('p') => app.toggle_show_all_parents().await,
                            KeyCode::Char('o') => app.open_sort_picker(),
                            KeyCode::Char('r') => app.refresh().await,
                            KeyCode::Char('?') => app.show_legend = !app.show_legend,
                            _ => {}
                        },
                        Mode::Searching => match key.code {
                            KeyCode::Esc => app.clear_search(),
                            KeyCode::Enter => app.confirm_search(),
                            KeyCode::Up | KeyCode::Char('\x1b') => app.move_up(),
                            KeyCode::Down => app.move_down(),
                            KeyCode::Backspace => {
                                app.search_input.pop();
                                app.apply_search_filter();
                            }
                            KeyCode::Char(c) => {
                                app.search_input.push(c);
                                app.apply_search_filter();
                            }
                            _ => {}
                        },
                        Mode::ConfirmBrowser => match key.code {
                            KeyCode::Char('y') | KeyCode::Enter => app.open_in_browser(),
                            KeyCode::Char('n') | KeyCode::Esc => app.cancel_browser(),
                            _ => {}
                        },
                        Mode::TicketDetail => match key.code {
                            KeyCode::Esc => app.close_detail(),
                            KeyCode::Enter => app.detail_open_in_browser(),
                            KeyCode::Up | KeyCode::Char('k') => app.detail_scroll_up(),
                            KeyCode::Down | KeyCode::Char('j') => app.detail_scroll_down(),
                            KeyCode::Char('n') => app.detail_next_comment(),
                            KeyCode::Char('p') => app.detail_prev_comment(),
                            KeyCode::Char('y') => app.copy_ticket_to_clipboard(),
                            KeyCode::Char('l') => app.copy_link_to_clipboard(),
                            KeyCode::Char('c') => app.start_adding_comment(),
                            KeyCode::Char('e') => app.start_editing_comment(),
                            KeyCode::Char('x') => app.confirm_delete_comment(),
                            KeyCode::Char('t') => app.open_transition_picker().await,
                            KeyCode::Char('s') => app.start_editing_summary(),
                            KeyCode::Char('?') => app.show_legend = !app.show_legend,
                            _ => {}
                        },
                        Mode::DetailTransition => match key.code {
                            KeyCode::Esc => app.cancel_transition(),
                            KeyCode::Up | KeyCode::Char('k') => app.transition_move_up(),
                            KeyCode::Down | KeyCode::Char('j') => app.transition_move_down(),
                            KeyCode::Enter => app.confirm_transition(),
                            _ => {}
                        },
                        Mode::DetailConfirmTransition => match key.code {
                            KeyCode::Char('y') | KeyCode::Enter => app.execute_transition().await,
                            KeyCode::Char('n') | KeyCode::Esc => app.cancel_confirm_transition(),
                            _ => {}
                        },
                        Mode::DetailAddingComment | Mode::DetailEditingComment => {
                            if app.mention.is_some() {
                                // Mention overlay active
                                match key.code {
                                    KeyCode::Up => app.mention_move_up(),
                                    KeyCode::Down => app.mention_move_down(),
                                    KeyCode::Enter | KeyCode::Tab => {
                                        app.select_mention();
                                    }
                                    KeyCode::Esc => app.cancel_mention(),
                                    KeyCode::Backspace => {
                                        let trigger_pos = app.mention.as_ref().map(|m| m.trigger_pos).unwrap_or(0);
                                        input_backspace(
                                            &mut app.comment_input,
                                            &mut app.cursor_pos,
                                        );
                                        // Cancel mention if cursor retreated past '@'
                                        if app.cursor_pos < trigger_pos {
                                            app.cancel_mention();
                                        } else {
                                            app.update_mention_query();
                                            app.fetch_mention_candidates().await;
                                        }
                                    }
                                    KeyCode::Char(' ') => {
                                        app.cancel_mention();
                                        input_insert(
                                            &mut app.comment_input,
                                            &mut app.cursor_pos,
                                            ' ',
                                        );
                                    }
                                    KeyCode::Char(c) => {
                                        input_insert(
                                            &mut app.comment_input,
                                            &mut app.cursor_pos,
                                            c,
                                        );
                                        app.update_mention_query();
                                        app.fetch_mention_candidates().await;
                                    }
                                    _ => {}
                                }
                            } else {
                                // Normal comment editing
                                match key.code {
                                    KeyCode::Enter => {
                                        if app.mode == Mode::DetailAddingComment {
                                            app.submit_comment().await;
                                        } else {
                                            app.save_edited_comment().await;
                                        }
                                    }
                                    KeyCode::Esc => app.cancel_comment_action(),
                                    KeyCode::Left => {
                                        if app.cursor_pos > 0 {
                                            app.cursor_pos -= 1;
                                        }
                                    }
                                    KeyCode::Right => {
                                        if app.cursor_pos < app.comment_input.chars().count() {
                                            app.cursor_pos += 1;
                                        }
                                    }
                                    KeyCode::Home => app.cursor_pos = 0,
                                    KeyCode::End => {
                                        app.cursor_pos = app.comment_input.chars().count()
                                    }
                                    KeyCode::Backspace => {
                                        input_backspace(
                                            &mut app.comment_input,
                                            &mut app.cursor_pos,
                                        );
                                        app.invalidate_overlapping_mentions();
                                    }
                                    KeyCode::Delete => {
                                        input_delete(
                                            &mut app.comment_input,
                                            &mut app.cursor_pos,
                                        );
                                        app.invalidate_overlapping_mentions();
                                    }
                                    KeyCode::Char('@') => {
                                        input_insert(
                                            &mut app.comment_input,
                                            &mut app.cursor_pos,
                                            '@',
                                        );
                                        app.activate_mention();
                                    }
                                    KeyCode::Char(c) => {
                                        input_insert(
                                            &mut app.comment_input,
                                            &mut app.cursor_pos,
                                            c,
                                        );
                                        app.invalidate_overlapping_mentions();
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Mode::DetailConfirmDelete => match key.code {
                            KeyCode::Char('y') => app.execute_delete_comment().await,
                            KeyCode::Char('n') | KeyCode::Esc => app.cancel_comment_action(),
                            _ => {}
                        },
                        Mode::DetailEditingSummary => match key.code {
                            KeyCode::Enter => app.save_summary().await,
                            KeyCode::Esc => app.cancel_editing_summary(),
                            KeyCode::Left => {
                                if app.cursor_pos > 0 {
                                    app.cursor_pos -= 1;
                                }
                            }
                            KeyCode::Right => {
                                if app.cursor_pos < app.summary_input.chars().count() {
                                    app.cursor_pos += 1;
                                }
                            }
                            KeyCode::Home => app.cursor_pos = 0,
                            KeyCode::End => {
                                app.cursor_pos = app.summary_input.chars().count()
                            }
                            KeyCode::Backspace => {
                                input_backspace(
                                    &mut app.summary_input,
                                    &mut app.cursor_pos,
                                );
                            }
                            KeyCode::Delete => {
                                input_delete(
                                    &mut app.summary_input,
                                    &mut app.cursor_pos,
                                );
                            }
                            KeyCode::Char(c) => {
                                input_insert(
                                    &mut app.summary_input,
                                    &mut app.cursor_pos,
                                    c,
                                );
                            }
                            _ => {}
                        },
                        Mode::EditingLongNote => {
                            if key.modifiers.contains(KeyModifiers::CONTROL)
                                && key.code == KeyCode::Char('s')
                            {
                                app.save_long_note();
                            } else {
                                match key.code {
                                    KeyCode::Esc => app.cancel_long_note(),
                                    KeyCode::Enter => {
                                        let bp = app.cursor_pos.min(app.long_note_input.len());
                                        app.long_note_input.insert(bp, '\n');
                                        app.cursor_pos = bp + 1;
                                    }
                                    KeyCode::Left => {
                                        if app.cursor_pos > 0 {
                                            app.cursor_pos -= 1;
                                        }
                                    }
                                    KeyCode::Right => {
                                        if app.cursor_pos < app.long_note_input.len() {
                                            app.cursor_pos += 1;
                                        }
                                    }
                                    KeyCode::Up => {
                                        // Move cursor up one line
                                        let text = &app.long_note_input[..app.cursor_pos];
                                        if let Some(nl) = text.rfind('\n') {
                                            let col = app.cursor_pos - nl - 1;
                                            let prev_line_start = text[..nl].rfind('\n').map(|p| p + 1).unwrap_or(0);
                                            let prev_line_len = nl - prev_line_start;
                                            app.cursor_pos = prev_line_start + col.min(prev_line_len);
                                        }
                                    }
                                    KeyCode::Down => {
                                        // Move cursor down one line
                                        let text = &app.long_note_input;
                                        if let Some(nl) = text[app.cursor_pos..].find('\n') {
                                            let line_start = text[..app.cursor_pos].rfind('\n').map(|p| p + 1).unwrap_or(0);
                                            let col = app.cursor_pos - line_start;
                                            let next_line_start = app.cursor_pos + nl + 1;
                                            let next_line_end = text[next_line_start..].find('\n').map(|p| next_line_start + p).unwrap_or(text.len());
                                            let next_line_len = next_line_end - next_line_start;
                                            app.cursor_pos = next_line_start + col.min(next_line_len);
                                        }
                                    }
                                    KeyCode::Home => {
                                        let line_start = app.long_note_input[..app.cursor_pos].rfind('\n').map(|p| p + 1).unwrap_or(0);
                                        app.cursor_pos = line_start;
                                    }
                                    KeyCode::End => {
                                        let line_end = app.long_note_input[app.cursor_pos..].find('\n').map(|p| app.cursor_pos + p).unwrap_or(app.long_note_input.len());
                                        app.cursor_pos = line_end;
                                    }
                                    KeyCode::Backspace => {
                                        if app.cursor_pos > 0 {
                                            app.cursor_pos -= 1;
                                            app.long_note_input.remove(app.cursor_pos);
                                        }
                                    }
                                    KeyCode::Delete => {
                                        if app.cursor_pos < app.long_note_input.len() {
                                            app.long_note_input.remove(app.cursor_pos);
                                        }
                                    }
                                    KeyCode::Char(c) => {
                                        let bp = app.cursor_pos.min(app.long_note_input.len());
                                        app.long_note_input.insert(bp, c);
                                        app.cursor_pos = bp + 1;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Mode::EditingNote => match key.code {
                            KeyCode::Enter => app.save_status(),
                            KeyCode::Esc => app.cancel_edit(),
                            KeyCode::Left => {
                                if app.cursor_pos > 0 {
                                    app.cursor_pos -= 1;
                                }
                            }
                            KeyCode::Right => {
                                if app.cursor_pos < app.note_input.chars().count() {
                                    app.cursor_pos += 1;
                                }
                            }
                            KeyCode::Home => app.cursor_pos = 0,
                            KeyCode::End => {
                                app.cursor_pos = app.note_input.chars().count()
                            }
                            KeyCode::Backspace => {
                                input_backspace(&mut app.note_input, &mut app.cursor_pos);
                            }
                            KeyCode::Delete => {
                                input_delete(&mut app.note_input, &mut app.cursor_pos);
                            }
                            KeyCode::Char(c) => {
                                input_insert(&mut app.note_input, &mut app.cursor_pos, c);
                            }
                            _ => {}
                        },
                        Mode::HighlightPicker => match key.code {
                            KeyCode::Esc => app.cancel_highlight_picker(),
                            KeyCode::Up | KeyCode::Char('k') => app.highlight_picker_up(),
                            KeyCode::Down | KeyCode::Char('j') => app.highlight_picker_down(),
                            KeyCode::Enter => app.apply_highlight(),
                            _ => {}
                        },
                        Mode::SortPicker => match key.code {
                            KeyCode::Esc => app.cancel_sort_picker(),
                            KeyCode::Up | KeyCode::Char('k') => app.sort_picker_up(),
                            KeyCode::Down | KeyCode::Char('j') => app.sort_picker_down(),
                            KeyCode::Enter => app.apply_sort(),
                            _ => {}
                        },
                        Mode::FilterEditor => match key.code {
                            KeyCode::Esc => app.close_filter_editor(),
                            KeyCode::Enter => app.apply_filters_and_refresh().await,
                            KeyCode::Up | KeyCode::Char('k') => app.filter_move_up(),
                            KeyCode::Down | KeyCode::Char('j') => app.filter_move_down(),
                            KeyCode::Char(' ') => app.toggle_filter(),
                            KeyCode::Char('a') => app.start_adding_filter(),
                            KeyCode::Char('d') | KeyCode::Delete => app.delete_filter(),
                            _ => {}
                        },
                        Mode::FilterAdding => match key.code {
                            KeyCode::Enter => app.confirm_add_filter(),
                            KeyCode::Esc => app.cancel_add_filter(),
                            KeyCode::Left => {
                                if app.cursor_pos > 0 {
                                    app.cursor_pos -= 1;
                                }
                            }
                            KeyCode::Right => {
                                if app.cursor_pos < app.filter_input.chars().count() {
                                    app.cursor_pos += 1;
                                }
                            }
                            KeyCode::Home => app.cursor_pos = 0,
                            KeyCode::End => {
                                app.cursor_pos = app.filter_input.chars().count()
                            }
                            KeyCode::Backspace => {
                                input_backspace(
                                    &mut app.filter_input,
                                    &mut app.cursor_pos,
                                );
                            }
                            KeyCode::Delete => {
                                input_delete(&mut app.filter_input, &mut app.cursor_pos);
                            }
                            KeyCode::Char(c) => {
                                input_insert(
                                    &mut app.filter_input,
                                    &mut app.cursor_pos,
                                    c,
                                );
                            }
                            _ => {}
                        },
                    }
                }
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        app.open_link_at(mouse.column, mouse.row);
                    }
                    MouseEventKind::ScrollUp => match app.mode {
                        Mode::TicketDetail
                        | Mode::DetailAddingComment
                        | Mode::DetailEditingComment
                        | Mode::DetailConfirmDelete
                        | Mode::DetailTransition
                        | Mode::DetailConfirmTransition
                        | Mode::DetailEditingSummary => app.detail_scroll_up(),
                        Mode::Normal | Mode::Searching => app.move_up(),
                        _ => {}
                    },
                    MouseEventKind::ScrollDown => match app.mode {
                        Mode::TicketDetail
                        | Mode::DetailAddingComment
                        | Mode::DetailEditingComment
                        | Mode::DetailConfirmDelete
                        | Mode::DetailTransition
                        | Mode::DetailConfirmTransition
                        | Mode::DetailEditingSummary => app.detail_scroll_down(),
                        Mode::Normal | Mode::Searching => app.move_down(),
                        _ => {}
                    },
                    _ => {}
                },
                _ => {}
            }
        }
    }

    io::stdout().execute(DisableMouseCapture)?;
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;

    Ok(())
}
