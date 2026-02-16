use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::Instant;

use crate::config::{Config, StatusFilter};
use crate::jira::{self, IssueDetail, JiraUser, MentionInsert, Transition};
use crate::notes;

pub struct MentionState {
    pub trigger_pos: usize,
    pub query: String,
    pub candidates: Vec<JiraUser>,
    pub selected: usize,
}

#[derive(Clone, Debug)]
pub struct ResolvedMention {
    pub start_pos: usize,
    pub len: usize,
    pub account_id: String,
    pub display_name: String,
}

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
    pub muted_keys: std::collections::HashSet<String>,
    pub config: Config,
    pub status_msg: String,
    pub status_set_at: Instant,
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
    // Mention state
    pub mention: Option<MentionState>,
    pub resolved_mentions: Vec<ResolvedMention>,
    // Dummy mode (no Jira connection)
    pub dummy: bool,
}

impl App {
    pub fn new(config: Config) -> Self {
        let notes = notes::load_notes();
        let long_notes = notes::load_long_notes();
        let highlighted_keys = notes::load_highlights();
        let muted_keys = notes::load_muted();
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
            muted_keys,
            config,
            status_msg: String::new(),
            status_set_at: Instant::now(),
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
            mention: None,
            resolved_mentions: Vec::new(),
            dummy: false,
        }
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_msg = msg.into();
        self.status_set_at = Instant::now();
    }

    pub async fn init(&mut self) {
        if self.dummy {
            self.current_account_id = "dummy-user-001".to_string();
            return;
        }
        match jira::fetch_current_account_id(&self.config).await {
            Ok(id) => self.current_account_id = id,
            Err(e) => self.set_status(format!("Warning: {e}")),
        }
    }

    pub async fn refresh(&mut self) {
        if self.dummy {
            self.all_rows = dummy_issues();
            let count = self.all_rows.len();
            self.set_status(format!("Loaded {count} issues"));
            self.apply_search_filter();
            return;
        }
        self.set_status("Fetching issues...");
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
                self.set_status(format!("Loaded {count} issues"));
                self.apply_search_filter();
            }
            Err(e) => {
                self.set_status(format!("Error: {e}"));
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

    pub fn copy_key_to_clipboard(&mut self) {
        if let Some(row) = self.rows.get(self.selected) {
            match copy_to_clipboard(&row.issue.key) {
                Ok(()) => self.set_status(format!("Copied {}", row.issue.key)),
                Err(e) => self.set_status(format!("Copy failed: {e}")),
            }
        }
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

    pub fn toggle_mute(&mut self) {
        if let Some(row) = self.rows.get(self.selected) {
            let key = row.issue.key.clone();
            if !self.muted_keys.remove(&key) {
                self.muted_keys.insert(key);
            }
            notes::save_muted(&self.muted_keys);
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
        if self.dummy {
            self.detail = Some(dummy_detail(&key));
            self.detail_scroll = 0;
            self.mode = Mode::TicketDetail;
            self.status_msg.clear();
            return;
        }
        self.set_status(format!("Loading {key}..."));
        match jira::fetch_issue_detail(&self.config, &key).await {
            Ok(detail) => {
                self.detail = Some(detail);
                self.detail_scroll = 0;
                self.mode = Mode::TicketDetail;
                self.status_msg.clear();
            }
            Err(e) => {
                self.set_status(format!("Error: {e}"));
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
            Ok(()) => self.set_status("Ticket copied to clipboard"),
            Err(e) => self.set_status(format!("Copy failed: {e}")),
        }
    }

    pub fn start_adding_comment(&mut self) {
        self.comment_input.clear();
        self.cursor_pos = 0;
        self.mention = None;
        self.resolved_mentions.clear();
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
            self.set_status("Can only edit your own comments");
            return;
        }
        self.comment_input = comment.body.clone();
        self.cursor_pos = self.comment_input.chars().count();
        self.editing_comment_id = Some(comment.id.clone());
        self.mention = None;
        self.resolved_mentions.clear();
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
                    self.set_status("Can only delete your own comments");
                    return;
                }
            }
        }
        self.mode = Mode::DetailConfirmDelete;
    }

    pub fn cancel_comment_action(&mut self) {
        self.comment_input.clear();
        self.editing_comment_id = None;
        self.mention = None;
        self.resolved_mentions.clear();
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
        if self.dummy {
            // Add comment locally to the detail
            if let Some(ref mut detail) = self.detail {
                detail.comments.insert(
                    0,
                    jira::Comment {
                        id: format!("dummy-{}", detail.comments.len() + 1),
                        author: "You (Demo User)".into(),
                        author_account_id: "dummy-user-001".into(),
                        created: "2026-02-16".into(),
                        body: text,
                    },
                );
            }
            self.set_status("Comment added (dummy)");
            self.comment_input.clear();
            self.mention = None;
            self.resolved_mentions.clear();
            self.mode = Mode::TicketDetail;
            return;
        }
        let mentions = self.build_mention_inserts();
        self.set_status("Adding comment...");
        match jira::add_comment(&self.config, &key, &text, &mentions).await {
            Ok(()) => {
                self.set_status("Comment added");
                self.comment_input.clear();
                self.mention = None;
                self.resolved_mentions.clear();
                self.mode = Mode::TicketDetail;
                self.refresh_detail(&key).await;
            }
            Err(e) => {
                self.set_status(format!("Error: {e}"));
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
        if self.dummy {
            if let (Some(ref mut detail), Some(ref cid)) = (&mut self.detail, &self.editing_comment_id) {
                if let Some(c) = detail.comments.iter_mut().find(|c| c.id == *cid) {
                    c.body = text;
                }
            }
            self.set_status("Comment updated (dummy)");
            self.comment_input.clear();
            self.editing_comment_id = None;
            self.mention = None;
            self.resolved_mentions.clear();
            self.mode = Mode::TicketDetail;
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
        let mentions = self.build_mention_inserts();
        self.set_status("Updating comment...");
        match jira::update_comment(&self.config, &key, &comment_id, &text, &mentions).await {
            Ok(()) => {
                self.set_status("Comment updated");
                self.comment_input.clear();
                self.editing_comment_id = None;
                self.mention = None;
                self.resolved_mentions.clear();
                self.mode = Mode::TicketDetail;
                self.refresh_detail(&key).await;
            }
            Err(e) => {
                self.set_status(format!("Error: {e}"));
                self.mode = Mode::TicketDetail;
            }
        }
    }

    pub async fn execute_delete_comment(&mut self) {
        let idx = match self.detail_comment_selected {
            Some(i) => i,
            None => return,
        };
        if self.dummy {
            if let Some(ref mut detail) = self.detail {
                if idx < detail.comments.len() {
                    detail.comments.remove(idx);
                }
            }
            self.set_status("Comment deleted (dummy)");
            self.detail_comment_selected = None;
            self.mode = Mode::TicketDetail;
            return;
        }
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
        self.set_status("Deleting comment...");
        match jira::delete_comment(&self.config, &key, &comment_id).await {
            Ok(()) => {
                self.set_status("Comment deleted");
                self.detail_comment_selected = None;
                self.mode = Mode::TicketDetail;
                self.refresh_detail(&key).await;
            }
            Err(e) => {
                self.set_status(format!("Error: {e}"));
                self.mode = Mode::TicketDetail;
            }
        }
    }

    async fn refresh_detail(&mut self, key: &str) {
        if self.dummy {
            return; // detail already updated in-place
        }
        match jira::fetch_issue_detail(&self.config, key).await {
            Ok(detail) => {
                self.detail = Some(detail);
            }
            Err(e) => {
                self.set_status(format!("Error refreshing: {e}"));
            }
        }
    }

    // --- Mention methods ---

    pub fn activate_mention(&mut self) {
        self.mention = Some(MentionState {
            trigger_pos: self.cursor_pos,
            query: String::new(),
            candidates: Vec::new(),
            selected: 0,
        });
    }

    pub fn update_mention_query(&mut self) {
        if let Some(ref mut mention) = self.mention {
            let chars: Vec<char> = self.comment_input.chars().collect();
            // The trigger_pos points to the position right after the '@' char
            // '@' is at trigger_pos - 1, query starts at trigger_pos
            if mention.trigger_pos <= chars.len() {
                let query: String = chars[mention.trigger_pos..self.cursor_pos].iter().collect();
                mention.query = query;
            }
        }
    }

    pub fn mention_move_up(&mut self) {
        if let Some(ref mut mention) = self.mention {
            if mention.selected > 0 {
                mention.selected -= 1;
            }
        }
    }

    pub fn mention_move_down(&mut self) {
        if let Some(ref mut mention) = self.mention {
            if !mention.candidates.is_empty()
                && mention.selected < mention.candidates.len() - 1
            {
                mention.selected += 1;
            }
        }
    }

    pub fn select_mention(&mut self) {
        let (trigger_pos, account_id, display_name) = match &self.mention {
            Some(mention) => {
                let candidate = match mention.candidates.get(mention.selected) {
                    Some(c) => c,
                    None => return,
                };
                (
                    mention.trigger_pos,
                    candidate.account_id.clone(),
                    candidate.display_name.clone(),
                )
            }
            None => return,
        };

        // The '@' is at trigger_pos - 1, query runs from trigger_pos to cursor_pos
        let at_pos = trigger_pos - 1;
        let replace_text = format!("@{} ", display_name);
        let replace_char_len = replace_text.chars().count();

        // Remove from '@' position to current cursor position
        let chars: Vec<char> = self.comment_input.chars().collect();
        let mut new_chars: Vec<char> = Vec::new();
        new_chars.extend_from_slice(&chars[..at_pos]);
        new_chars.extend(replace_text.chars());
        new_chars.extend_from_slice(&chars[self.cursor_pos..]);
        self.comment_input = new_chars.iter().collect();

        let old_cursor = self.cursor_pos;
        self.cursor_pos = at_pos + replace_char_len;

        // Record the resolved mention (the @DisplayName part, excluding trailing space)
        let mention_text_len = replace_char_len - 1; // exclude trailing space
        self.resolved_mentions.push(ResolvedMention {
            start_pos: at_pos,
            len: mention_text_len,
            account_id,
            display_name,
        });

        // Adjust positions of existing resolved mentions that come after the edit
        let chars_removed = old_cursor - at_pos;
        let chars_added = replace_char_len;
        let shift = chars_added as isize - chars_removed as isize;
        if shift != 0 {
            for rm in &mut self.resolved_mentions {
                if rm.start_pos > at_pos
                    && rm.start_pos != at_pos // skip the one we just added
                {
                    rm.start_pos = (rm.start_pos as isize + shift) as usize;
                }
            }
        }

        self.mention = None;
    }

    pub fn cancel_mention(&mut self) {
        self.mention = None;
    }

    pub async fn fetch_mention_candidates(&mut self) {
        let query = match &self.mention {
            Some(m) => m.query.clone(),
            None => return,
        };
        if query.is_empty() {
            if let Some(ref mut mention) = self.mention {
                mention.candidates.clear();
                mention.selected = 0;
            }
            return;
        }
        if self.dummy {
            let all_users = vec![
                JiraUser { account_id: "u-001".into(), display_name: "Jane Smith".into() },
                JiraUser { account_id: "u-002".into(), display_name: "John Doe".into() },
                JiraUser { account_id: "u-003".into(), display_name: "Alice Chen".into() },
                JiraUser { account_id: "u-004".into(), display_name: "Bob Martinez".into() },
                JiraUser { account_id: "u-005".into(), display_name: "Sarah Johnson".into() },
                JiraUser { account_id: "u-006".into(), display_name: "James Wilson".into() },
                JiraUser { account_id: "u-007".into(), display_name: "Emily Davis".into() },
                JiraUser { account_id: "u-008".into(), display_name: "Michael Brown".into() },
            ];
            let q = query.to_lowercase();
            let filtered: Vec<JiraUser> = all_users
                .into_iter()
                .filter(|u| u.display_name.to_lowercase().contains(&q))
                .collect();
            if let Some(ref mut mention) = self.mention {
                mention.candidates = filtered;
                mention.selected = 0;
            }
            return;
        }
        match jira::search_users(&self.config, &query).await {
            Ok(users) => {
                if let Some(ref mut mention) = self.mention {
                    mention.candidates = users;
                    mention.selected = 0;
                }
            }
            Err(_) => {
                // Silently ignore search errors
            }
        }
    }

    pub fn invalidate_overlapping_mentions(&mut self) {
        let chars: Vec<char> = self.comment_input.chars().collect();
        self.resolved_mentions.retain(|rm| {
            let end = rm.start_pos + rm.len;
            if end > chars.len() {
                return false;
            }
            // Check that the text at the recorded position still matches @DisplayName
            let expected = format!("@{}", rm.display_name);
            let actual: String = chars[rm.start_pos..end].iter().collect();
            actual == expected
        });
    }

    fn build_mention_inserts(&self) -> Vec<MentionInsert> {
        self.resolved_mentions
            .iter()
            .map(|rm| MentionInsert {
                start: rm.start_pos,
                len: rm.len,
                account_id: rm.account_id.clone(),
                display_name: rm.display_name.clone(),
            })
            .collect()
    }

    // --- Transitions ---

    pub async fn open_transition_picker(&mut self) {
        if self.dummy {
            let current = self.detail.as_ref().map(|d| d.status.as_str()).unwrap_or("");
            self.transitions = vec![
                Transition { id: "t1".into(), name: "Start Progress".into(), to_status: "In Progress".into() },
                Transition { id: "t2".into(), name: "Move to Review".into(), to_status: "In Review".into() },
                Transition { id: "t3".into(), name: "Done".into(), to_status: "Done".into() },
                Transition { id: "t4".into(), name: "Reopen".into(), to_status: current.to_string() },
            ];
            self.transition_selected = 0;
            self.mode = Mode::DetailTransition;
            return;
        }
        let key = match &self.detail {
            Some(d) => d.key.clone(),
            None => return,
        };
        self.set_status("Loading transitions...");
        match jira::fetch_transitions(&self.config, &key).await {
            Ok(transitions) => {
                if transitions.is_empty() {
                    self.set_status("No transitions available");
                    return;
                }
                self.transitions = transitions;
                self.transition_selected = 0;
                self.mode = Mode::DetailTransition;
                self.status_msg.clear();
            }
            Err(e) => {
                self.set_status(format!("Error: {e}"));
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
        let name = transition.name.clone();
        let to_status = transition.to_status.clone();
        if self.dummy {
            if let Some(ref mut detail) = self.detail {
                detail.status = to_status;
            }
            self.transitions.clear();
            self.mode = Mode::TicketDetail;
            self.set_status(format!("Transitioned to {name} (dummy)"));
            return;
        }
        let key = match &self.detail {
            Some(d) => d.key.clone(),
            None => return,
        };
        let id = transition.id.clone();
        self.set_status(format!("Transitioning to {name}..."));
        match jira::do_transition(&self.config, &key, &id).await {
            Ok(()) => {
                self.transitions.clear();
                self.mode = Mode::TicketDetail;
                self.refresh().await;
                self.refresh_detail(&key).await;
                self.set_status(format!("Transitioned to {name}"));
            }
            Err(e) => {
                self.set_status(format!("Error: {e}"));
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
        if !self.dummy {
            self.config.save();
        }
        self.mode = Mode::Normal;
    }

    pub async fn apply_filters_and_refresh(&mut self) {
        if !self.dummy {
            self.config.save();
        }
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

// --- Dummy data generators ---

fn dummy_issues() -> Vec<DisplayRow> {
    let data: Vec<(
        &str,  // key
        &str,  // summary
        &str,  // issue_type
        &str,  // priority
        &str,  // status
        &str,  // resolution
        &str,  // reporter
        &str,  // assignee
        &str,  // created
        Option<&str>, // parent_key
    )> = vec![
        ("PROJ-101", "User authentication flow redesign", "Epic", "High", "In Progress", "Unresolved", "Sarah Johnson", "You", "2026-01-15", None),
        ("PROJ-102", "Implement OAuth2 login with Google", "Story", "High", "In Progress", "Unresolved", "Sarah Johnson", "You", "2026-01-20", Some("PROJ-101")),
        ("PROJ-103", "Add refresh token rotation", "Sub-task", "Major", "To Do", "Unresolved", "Jane Smith", "You", "2026-01-22", Some("PROJ-101")),
        ("PROJ-110", "Dashboard performance optimization", "Story", "Critical", "In Progress", "Unresolved", "Alice Chen", "You", "2026-01-10", None),
        ("PROJ-111", "Fix N+1 query in project list endpoint", "Bug", "Blocker", "In Review", "Unresolved", "Bob Martinez", "You", "2026-02-01", Some("PROJ-110")),
        ("PROJ-120", "Notification system overhaul", "Epic", "Medium", "To Do", "Unresolved", "James Wilson", "You", "2026-02-05", None),
        ("PROJ-121", "Email digest preferences UI", "Task", "Medium", "Selected for Development", "Unresolved", "Emily Davis", "You", "2026-02-06", Some("PROJ-120")),
        ("PROJ-122", "Slack integration webhook handler", "Task", "Medium", "To Do", "Unresolved", "Michael Brown", "You", "2026-02-07", Some("PROJ-120")),
        ("PROJ-130", "Search results showing stale data after index rebuild", "Bug", "High", "In Progress", "Unresolved", "John Doe", "You", "2026-02-10", None),
        ("PROJ-140", "Add dark mode support to settings page", "Improvement", "Low", "Backlog", "Unresolved", "Alice Chen", "You", "2026-02-12", None),
        ("PROJ-150", "Evaluate migration to PostgreSQL 17", "Spike", "Minor", "To Do", "Unresolved", "Bob Martinez", "You", "2026-02-14", None),
        ("PROJ-160", "Bulk export feature for admin dashboard", "New Feature", "Normal", "In Progress", "Unresolved", "Sarah Johnson", "You", "2026-01-28", None),
        ("PROJ-161", "CSV export handler", "Sub-task", "Normal", "Done", "Fixed", "Jane Smith", "You", "2026-01-30", Some("PROJ-160")),
        ("PROJ-162", "PDF export with charts", "Sub-task", "Normal", "In Progress", "Unresolved", "Jane Smith", "You", "2026-02-02", Some("PROJ-160")),
    ];

    data.into_iter()
        .map(|(key, summary, issue_type, priority, status, resolution, reporter, assignee, created, parent_key)| {
            let is_subtask = parent_key.is_some();
            DisplayRow {
                issue: jira::JiraIssue {
                    key: key.to_string(),
                    summary: summary.to_string(),
                    assignee: assignee.to_string(),
                    reporter: reporter.to_string(),
                    priority: priority.to_string(),
                    status: status.to_string(),
                    resolution: resolution.to_string(),
                    created: created.to_string(),
                    issue_type: issue_type.to_string(),
                    parent_key: parent_key.map(|s| s.to_string()),
                    is_subtask,
                    is_context_parent: false,
                },
                depth: if is_subtask { 1 } else { 0 },
                is_context_parent: false,
            }
        })
        .collect()
}

fn dummy_detail(key: &str) -> IssueDetail {
    // Find matching issue info for the summary
    let (summary, issue_type, status) = match key {
        "PROJ-101" => ("User authentication flow redesign", "Epic", "In Progress"),
        "PROJ-102" => ("Implement OAuth2 login with Google", "Story", "In Progress"),
        "PROJ-103" => ("Add refresh token rotation", "Sub-task", "To Do"),
        "PROJ-110" => ("Dashboard performance optimization", "Story", "In Progress"),
        "PROJ-111" => ("Fix N+1 query in project list endpoint", "Bug", "In Review"),
        "PROJ-120" => ("Notification system overhaul", "Epic", "To Do"),
        "PROJ-121" => ("Email digest preferences UI", "Task", "Selected for Development"),
        "PROJ-122" => ("Slack integration webhook handler", "Task", "To Do"),
        "PROJ-130" => ("Search results showing stale data after index rebuild", "Bug", "In Progress"),
        "PROJ-140" => ("Add dark mode support to settings page", "Improvement", "Backlog"),
        "PROJ-150" => ("Evaluate migration to PostgreSQL 17", "Spike", "To Do"),
        "PROJ-160" => ("Bulk export feature for admin dashboard", "New Feature", "In Progress"),
        "PROJ-161" => ("CSV export handler", "Sub-task", "Done"),
        "PROJ-162" => ("PDF export with charts", "Sub-task", "In Progress"),
        _ => ("Unknown issue", "Task", "To Do"),
    };

    let description = match key {
        "PROJ-110" => "## Overview\n\nThe main dashboard is taking **3-4 seconds** to load for users with more than 50 projects. We need to optimize the backend queries and add caching.\n\n## Acceptance Criteria\n\n  1. Dashboard loads in under 500ms for 100+ projects\n  2. Add Redis caching layer for project metadata\n  3. No visible change to the user experience\n\n## Technical Notes\n\nThe main bottleneck is in the `ProjectService.listWithStats()` method which fires N+1 queries for each project's issue count. Consider using a single aggregation query.\n\nhttps://wiki.internal.example.com/perf-guidelines".to_string(),
        "PROJ-130" => "## Bug Report\n\nAfter the nightly Elasticsearch index rebuild, search results are showing stale data for approximately **10 minutes**.\n\n## Steps to Reproduce\n\n  1. Trigger a full index rebuild via admin panel\n  2. Search for a recently updated ticket\n  3. Observe that old field values are shown\n\n## Expected Behavior\n\nSearch results should reflect the latest data immediately after rebuild completes.\n\n## Environment\n\n  - Elasticsearch 8.12\n  - Java 21\n  - `search-service` v2.4.1".to_string(),
        _ => format!("## Description\n\nThis ticket covers the implementation of: **{summary}**.\n\nPlease see the linked design doc for full requirements and mockups.\n\n## Acceptance Criteria\n\n  1. Feature works as described in the design doc\n  2. Unit tests cover all edge cases\n  3. Documentation updated\n\n## Notes\n\nReach out to @Alice Chen or @Bob Martinez if you have questions about the architecture."),
    };

    let comments = vec![
        jira::Comment {
            id: "c-001".to_string(),
            author: "Alice Chen".to_string(),
            author_account_id: "u-003".to_string(),
            created: "2026-02-14".to_string(),
            body: "I've started looking into this. The main blocker is the dependency on the auth service refactor. @Jane Smith can you confirm the timeline for that?".to_string(),
        },
        jira::Comment {
            id: "c-002".to_string(),
            author: "Jane Smith".to_string(),
            author_account_id: "u-001".to_string(),
            created: "2026-02-13".to_string(),
            body: "Auth service refactor should be done by end of this sprint. I'll ping you when the PR is merged.".to_string(),
        },
        jira::Comment {
            id: "c-003".to_string(),
            author: "You (Demo User)".to_string(),
            author_account_id: "dummy-user-001".to_string(),
            created: "2026-02-12".to_string(),
            body: "Sounds good, I'll start on the frontend pieces in the meantime. Let me know if you need help with the auth service PR.".to_string(),
        },
    ];

    IssueDetail {
        key: key.to_string(),
        issue_type: issue_type.to_string(),
        status: status.to_string(),
        summary: summary.to_string(),
        description,
        comments,
    }
}
