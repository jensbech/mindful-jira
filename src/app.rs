use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use crate::config::{Config, StatusFilter};
use crate::jira::{self, IssueDetail, Transition};
use crate::notes;

#[derive(PartialEq)]
pub enum Mode {
    Normal,
    Searching,
    EditingNote,
    EditingLongNote,
    FilterEditor,
    FilterAdding,
    TicketDetail,
    ConfirmBrowser,
    DetailAddingComment,
    DetailEditingComment,
    DetailConfirmDelete,
    DetailTransition,
    DetailConfirmTransition,
}

#[derive(Clone)]
pub struct DisplayRow {
    pub issue: jira::JiraIssue,
    pub depth: u8,
    pub is_context_parent: bool,
}

pub struct App {
    pub rows: Vec<DisplayRow>,
    pub all_rows: Vec<DisplayRow>,
    pub selected: usize,
    pub mode: Mode,
    pub search_input: String,
    pub note_input: String,
    pub notes: HashMap<String, String>,
    pub long_notes: HashMap<String, String>,
    pub long_note_input: String,
    pub long_note_scroll: usize,
    pub highlighted_keys: std::collections::HashSet<String>,
    pub config: Config,
    pub status_msg: String,
    pub show_all_parents: bool,
    // Filter editor state
    pub filter_selected: usize,
    pub filter_input: String,
    // Ticket detail state
    pub detail: Option<IssueDetail>,
    pub detail_scroll: u16,
    pub detail_lines: Cell<usize>,
    // Comment interaction state
    pub detail_comment_selected: Option<usize>,
    pub comment_input: String,
    pub editing_comment_id: Option<String>,
    // Text input cursor
    pub cursor_pos: usize,
    // Link click tracking (set during rendering)
    pub detail_link_map: RefCell<Vec<Option<String>>>,
    pub detail_content_y: Cell<u16>,
    pub detail_content_height: Cell<u16>,
    // Comment line offsets (set during rendering, used for auto-scroll)
    pub detail_comment_offsets: RefCell<Vec<usize>>,
    // Transition picker state
    pub transitions: Vec<Transition>,
    pub transition_selected: usize,
    // Current user identity
    pub current_account_id: String,
    // Legend toggle
    pub show_legend: bool,
}

impl App {
    pub fn new(config: Config) -> Self {
        let notes = notes::load_notes();
        let long_notes = notes::load_long_notes();
        let highlighted_keys = notes::load_highlights();
        App {
            rows: Vec::new(),
            all_rows: Vec::new(),
            selected: 0,
            mode: Mode::Normal,
            search_input: String::new(),
            note_input: String::new(),
            notes,
            long_notes,
            long_note_input: String::new(),
            long_note_scroll: 0,
            highlighted_keys,
            config,
            status_msg: String::new(),
            show_all_parents: false,
            filter_selected: 0,
            filter_input: String::new(),
            detail: None,
            detail_scroll: 0,
            detail_lines: Cell::new(0),
            detail_comment_selected: None,
            comment_input: String::new(),
            editing_comment_id: None,
            cursor_pos: 0,
            detail_link_map: RefCell::new(Vec::new()),
            detail_content_y: Cell::new(0),
            detail_content_height: Cell::new(0),
            detail_comment_offsets: RefCell::new(Vec::new()),
            transitions: Vec::new(),
            transition_selected: 0,
            current_account_id: String::new(),
            show_legend: false,
        }
    }

    pub async fn init(&mut self) {
        match jira::fetch_current_account_id(&self.config).await {
            Ok(id) => self.current_account_id = id,
            Err(e) => self.status_msg = format!("Warning: {e}"),
        }
    }

    pub async fn refresh(&mut self) {
        self.status_msg = "Fetching issues...".to_string();
        match jira::fetch_issues(&self.config, self.show_all_parents).await {
            Ok(issues) => {
                self.all_rows = issues
                    .into_iter()
                    .map(|issue| {
                        let depth = if issue.is_subtask || issue.parent_key.is_some() {
                            1
                        } else {
                            0
                        };
                        let is_context_parent = issue.is_context_parent;
                        DisplayRow {
                            issue,
                            depth,
                            is_context_parent,
                        }
                    })
                    .collect();
                let count = self.all_rows.len();
                self.status_msg = format!("Loaded {count} issues");
                self.apply_search_filter();
            }
            Err(e) => {
                self.status_msg = format!("Error: {e}");
            }
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if !self.rows.is_empty() && self.selected < self.rows.len() - 1 {
            self.selected += 1;
        }
    }

    pub fn confirm_open_in_browser(&mut self) {
        if self.rows.get(self.selected).is_some() {
            self.mode = Mode::ConfirmBrowser;
        }
    }

    pub fn open_in_browser(&mut self) {
        if let Some(row) = self.rows.get(self.selected) {
            let url = format!(
                "{}/browse/{}",
                self.config.jira_url.trim_end_matches('/'),
                row.issue.key
            );
            let _ = open::that(&url);
        }
        self.mode = Mode::Normal;
    }

    pub fn cancel_browser(&mut self) {
        self.mode = Mode::Normal;
    }

    pub fn start_editing_status(&mut self) {
        if let Some(row) = self.rows.get(self.selected) {
            self.note_input = self
                .notes
                .get(&row.issue.key)
                .cloned()
                .unwrap_or_default();
            self.cursor_pos = self.note_input.chars().count();
            self.mode = Mode::EditingNote;
        }
    }

    pub fn save_status(&mut self) {
        if let Some(row) = self.rows.get(self.selected) {
            let key = row.issue.key.clone();
            if self.note_input.is_empty() {
                self.notes.remove(&key);
            } else {
                self.notes.insert(key, self.note_input.clone());
            }
            notes::save_notes(&self.notes);
        }
        self.note_input.clear();
        self.mode = Mode::Normal;
    }

    pub fn cancel_edit(&mut self) {
        self.note_input.clear();
        self.mode = Mode::Normal;
    }

    pub fn start_editing_long_note(&mut self) {
        if let Some(row) = self.rows.get(self.selected) {
            self.long_note_input = self
                .long_notes
                .get(&row.issue.key)
                .cloned()
                .unwrap_or_default();
            self.cursor_pos = self.long_note_input.len();
            self.long_note_scroll = 0;
            self.mode = Mode::EditingLongNote;
        }
    }

    pub fn save_long_note(&mut self) {
        if let Some(row) = self.rows.get(self.selected) {
            let key = row.issue.key.clone();
            if self.long_note_input.is_empty() {
                self.long_notes.remove(&key);
            } else {
                self.long_notes.insert(key, self.long_note_input.clone());
            }
            notes::save_long_notes(&self.long_notes);
        }
        self.long_note_input.clear();
        self.mode = Mode::Normal;
    }

    pub fn cancel_long_note(&mut self) {
        self.long_note_input.clear();
        self.mode = Mode::Normal;
    }

    pub fn toggle_highlight(&mut self) {
        if let Some(row) = self.rows.get(self.selected) {
            let key = row.issue.key.clone();
            if !self.highlighted_keys.remove(&key) {
                self.highlighted_keys.insert(key);
            }
            notes::save_highlights(&self.highlighted_keys);
        }
    }

    // --- Fuzzy search ---

    pub fn start_search(&mut self) {
        self.search_input.clear();
        self.mode = Mode::Searching;
    }

    pub fn apply_search_filter(&mut self) {
        if self.search_input.is_empty() {
            self.rows = self.all_rows.clone();
        } else {
            self.rows = self
                .all_rows
                .iter()
                .filter(|row| {
                    let haystack = format!("{} {}", row.issue.key, row.issue.summary);
                    fuzzy_match(&haystack, &self.search_input).is_some()
                })
                .cloned()
                .collect();
        }
        if self.rows.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.rows.len() {
            self.selected = self.rows.len() - 1;
        }
    }

    pub fn confirm_search(&mut self) {
        self.mode = Mode::Normal;
    }

    pub fn clear_search(&mut self) {
        self.search_input.clear();
        self.rows = self.all_rows.clone();
        if self.rows.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.rows.len() {
            self.selected = self.rows.len() - 1;
        }
        self.mode = Mode::Normal;
    }

    // --- Ticket detail ---

    pub async fn open_ticket_detail(&mut self) {
        let key = match self.rows.get(self.selected) {
            Some(row) => row.issue.key.clone(),
            None => return,
        };
        self.status_msg = format!("Loading {key}...");
        match jira::fetch_issue_detail(&self.config, &key).await {
            Ok(detail) => {
                self.detail = Some(detail);
                self.detail_scroll = 0;
                self.mode = Mode::TicketDetail;
                self.status_msg.clear();
            }
            Err(e) => {
                self.status_msg = format!("Error: {e}");
            }
        }
    }

    pub fn close_detail(&mut self) {
        self.detail = None;
        self.detail_scroll = 0;
        self.detail_comment_selected = None;
        self.mode = Mode::Normal;
    }

    pub fn detail_scroll_up(&mut self) {
        self.detail_scroll = self.detail_scroll.saturating_sub(1);
    }

    pub fn detail_scroll_down(&mut self) {
        let total = self.detail_lines.get();
        if (self.detail_scroll as usize) + 1 < total {
            self.detail_scroll += 1;
        }
    }

    pub fn detail_open_in_browser(&self) {
        if let Some(ref detail) = self.detail {
            let url = format!(
                "{}/browse/{}",
                self.config.jira_url.trim_end_matches('/'),
                detail.key
            );
            let _ = open::that(&url);
        }
    }

    // --- Comment navigation & CRUD ---

    pub fn detail_next_comment(&mut self) {
        let count = self
            .detail
            .as_ref()
            .map(|d| d.comments.len())
            .unwrap_or(0);
        if count == 0 {
            return;
        }
        self.detail_comment_selected = Some(match self.detail_comment_selected {
            None => 0,
            Some(i) => (i + 1).min(count - 1),
        });
        self.scroll_to_selected_comment();
    }

    pub fn detail_prev_comment(&mut self) {
        self.detail_comment_selected = match self.detail_comment_selected {
            None => None,
            Some(0) => None,
            Some(i) => Some(i - 1),
        };
        self.scroll_to_selected_comment();
    }

    fn scroll_to_selected_comment(&mut self) {
        let idx = match self.detail_comment_selected {
            Some(i) => i,
            None => return,
        };
        let offsets = self.detail_comment_offsets.borrow();
        let line = match offsets.get(idx) {
            Some(&l) => l,
            None => return,
        };
        let visible_h = self.detail_content_height.get() as usize;
        if visible_h == 0 {
            return;
        }
        let scroll = self.detail_scroll as usize;
        if line < scroll {
            self.detail_scroll = line as u16;
        } else if line >= scroll + visible_h {
            self.detail_scroll = line as u16;
        }
    }

    pub fn copy_ticket_to_clipboard(&mut self) {
        let detail = match &self.detail {
            Some(d) => d,
            None => return,
        };
        let mut text = String::new();
        text.push_str(&format!("{}\n{}\n\n", detail.key, detail.summary));
        text.push_str("Description:\n");
        text.push_str(&detail.description);
        text.push_str("\n\n");
        if !detail.comments.is_empty() {
            text.push_str(&format!("Comments ({}):\n", detail.comments.len()));
            for (i, comment) in detail.comments.iter().enumerate() {
                text.push_str(&format!(
                    "\n#{} {} ({})\n",
                    i + 1,
                    comment.author,
                    comment.created
                ));
                text.push_str(&comment.body);
                text.push('\n');
            }
        }
        match copy_to_clipboard(&text) {
            Ok(()) => self.status_msg = "Ticket copied to clipboard".to_string(),
            Err(e) => self.status_msg = format!("Copy failed: {e}"),
        }
    }

    pub fn start_adding_comment(&mut self) {
        self.comment_input.clear();
        self.cursor_pos = 0;
        self.mode = Mode::DetailAddingComment;
    }

    pub fn start_editing_comment(&mut self) {
        let idx = match self.detail_comment_selected {
            Some(i) => i,
            None => return,
        };
        let detail = match &self.detail {
            Some(d) => d,
            None => return,
        };
        let comment = match detail.comments.get(idx) {
            Some(c) => c,
            None => return,
        };
        if comment.author_account_id != self.current_account_id {
            self.status_msg = "Can only edit your own comments".to_string();
            return;
        }
        self.comment_input = comment.body.clone();
        self.cursor_pos = self.comment_input.chars().count();
        self.editing_comment_id = Some(comment.id.clone());
        self.mode = Mode::DetailEditingComment;
    }

    pub fn confirm_delete_comment(&mut self) {
        let idx = match self.detail_comment_selected {
            Some(i) => i,
            None => return,
        };
        if let Some(detail) = &self.detail {
            if let Some(comment) = detail.comments.get(idx) {
                if comment.author_account_id != self.current_account_id {
                    self.status_msg = "Can only delete your own comments".to_string();
                    return;
                }
            }
        }
        self.mode = Mode::DetailConfirmDelete;
    }

    pub fn cancel_comment_action(&mut self) {
        self.comment_input.clear();
        self.editing_comment_id = None;
        self.mode = Mode::TicketDetail;
    }

    pub async fn submit_comment(&mut self) {
        let text = self.comment_input.trim().to_string();
        if text.is_empty() {
            self.cancel_comment_action();
            return;
        }
        let key = match &self.detail {
            Some(d) => d.key.clone(),
            None => return,
        };
        self.status_msg = "Adding comment...".to_string();
        match jira::add_comment(&self.config, &key, &text).await {
            Ok(()) => {
                self.status_msg = "Comment added".to_string();
                self.comment_input.clear();
                self.mode = Mode::TicketDetail;
                self.refresh_detail(&key).await;
            }
            Err(e) => {
                self.status_msg = format!("Error: {e}");
                self.mode = Mode::TicketDetail;
            }
        }
    }

    pub async fn save_edited_comment(&mut self) {
        let text = self.comment_input.trim().to_string();
        if text.is_empty() {
            self.cancel_comment_action();
            return;
        }
        let key = match &self.detail {
            Some(d) => d.key.clone(),
            None => return,
        };
        let comment_id = match &self.editing_comment_id {
            Some(id) => id.clone(),
            None => return,
        };
        self.status_msg = "Updating comment...".to_string();
        match jira::update_comment(&self.config, &key, &comment_id, &text).await {
            Ok(()) => {
                self.status_msg = "Comment updated".to_string();
                self.comment_input.clear();
                self.editing_comment_id = None;
                self.mode = Mode::TicketDetail;
                self.refresh_detail(&key).await;
            }
            Err(e) => {
                self.status_msg = format!("Error: {e}");
                self.mode = Mode::TicketDetail;
            }
        }
    }

    pub async fn execute_delete_comment(&mut self) {
        let idx = match self.detail_comment_selected {
            Some(i) => i,
            None => return,
        };
        let detail = match &self.detail {
            Some(d) => d,
            None => return,
        };
        let comment = match detail.comments.get(idx) {
            Some(c) => c,
            None => return,
        };
        let key = detail.key.clone();
        let comment_id = comment.id.clone();
        self.status_msg = "Deleting comment...".to_string();
        match jira::delete_comment(&self.config, &key, &comment_id).await {
            Ok(()) => {
                self.status_msg = "Comment deleted".to_string();
                self.detail_comment_selected = None;
                self.mode = Mode::TicketDetail;
                self.refresh_detail(&key).await;
            }
            Err(e) => {
                self.status_msg = format!("Error: {e}");
                self.mode = Mode::TicketDetail;
            }
        }
    }

    async fn refresh_detail(&mut self, key: &str) {
        match jira::fetch_issue_detail(&self.config, key).await {
            Ok(detail) => {
                self.detail = Some(detail);
            }
            Err(e) => {
                self.status_msg = format!("Error refreshing: {e}");
            }
        }
    }

    // --- Transitions ---

    pub async fn open_transition_picker(&mut self) {
        let key = match &self.detail {
            Some(d) => d.key.clone(),
            None => return,
        };
        self.status_msg = "Loading transitions...".to_string();
        match jira::fetch_transitions(&self.config, &key).await {
            Ok(transitions) => {
                if transitions.is_empty() {
                    self.status_msg = "No transitions available".to_string();
                    return;
                }
                self.transitions = transitions;
                self.transition_selected = 0;
                self.mode = Mode::DetailTransition;
                self.status_msg.clear();
            }
            Err(e) => {
                self.status_msg = format!("Error: {e}");
            }
        }
    }

    pub fn transition_move_up(&mut self) {
        if self.transition_selected > 0 {
            self.transition_selected -= 1;
        }
    }

    pub fn transition_move_down(&mut self) {
        if !self.transitions.is_empty()
            && self.transition_selected < self.transitions.len() - 1
        {
            self.transition_selected += 1;
        }
    }

    pub fn cancel_transition(&mut self) {
        self.transitions.clear();
        self.mode = Mode::TicketDetail;
    }

    pub fn confirm_transition(&mut self) {
        if self.transitions.get(self.transition_selected).is_some() {
            self.mode = Mode::DetailConfirmTransition;
        }
    }

    pub fn cancel_confirm_transition(&mut self) {
        self.mode = Mode::DetailTransition;
    }

    pub async fn execute_transition(&mut self) {
        let transition = match self.transitions.get(self.transition_selected) {
            Some(t) => t,
            None => return,
        };
        let key = match &self.detail {
            Some(d) => d.key.clone(),
            None => return,
        };
        let name = transition.name.clone();
        let id = transition.id.clone();
        self.status_msg = format!("Transitioning to {name}...");
        match jira::do_transition(&self.config, &key, &id).await {
            Ok(()) => {
                self.transitions.clear();
                self.mode = Mode::TicketDetail;
                self.refresh().await;
                self.refresh_detail(&key).await;
                self.status_msg = format!("Transitioned to {name}");
            }
            Err(e) => {
                self.status_msg = format!("Error: {e}");
                self.transitions.clear();
                self.mode = Mode::TicketDetail;
            }
        }
    }

    // --- Filter editor ---

    pub fn open_filter_editor(&mut self) {
        self.filter_selected = 0;
        self.mode = Mode::FilterEditor;
    }

    pub fn filter_move_up(&mut self) {
        if self.filter_selected > 0 {
            self.filter_selected -= 1;
        }
    }

    pub fn filter_move_down(&mut self) {
        if !self.config.status_filters.is_empty()
            && self.filter_selected < self.config.status_filters.len() - 1
        {
            self.filter_selected += 1;
        }
    }

    pub fn toggle_filter(&mut self) {
        if let Some(f) = self.config.status_filters.get_mut(self.filter_selected) {
            f.excluded = !f.excluded;
        }
    }

    pub fn start_adding_filter(&mut self) {
        self.filter_input.clear();
        self.cursor_pos = 0;
        self.mode = Mode::FilterAdding;
    }

    pub fn confirm_add_filter(&mut self) {
        let name = self.filter_input.trim().to_string();
        if !name.is_empty() {
            self.config.status_filters.push(StatusFilter {
                name,
                excluded: true,
            });
            self.filter_selected = self.config.status_filters.len() - 1;
        }
        self.filter_input.clear();
        self.mode = Mode::FilterEditor;
    }

    pub fn cancel_add_filter(&mut self) {
        self.filter_input.clear();
        self.mode = Mode::FilterEditor;
    }

    pub fn delete_filter(&mut self) {
        if !self.config.status_filters.is_empty() {
            self.config.status_filters.remove(self.filter_selected);
            if self.filter_selected >= self.config.status_filters.len() {
                self.filter_selected = self.config.status_filters.len().saturating_sub(1);
            }
        }
    }

    pub fn close_filter_editor(&mut self) {
        self.config.save();
        self.mode = Mode::Normal;
    }

    pub async fn apply_filters_and_refresh(&mut self) {
        self.config.save();
        self.mode = Mode::Normal;
        self.refresh().await;
    }

    pub fn open_link_at(&self, _screen_x: u16, screen_y: u16) -> bool {
        let content_y = self.detail_content_y.get();
        let content_h = self.detail_content_height.get();
        if screen_y < content_y || screen_y >= content_y + content_h {
            return false;
        }
        let line_idx = (screen_y - content_y) as usize + self.detail_scroll as usize;
        let link_map = self.detail_link_map.borrow();
        if let Some(Some(url)) = link_map.get(line_idx) {
            let _ = open::that(url);
            return true;
        }
        false
    }

    pub async fn toggle_show_all_parents(&mut self) {
        self.show_all_parents = !self.show_all_parents;
        self.refresh().await;
    }
}

/// Case-insensitive subsequence fuzzy match. Returns matched char positions if all
/// needle chars are found in order within the haystack.
pub fn fuzzy_match(haystack: &str, needle: &str) -> Option<Vec<usize>> {
    let haystack_lower: Vec<char> = haystack.chars().flat_map(|c| c.to_lowercase()).collect();
    let needle_lower: Vec<char> = needle.chars().flat_map(|c| c.to_lowercase()).collect();

    let mut positions = Vec::with_capacity(needle_lower.len());
    let mut hay_idx = 0;
    for nc in &needle_lower {
        let mut found = false;
        while hay_idx < haystack_lower.len() {
            if haystack_lower[hay_idx] == *nc {
                positions.push(hay_idx);
                hay_idx += 1;
                found = true;
                break;
            }
            hay_idx += 1;
        }
        if !found {
            return None;
        }
    }
    Some(positions)
}

fn copy_to_clipboard(text: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut cmd = if cfg!(target_os = "macos") {
        Command::new("pbcopy")
    } else {
        let mut c = Command::new("xclip");
        c.arg("-selection").arg("clipboard");
        c
    };

    let mut child = cmd
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("{e}"))?;
    }

    child.wait().map_err(|e| format!("{e}"))?;
    Ok(())
}
