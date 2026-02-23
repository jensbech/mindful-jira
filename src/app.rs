use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::Instant;

use ratatui::text::Line;

use crate::config::{Config, StatusFilter};
use crate::github::GithubPR;
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

#[derive(PartialEq, Clone, Copy)]
pub enum HighlightColor {
    Orange,
    Green,
}

impl HighlightColor {
    pub fn as_str(&self) -> &'static str {
        match self {
            HighlightColor::Orange => "orange",
            HighlightColor::Green => "green",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "orange" => Some(HighlightColor::Orange),
            "green" => Some(HighlightColor::Green),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            HighlightColor::Orange => "Doing now",
            HighlightColor::Green => "Ready for review",
        }
    }
}

pub const HIGHLIGHT_OPTIONS: [HighlightColor; 2] = [
    HighlightColor::Orange,
    HighlightColor::Green,
];

#[derive(PartialEq, Clone, Copy)]
pub enum SortCriteria {
    Default,
    Board,
    Priority,
    Muted,
    Highlight,
}

impl SortCriteria {
    pub const ALL: [SortCriteria; 5] = [
        SortCriteria::Default,
        SortCriteria::Board,
        SortCriteria::Priority,
        SortCriteria::Muted,
        SortCriteria::Highlight,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            SortCriteria::Default => "Default (Jira order)",
            SortCriteria::Board => "Board (project prefix)",
            SortCriteria::Priority => "Priority",
            SortCriteria::Muted => "Muted (sink to bottom)",
            SortCriteria::Highlight => "Highlight (float to top)",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SortCriteria::Default => "default",
            SortCriteria::Board => "board",
            SortCriteria::Priority => "priority",
            SortCriteria::Muted => "muted",
            SortCriteria::Highlight => "highlight",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "board" => SortCriteria::Board,
            "priority" => SortCriteria::Priority,
            "muted" => SortCriteria::Muted,
            "highlight" => SortCriteria::Highlight,
            _ => SortCriteria::Default,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Column {
    Assignee,
    Reporter,
    Priority,
    Status,
    Resolution,
    Created,
}

impl Column {
    pub const ALL: [Column; 6] = [
        Column::Assignee,
        Column::Reporter,
        Column::Priority,
        Column::Status,
        Column::Resolution,
        Column::Created,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Column::Assignee => "Assignee",
            Column::Reporter => "Reporter",
            Column::Priority => "Priority",
            Column::Status => "Status",
            Column::Resolution => "Resolution",
            Column::Created => "Created",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Column::Assignee => "assignee",
            Column::Reporter => "reporter",
            Column::Priority => "priority",
            Column::Status => "status",
            Column::Resolution => "resolution",
            Column::Created => "created",
        }
    }
}

fn priority_rank(p: &str) -> u8 {
    match p {
        "Blocker" => 6,
        "Critical" | "Highest" => 5,
        "Major" | "High" => 4,
        "Normal" | "Medium" => 3,
        "Minor" | "Low" => 2,
        "Trivial" | "Lowest" => 1,
        _ => 0,
    }
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
    DetailEditingSummary,
    DetailPRList,
    HighlightPicker,
    SortPicker,
    ColumnPicker,
    ConfirmQuit,
}

#[derive(Clone)]
pub struct DisplayRow {
    pub issue: jira::JiraIssue,
    pub depth: u8,
    pub is_context_parent: bool,
    pub original_index: usize,
}

pub struct DetailRenderCache {
    pub version: u64,
    pub selected_comment: Option<usize>,
    pub render_width: u16,
    pub lines: Vec<Line<'static>>,
    pub link_map: Vec<Option<String>>,
    pub comment_offsets: Vec<usize>,
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
    pub highlighted_keys: HashMap<String, String>,
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
    // Detail render cache (avoids rebuilding markdown on every frame)
    pub detail_content_version: Cell<u64>,
    pub detail_render_cache: RefCell<Option<DetailRenderCache>>,
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
    pub last_mention_query: String,
    // Highlight picker state
    pub highlight_selected: usize,
    // Summary editing
    pub summary_input: String,
    // Detail-modal status (visible inside the modal)
    pub detail_status_msg: String,
    pub detail_status_set_at: Instant,
    // Sort picker state
    pub sort_selected: usize,
    pub sort_criteria: SortCriteria,
    // Column picker state
    pub column_picker_selected: usize,
    // PR list state
    pub pr_list: Vec<GithubPR>,
    pub pr_list_selected: usize,
}

impl App {
    pub fn new(config: Config) -> Self {
        let notes = notes::load_notes();
        let long_notes = notes::load_long_notes();
        let highlighted_keys = notes::load_highlights();
        let muted_keys = notes::load_muted();
        let sort_criteria = config
            .sort_order
            .as_deref()
            .map(SortCriteria::from_str)
            .unwrap_or(SortCriteria::Default);
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
            detail_content_version: Cell::new(0),
            detail_render_cache: RefCell::new(None),
            transitions: Vec::new(),
            transition_selected: 0,
            current_account_id: String::new(),
            show_legend: false,
            mention: None,
            resolved_mentions: Vec::new(),
            last_mention_query: String::new(),
            highlight_selected: 0,
            summary_input: String::new(),
            detail_status_msg: String::new(),
            detail_status_set_at: Instant::now(),
            sort_selected: 0,
            sort_criteria,
            column_picker_selected: 0,
            pr_list: Vec::new(),
            pr_list_selected: 0,
        }
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_msg = msg.into();
        self.status_set_at = Instant::now();
    }

    pub fn set_detail_status(&mut self, msg: impl Into<String>) {
        self.detail_status_msg = msg.into();
        self.detail_status_set_at = Instant::now();
    }

    pub async fn init(&mut self) {
        match jira::fetch_current_account_id(&self.config).await {
            Ok(id) => self.current_account_id = id,
            Err(e) => self.set_status(format!("Warning: {e}")),
        }
    }

    pub async fn refresh(&mut self) {
        self.set_status("Fetching issues...");
        match jira::fetch_issues(&self.config, self.show_all_parents).await {
            Ok(issues) => {
                self.all_rows = issues
                    .into_iter()
                    .enumerate()
                    .map(|(i, issue)| {
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
                            original_index: i,
                        }
                    })
                    .collect();
                let count = self.all_rows.len();
                self.set_status(format!("Loaded {count} issues"));
                self.sort_rows();
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

    pub fn confirm_quit(&mut self) {
        self.mode = Mode::ConfirmQuit;
    }

    pub fn cancel_quit(&mut self) {
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
                Ok(()) => self.set_status(format!("Copied ticket key '{}' to clipboard", row.issue.key)),
                Err(e) => self.set_status(format!("Copy failed: {e}")),
            }
        }
    }

    pub fn open_highlight_picker(&mut self) {
        if self.rows.get(self.selected).is_none() {
            return;
        }
        // Pre-select current highlight if one exists
        let key = &self.rows[self.selected].issue.key;
        self.highlight_selected = match self.highlighted_keys.get(key).and_then(|s| HighlightColor::from_str(s)) {
            Some(HighlightColor::Orange) => 0,
            Some(HighlightColor::Green) => 1,
            None => 0,
        };
        self.mode = Mode::HighlightPicker;
    }

    pub fn highlight_picker_up(&mut self) {
        if self.highlight_selected > 0 {
            self.highlight_selected -= 1;
        }
    }

    pub fn highlight_picker_down(&mut self) {
        let max = if self.current_highlight().is_some() {
            HIGHLIGHT_OPTIONS.len() // includes "Remove" at index len
        } else {
            HIGHLIGHT_OPTIONS.len() - 1
        };
        if self.highlight_selected < max {
            self.highlight_selected += 1;
        }
    }

    pub fn apply_highlight(&mut self) {
        if let Some(row) = self.rows.get(self.selected) {
            let key = row.issue.key.clone();
            if self.highlight_selected < HIGHLIGHT_OPTIONS.len() {
                let color = HIGHLIGHT_OPTIONS[self.highlight_selected];
                self.highlighted_keys.insert(key, color.as_str().to_string());
            } else {
                // "Remove" option
                self.highlighted_keys.remove(&key);
            }
            notes::save_highlights(&self.highlighted_keys);
        }
        self.mode = Mode::Normal;
    }

    pub fn cancel_highlight_picker(&mut self) {
        self.mode = Mode::Normal;
    }

    pub fn current_highlight(&self) -> Option<HighlightColor> {
        self.rows
            .get(self.selected)
            .and_then(|row| self.highlighted_keys.get(&row.issue.key))
            .and_then(|s| HighlightColor::from_str(s))
    }

    // --- Sort picker ---

    pub fn open_sort_picker(&mut self) {
        self.sort_selected = SortCriteria::ALL
            .iter()
            .position(|c| *c == self.sort_criteria)
            .unwrap_or(0);
        self.mode = Mode::SortPicker;
    }

    pub fn sort_picker_up(&mut self) {
        if self.sort_selected > 0 {
            self.sort_selected -= 1;
        }
    }

    pub fn sort_picker_down(&mut self) {
        if self.sort_selected < SortCriteria::ALL.len() - 1 {
            self.sort_selected += 1;
        }
    }

    pub fn apply_sort(&mut self) {
        self.sort_criteria = SortCriteria::ALL[self.sort_selected];
        self.config.sort_order = Some(self.sort_criteria.as_str().to_string());
        self.config.save();
        self.sort_rows();
        self.apply_search_filter();
        self.mode = Mode::Normal;
    }

    pub fn cancel_sort_picker(&mut self) {
        self.mode = Mode::Normal;
    }

    // --- Column picker ---

    pub fn is_column_visible(&self, col: Column) -> bool {
        !self.config.hidden_columns.iter().any(|s| s == col.as_str())
    }

    pub fn open_column_picker(&mut self) {
        self.column_picker_selected = 0;
        self.mode = Mode::ColumnPicker;
    }

    pub fn column_picker_up(&mut self) {
        if self.column_picker_selected > 0 {
            self.column_picker_selected -= 1;
        }
    }

    pub fn column_picker_down(&mut self) {
        if self.column_picker_selected < Column::ALL.len() - 1 {
            self.column_picker_selected += 1;
        }
    }

    pub fn toggle_column_visibility(&mut self) {
        let col = Column::ALL[self.column_picker_selected];
        let key = col.as_str().to_string();
        if let Some(pos) = self.config.hidden_columns.iter().position(|s| s == &key) {
            self.config.hidden_columns.remove(pos);
        } else {
            self.config.hidden_columns.push(key);
        }
        self.config.save();
    }

    pub fn close_column_picker(&mut self) {
        self.mode = Mode::Normal;
    }

    pub fn sort_rows(&mut self) {
        match self.sort_criteria {
            SortCriteria::Default => {
                self.all_rows.sort_by_key(|r| r.original_index);
            }
            SortCriteria::Board => {
                self.all_rows.sort_by(|a, b| {
                    let (a_proj, a_num) = split_key(&a.issue.key);
                    let (b_proj, b_num) = split_key(&b.issue.key);
                    a_proj.cmp(&b_proj).then(a_num.cmp(&b_num))
                });
            }
            SortCriteria::Priority => {
                self.all_rows.sort_by(|a, b| {
                    priority_rank(&b.issue.priority)
                        .cmp(&priority_rank(&a.issue.priority))
                        .then(a.original_index.cmp(&b.original_index))
                });
            }
            SortCriteria::Muted => {
                let muted = &self.muted_keys;
                self.all_rows.sort_by(|a, b| {
                    let a_muted = muted.contains(&a.issue.key);
                    let b_muted = muted.contains(&b.issue.key);
                    a_muted
                        .cmp(&b_muted)
                        .then(a.original_index.cmp(&b.original_index))
                });
            }
            SortCriteria::Highlight => {
                let highlights = &self.highlighted_keys;
                self.all_rows.sort_by(|a, b| {
                    let a_rank = highlight_rank(highlights.get(&a.issue.key).map(|s| s.as_str()));
                    let b_rank = highlight_rank(highlights.get(&b.issue.key).map(|s| s.as_str()));
                    a_rank
                        .cmp(&b_rank)
                        .then(a.original_index.cmp(&b.original_index))
                });
            }
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
        self.set_status(format!("Loading {key}..."));
        match jira::fetch_issue_detail(&self.config, &key).await {
            Ok(detail) => {
                self.detail = Some(detail);
                self.detail_content_version.set(self.detail_content_version.get() + 1);
                self.detail_scroll = 0;
                self.mode = Mode::TicketDetail;
                self.status_msg.clear();
                self.detail_status_msg.clear();
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
        self.detail_status_msg.clear();
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

    pub fn copy_link_to_clipboard(&mut self) {
        if let Some(ref detail) = self.detail {
            let url = format!(
                "{}/browse/{}",
                self.config.jira_url.trim_end_matches('/'),
                detail.key
            );
            match copy_to_clipboard(&url) {
                Ok(()) => self.set_detail_status("Issue link copied to clipboard"),
                Err(e) => self.set_detail_status(format!("Copy failed: {e}")),
            }
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
            Ok(()) => self.set_detail_status("Ticket contents copied to clipboard"),
            Err(e) => self.set_detail_status(format!("Copy failed: {e}")),
        }
    }

    pub fn start_adding_comment(&mut self) {
        self.comment_input.clear();
        self.cursor_pos = 0;
        self.mention = None;
        self.last_mention_query.clear();
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
            self.set_detail_status("Can only edit your own comments");
            return;
        }
        self.comment_input = comment.body.clone();
        self.cursor_pos = self.comment_input.chars().count();
        self.editing_comment_id = Some(comment.id.clone());
        self.mention = None;
        self.last_mention_query.clear();
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
                    self.set_detail_status("Can only delete your own comments");
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
        self.last_mention_query.clear();
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
        let mentions = self.build_mention_inserts();
        self.set_detail_status("Adding comment...");
        match jira::add_comment(&self.config, &key, &text, &mentions).await {
            Ok(()) => {
                self.set_detail_status("Comment added");
                self.comment_input.clear();
                self.mention = None;
                self.resolved_mentions.clear();
                self.mode = Mode::TicketDetail;
                self.refresh_detail(&key).await;
            }
            Err(e) => {
                self.set_detail_status(format!("Error: {e}"));
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
        let mentions = self.build_mention_inserts();
        self.set_detail_status("Updating comment...");
        match jira::update_comment(&self.config, &key, &comment_id, &text, &mentions).await {
            Ok(()) => {
                self.set_detail_status("Comment updated");
                self.comment_input.clear();
                self.editing_comment_id = None;
                self.mention = None;
                self.resolved_mentions.clear();
                self.mode = Mode::TicketDetail;
                self.refresh_detail(&key).await;
            }
            Err(e) => {
                self.set_detail_status(format!("Error: {e}"));
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
        self.set_detail_status("Deleting comment...");
        match jira::delete_comment(&self.config, &key, &comment_id).await {
            Ok(()) => {
                self.set_detail_status("Comment deleted");
                self.detail_comment_selected = None;
                self.mode = Mode::TicketDetail;
                self.refresh_detail(&key).await;
            }
            Err(e) => {
                self.set_detail_status(format!("Error: {e}"));
                self.mode = Mode::TicketDetail;
            }
        }
    }

    async fn refresh_detail(&mut self, key: &str) {
        match jira::fetch_issue_detail(&self.config, key).await {
            Ok(detail) => {
                self.detail = Some(detail);
                self.detail_content_version.set(self.detail_content_version.get() + 1);
            }
            Err(e) => {
                self.set_detail_status(format!("Error refreshing: {e}"));
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
        self.last_mention_query.clear();
    }

    pub async fn fetch_mention_candidates(&mut self) {
        let query = match &self.mention {
            Some(m) => m.query.clone(),
            None => return,
        };
        if query.is_empty() {
            self.last_mention_query.clear();
            if let Some(ref mut mention) = self.mention {
                mention.candidates.clear();
                mention.selected = 0;
            }
            return;
        }
        if query == self.last_mention_query {
            return;
        }
        self.last_mention_query = query.clone();
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
        let key = match &self.detail {
            Some(d) => d.key.clone(),
            None => return,
        };
        self.set_detail_status("Loading transitions...");
        match jira::fetch_transitions(&self.config, &key).await {
            Ok(transitions) => {
                if transitions.is_empty() {
                    self.set_detail_status("No transitions available");
                    return;
                }
                self.transitions = transitions;
                self.transition_selected = 0;
                self.mode = Mode::DetailTransition;
                self.detail_status_msg.clear();
            }
            Err(e) => {
                self.set_detail_status(format!("Error: {e}"));
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
        self.set_detail_status(format!("Transitioning to {name}..."));
        match jira::do_transition(&self.config, &key, &id).await {
            Ok(()) => {
                self.transitions.clear();
                self.mode = Mode::TicketDetail;
                self.refresh().await;
                self.refresh_detail(&key).await;
                self.set_detail_status(format!("Transitioned to {name}"));
            }
            Err(e) => {
                self.set_detail_status(format!("Error: {e}"));
                self.transitions.clear();
                self.mode = Mode::TicketDetail;
            }
        }
    }

    // --- Summary editing ---

    pub fn start_editing_summary(&mut self) {
        let detail = match &self.detail {
            Some(d) => d,
            None => return,
        };
        if detail.reporter_account_id != self.current_account_id {
            self.set_detail_status("Can only edit summaries of tickets you reported");
            return;
        }
        self.summary_input = detail.summary.clone();
        self.cursor_pos = self.summary_input.chars().count();
        self.mode = Mode::DetailEditingSummary;
    }

    pub fn cancel_editing_summary(&mut self) {
        self.summary_input.clear();
        self.mode = Mode::TicketDetail;
    }

    pub async fn save_summary(&mut self) {
        let text = self.summary_input.trim().to_string();
        if text.is_empty() {
            self.cancel_editing_summary();
            return;
        }
        let key = match &self.detail {
            Some(d) => d.key.clone(),
            None => return,
        };
        self.set_detail_status("Updating summary...");
        match jira::update_summary(&self.config, &key, &text).await {
            Ok(()) => {
                self.set_detail_status("Summary updated");
                self.summary_input.clear();
                self.mode = Mode::TicketDetail;
                self.refresh().await;
                self.refresh_detail(&key).await;
            }
            Err(e) => {
                self.set_detail_status(format!("Error: {e}"));
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

    // --- GitHub PR list ---

    pub async fn open_pr_list(&mut self) {
        let key = match self.detail.as_ref() {
            Some(d) => d.key.clone(),
            None => return,
        };
        self.set_detail_status(format!("Fetching PRs for {key}..."));
        let repo = self.config.github_repo.as_deref();
        match crate::github::fetch_prs_for_ticket(repo, &key).await {
            Ok(prs) => {
                self.pr_list = prs;
                self.pr_list_selected = 0;
                self.detail_status_msg.clear();
                self.mode = Mode::DetailPRList;
            }
            Err(e) => {
                self.set_detail_status(format!("Error: {e}"));
            }
        }
    }

    pub fn close_pr_list(&mut self) {
        self.pr_list.clear();
        self.pr_list_selected = 0;
        self.mode = Mode::TicketDetail;
    }

    pub fn pr_list_move_up(&mut self) {
        if self.pr_list_selected > 0 {
            self.pr_list_selected -= 1;
        }
    }

    pub fn pr_list_move_down(&mut self) {
        if self.pr_list_selected + 1 < self.pr_list.len() {
            self.pr_list_selected += 1;
        }
    }

    pub fn open_selected_pr(&self) {
        if let Some(pr) = self.pr_list.get(self.pr_list_selected) {
            let _ = open::that(&pr.html_url);
        }
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

fn split_key(key: &str) -> (&str, u64) {
    match key.rsplit_once('-') {
        Some((prefix, num)) => (prefix, num.parse().unwrap_or(0)),
        None => (key, 0),
    }
}

fn highlight_rank(color: Option<&str>) -> u8 {
    match color {
        Some("orange") => 0,
        Some("green") => 1,
        _ => 2,
    }
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
