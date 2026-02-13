mod app;
mod config;
mod jira;
mod notes;
mod ui;

use std::io;
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
                            KeyCode::Enter => app.open_in_browser(),
                            KeyCode::Char('d') => app.open_ticket_detail().await,
                            KeyCode::Char('n') => app.start_editing_note(),
                            KeyCode::Char('f') => app.open_filter_editor(),
                            KeyCode::Char('p') => app.toggle_show_all_parents().await,
                            KeyCode::Char('r') => app.refresh().await,
                            KeyCode::Char('?') => app.show_legend = !app.show_legend,
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
                            KeyCode::Char('?') => app.show_legend = !app.show_legend,
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
                        | Mode::DetailConfirmDelete => app.detail_scroll_up(),
                        Mode::Normal => app.move_up(),
                        _ => {}
                    },
                    MouseEventKind::ScrollDown => match app.mode {
                        Mode::TicketDetail
                        | Mode::DetailAddingComment
                        | Mode::DetailEditingComment
                        | Mode::DetailConfirmDelete => app.detail_scroll_down(),
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
