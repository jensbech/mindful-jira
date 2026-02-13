mod app;
mod config;
mod jira;
mod notes;
mod ui;

use std::io;
use std::io::Write;
use std::time::Duration;

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
    MouseEventKind,
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

    let status_filters = existing
        .map(|c| c.status_filters)
        .unwrap_or_else(config::default_status_filters);

    let config = Config {
        jira_url,
        email,
        api_token,
        status_filters,
    };
    config.save();

    println!("\nConfig saved to {}", config::config_dir().join("config.json").display());
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(|s| s.as_str()) == Some("setup") {
        run_setup();
        return Ok(());
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

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    match app.mode {
                        Mode::Normal => match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => break,
                            KeyCode::Up | KeyCode::Char('k') => app.move_up(),
                            KeyCode::Down | KeyCode::Char('j') => app.move_down(),
                            KeyCode::Enter => app.open_ticket_detail().await,
                            KeyCode::Char('w') => app.confirm_open_in_browser(),
                            KeyCode::Char('n') => app.start_editing_note(),
                            KeyCode::Char('h') => app.toggle_highlight(),
                            KeyCode::Char('f') => app.open_filter_editor(),
                            KeyCode::Char('p') => app.toggle_show_all_parents().await,
                            KeyCode::Char('r') => app.refresh().await,
                            KeyCode::Char('?') => app.show_legend = !app.show_legend,
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
                            KeyCode::Char('c') => app.start_adding_comment(),
                            KeyCode::Char('e') => app.start_editing_comment(),
                            KeyCode::Char('x') => app.confirm_delete_comment(),
                            KeyCode::Char('t') => app.open_transition_picker().await,
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
                                }
                                KeyCode::Delete => {
                                    input_delete(
                                        &mut app.comment_input,
                                        &mut app.cursor_pos,
                                    );
                                }
                                KeyCode::Char(c) => {
                                    input_insert(
                                        &mut app.comment_input,
                                        &mut app.cursor_pos,
                                        c,
                                    );
                                }
                                _ => {}
                            }
                        }
                        Mode::DetailConfirmDelete => match key.code {
                            KeyCode::Char('y') => app.execute_delete_comment().await,
                            KeyCode::Char('n') | KeyCode::Esc => app.cancel_comment_action(),
                            _ => {}
                        },
                        Mode::EditingNote => match key.code {
                            KeyCode::Enter => app.save_note(),
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
                        | Mode::DetailConfirmTransition => app.detail_scroll_up(),
                        Mode::Normal => app.move_up(),
                        _ => {}
                    },
                    MouseEventKind::ScrollDown => match app.mode {
                        Mode::TicketDetail
                        | Mode::DetailAddingComment
                        | Mode::DetailEditingComment
                        | Mode::DetailConfirmDelete
                        | Mode::DetailTransition
                        | Mode::DetailConfirmTransition => app.detail_scroll_down(),
                        Mode::Normal => app.move_down(),
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
