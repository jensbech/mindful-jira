use crate::config::Config;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct JiraUser {
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
}

#[derive(Debug, Clone)]
pub struct MentionInsert {
    pub start: usize,
    pub len: usize,
    pub account_id: String,
    pub display_name: String,
}

#[derive(Debug, Clone)]
pub struct JiraIssue {
    pub key: String,
    pub summary: String,
    pub assignee: String,
    pub reporter: String,
    pub priority: String,
    pub status: String,
    pub resolution: String,
    pub created: String,
    pub issue_type: String,
    pub parent_key: Option<String>,
    pub is_subtask: bool,
    pub is_context_parent: bool,
}

pub struct IssueDetail {
    pub key: String,
    pub issue_type: String,
    pub status: String,
    pub summary: String,
    pub description: String,
    pub comments: Vec<Comment>,
}

pub struct Comment {
    pub id: String,
    pub author: String,
    pub author_account_id: String,
    pub created: String,
    pub body: String,
}

#[derive(Deserialize)]
struct SearchResponse {
    issues: Vec<RawIssue>,
}

#[derive(Deserialize)]
struct RawIssue {
    key: String,
    fields: RawFields,
}

#[derive(Deserialize)]
struct RawFields {
    summary: Option<String>,
    assignee: Option<NameField>,
    reporter: Option<NameField>,
    priority: Option<NameField>,
    status: Option<NameField>,
    resolution: Option<NameField>,
    created: Option<String>,
    parent: Option<ParentField>,
    issuetype: Option<IssueTypeField>,
}

#[derive(Deserialize)]
struct NameField {
    #[serde(alias = "displayName")]
    name: Option<String>,
}

#[derive(Deserialize)]
struct ParentField {
    key: Option<String>,
}

#[derive(Deserialize)]
struct IssueTypeField {
    name: Option<String>,
    subtask: Option<bool>,
}

// --- Current user ---

pub async fn fetch_current_account_id(config: &Config) -> Result<String, String> {
    let url = format!(
        "{}/rest/api/3/myself",
        config.jira_url.trim_end_matches('/')
    );
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .basic_auth(&config.email, Some(&config.api_token))
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Jira API error {status}: {body}"));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse: {e}"))?;
    Ok(json["accountId"]
        .as_str()
        .unwrap_or("")
        .to_string())
}

// --- User search ---

pub async fn search_users(config: &Config, query: &str) -> Result<Vec<JiraUser>, String> {
    let url = format!(
        "{}/rest/api/3/user/search",
        config.jira_url.trim_end_matches('/')
    );
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .basic_auth(&config.email, Some(&config.api_token))
        .query(&[("query", query), ("maxResults", "8")])
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Jira API error {status}: {body}"));
    }

    let users: Vec<JiraUser> = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse: {e}"))?;
    Ok(users)
}

// --- Issue list ---

pub async fn fetch_issues(
    config: &Config,
    show_all_parents: bool,
) -> Result<Vec<JiraIssue>, String> {
    let excluded = config.excluded_status_names();
    let jql = if excluded.is_empty() {
        "assignee = currentUser() ORDER BY priority DESC, updated DESC".to_string()
    } else {
        let list = excluded
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(", ");
        format!("assignee = currentUser() AND status NOT IN ({list}) ORDER BY priority DESC, updated DESC")
    };

    let client = reqwest::Client::new();
    let mut issues = search_issues(&client, config, &jql).await?;

    let issue_keys: std::collections::HashSet<String> =
        issues.iter().map(|i| i.key.clone()).collect();
    let missing_parents: Vec<String> = issues
        .iter()
        .filter_map(|i| i.parent_key.as_ref())
        .filter(|pk| !issue_keys.contains(*pk))
        .cloned()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    if !missing_parents.is_empty() {
        let keys_jql = missing_parents
            .iter()
            .map(|k| format!("\"{k}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let parent_jql = if show_all_parents {
            format!("key in ({keys_jql})")
        } else {
            format!("key in ({keys_jql}) AND assignee = currentUser()")
        };
        if let Ok(mut parents) = search_issues(&client, config, &parent_jql).await {
            for p in &mut parents {
                p.parent_key = None;
                p.is_subtask = false;
                p.is_context_parent = true;
            }
            issues.extend(parents);
        }
    }

    Ok(build_tree(issues))
}

async fn search_issues(
    client: &reqwest::Client,
    config: &Config,
    jql: &str,
) -> Result<Vec<JiraIssue>, String> {
    let fields =
        "key,summary,assignee,reporter,priority,status,resolution,created,parent,issuetype,subtasks";
    let url = format!(
        "{}/rest/api/3/search/jql",
        config.jira_url.trim_end_matches('/')
    );

    let resp = client
        .get(&url)
        .basic_auth(&config.email, Some(&config.api_token))
        .query(&[
            ("jql", jql),
            ("fields", fields),
            ("maxResults", "100"),
        ])
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Jira API error {status}: {body}"));
    }

    let search: SearchResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Jira response: {e}"))?;

    Ok(search
        .issues
        .into_iter()
        .map(|raw| {
            let f = raw.fields;
            JiraIssue {
                key: raw.key,
                summary: f.summary.unwrap_or_default(),
                assignee: f.assignee.and_then(|a| a.name).unwrap_or_default(),
                reporter: f.reporter.and_then(|r| r.name).unwrap_or_default(),
                priority: f.priority.and_then(|p| p.name).unwrap_or_default(),
                status: f.status.and_then(|s| s.name).unwrap_or_default(),
                resolution: f
                    .resolution
                    .and_then(|r| r.name)
                    .unwrap_or_else(|| "Unresolved".to_string()),
                created: f.created.map(|c| format_date(&c)).unwrap_or_default(),
                issue_type: f.issuetype.as_ref().and_then(|t| t.name.clone()).unwrap_or_default(),
                parent_key: f.parent.and_then(|p| p.key),
                is_subtask: f.issuetype.and_then(|t| t.subtask).unwrap_or(false),
                is_context_parent: false,
            }
        })
        .collect())
}

// --- Issue detail ---

pub async fn fetch_issue_detail(
    config: &Config,
    key: &str,
) -> Result<IssueDetail, String> {
    let url = format!(
        "{}/rest/api/3/issue/{}",
        config.jira_url.trim_end_matches('/'),
        key
    );

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .basic_auth(&config.email, Some(&config.api_token))
        .query(&[("fields", "summary,description,comment,issuetype,status")])
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Jira API error {status}: {body}"));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {e}"))?;

    let fields = &json["fields"];

    let summary = fields["summary"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let description = if fields["description"].is_null() {
        "(no description)".to_string()
    } else {
        adf_to_text(&fields["description"])
            .trim()
            .to_string()
    };

    let comments = fields["comment"]["comments"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|c| Comment {
                    id: c["id"]
                        .as_str()
                        .unwrap_or("")
                        .to_string(),
                    author: c["author"]["displayName"]
                        .as_str()
                        .unwrap_or("")
                        .to_string(),
                    author_account_id: c["author"]["accountId"]
                        .as_str()
                        .unwrap_or("")
                        .to_string(),
                    created: c["created"]
                        .as_str()
                        .map(|s| format_date(s))
                        .unwrap_or_default(),
                    body: adf_to_text(&c["body"]).trim().to_string(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let comments = {
        let mut c = comments;
        c.reverse();
        c
    };

    let issue_type = fields["issuetype"]["name"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let status = fields["status"]["name"]
        .as_str()
        .unwrap_or("")
        .to_string();

    Ok(IssueDetail {
        key: key.to_string(),
        issue_type,
        status,
        summary,
        description,
        comments,
    })
}

// --- ADF (Atlassian Document Format) to plain text ---

fn adf_to_text(value: &serde_json::Value) -> String {
    match value.get("type").and_then(|t| t.as_str()) {
        Some("doc") => adf_children_text(value),
        Some("paragraph") => {
            let t = adf_children_text(value);
            format!("{t}\n")
        }
        Some("heading") => {
            let level = value
                .get("attrs")
                .and_then(|a| a.get("level"))
                .and_then(|l| l.as_u64())
                .unwrap_or(1) as usize;
            let prefix = "#".repeat(level);
            let t = adf_children_text(value);
            format!("{prefix} {t}\n")
        }
        Some("text") => {
            let raw = value
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("");
            let marks = value.get("marks").and_then(|m| m.as_array());
            format_text_with_marks(raw, marks)
        }
        Some("hardBreak") => "\n".to_string(),
        Some("bulletList") => adf_children_text(value),
        Some("orderedList") => {
            value
                .get("content")
                .and_then(|c| c.as_array())
                .map(|arr| {
                    arr.iter()
                        .enumerate()
                        .map(|(i, item)| {
                            let t = adf_children_text(item);
                            format!("  {}. {}", i + 1, t)
                        })
                        .collect::<String>()
                })
                .unwrap_or_default()
        }
        Some("listItem") => {
            let t = adf_children_text(value);
            format!("  - {t}")
        }
        Some("blockquote") => {
            let t = adf_children_text(value);
            t.lines().map(|l| format!("> {l}\n")).collect()
        }
        Some("codeBlock") => {
            let lang = value
                .get("attrs")
                .and_then(|a| a.get("language"))
                .and_then(|l| l.as_str())
                .unwrap_or("");
            let t = adf_children_text(value);
            format!("```{lang}\n{t}```\n")
        }
        Some("mention") => value
            .get("attrs")
            .and_then(|a| a.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("@someone")
            .to_string(),
        Some("emoji") => value
            .get("attrs")
            .and_then(|a| a.get("shortName"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string(),
        Some("inlineCard") => {
            let url = value
                .get("attrs")
                .and_then(|a| a.get("url"))
                .and_then(|t| t.as_str())
                .unwrap_or("[link]");
            url.to_string()
        }
        Some("mediaGroup") | Some("mediaSingle") => "[media]\n".to_string(),
        Some("media") => "[media]".to_string(),
        Some("rule") => "────────\n".to_string(),
        Some("table") | Some("tableRow") | Some("tableCell") | Some("tableHeader") => {
            let t = adf_children_text(value);
            format!("{t}\n")
        }
        _ => adf_children_text(value),
    }
}

fn format_text_with_marks(text: &str, marks: Option<&Vec<serde_json::Value>>) -> String {
    let marks = match marks {
        Some(m) if !m.is_empty() => m,
        _ => return text.to_string(),
    };

    let mut result = text.to_string();

    for mark in marks {
        match mark.get("type").and_then(|t| t.as_str()) {
            Some("strong") => result = format!("**{result}**"),
            Some("em") => result = format!("_{result}_"),
            Some("code") => result = format!("`{result}`"),
            Some("strike") => result = format!("~{result}~"),
            Some("link") => {} // bare URLs are auto-detected and colored
            _ => {}
        }
    }

    result
}

fn adf_children_text(value: &serde_json::Value) -> String {
    value
        .get("content")
        .and_then(|c| c.as_array())
        .map(|arr| arr.iter().map(adf_to_text).collect::<String>())
        .unwrap_or_default()
}

// --- Helpers ---

fn format_date(iso: &str) -> String {
    iso.get(..10).unwrap_or(iso).to_string()
}

// --- Comment CRUD ---

/// Split a plain text segment into text nodes and inlineCard nodes for any URLs found.
fn text_to_adf_nodes(segment: &str) -> Vec<serde_json::Value> {
    let mut nodes: Vec<serde_json::Value> = Vec::new();
    let mut remaining = segment;

    while !remaining.is_empty() {
        let url_start = remaining
            .find("https://")
            .or_else(|| remaining.find("http://"));

        match url_start {
            Some(start) => {
                if start > 0 {
                    nodes.push(serde_json::json!({ "type": "text", "text": &remaining[..start] }));
                }
                let url_part = &remaining[start..];
                let url_end = url_part
                    .find(|c: char| c.is_whitespace())
                    .unwrap_or(url_part.len());
                let url = &url_part[..url_end];
                nodes.push(serde_json::json!({
                    "type": "inlineCard",
                    "attrs": { "url": url }
                }));
                remaining = &url_part[url_end..];
            }
            None => {
                nodes.push(serde_json::json!({ "type": "text", "text": remaining }));
                break;
            }
        }
    }

    nodes
}

fn text_to_adf(text: &str, mentions: &[MentionInsert]) -> serde_json::Value {
    // Build a sorted list of mentions by start position
    let mut sorted_mentions: Vec<&MentionInsert> = mentions.iter().collect();
    sorted_mentions.sort_by_key(|m| m.start);

    let chars: Vec<char> = text.chars().collect();
    let paragraphs: Vec<serde_json::Value> = text
        .split('\n')
        .enumerate()
        .scan(0usize, |char_offset, (_line_idx, line)| {
            let line_start = *char_offset;
            let line_char_count = line.chars().count();
            let line_end = line_start + line_char_count;

            // Find mentions that fall within this line
            let line_mentions: Vec<&&MentionInsert> = sorted_mentions
                .iter()
                .filter(|m| m.start >= line_start && m.start < line_end)
                .collect();

            let content = if line_mentions.is_empty() {
                text_to_adf_nodes(line)
            } else {
                let mut nodes: Vec<serde_json::Value> = Vec::new();
                let mut pos = line_start;
                for mention in &line_mentions {
                    if mention.start > pos {
                        let segment: String = chars[pos..mention.start].iter().collect();
                        if !segment.is_empty() {
                            nodes.extend(text_to_adf_nodes(&segment));
                        }
                    }
                    nodes.push(serde_json::json!({
                        "type": "mention",
                        "attrs": {
                            "id": mention.account_id,
                            "text": format!("@{}", mention.display_name),
                            "accessLevel": ""
                        }
                    }));
                    pos = mention.start + mention.len;
                }
                if pos < line_end {
                    let segment: String = chars[pos..line_end].iter().collect();
                    if !segment.is_empty() {
                        nodes.extend(text_to_adf_nodes(&segment));
                    }
                }
                if nodes.is_empty() {
                    vec![serde_json::json!({ "type": "text", "text": "" })]
                } else {
                    nodes
                }
            };

            // +1 for the newline character
            *char_offset = line_end + 1;

            Some(serde_json::json!({
                "type": "paragraph",
                "content": content
            }))
        })
        .collect();

    serde_json::json!({
        "type": "doc",
        "version": 1,
        "content": paragraphs
    })
}

pub async fn add_comment(
    config: &Config,
    issue_key: &str,
    body_text: &str,
    mentions: &[MentionInsert],
) -> Result<(), String> {
    let url = format!(
        "{}/rest/api/3/issue/{}/comment",
        config.jira_url.trim_end_matches('/'),
        issue_key
    );

    let payload = serde_json::json!({ "body": text_to_adf(body_text, mentions) });

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .basic_auth(&config.email, Some(&config.api_token))
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Jira API error {status}: {body}"));
    }

    Ok(())
}

pub async fn update_comment(
    config: &Config,
    issue_key: &str,
    comment_id: &str,
    body_text: &str,
    mentions: &[MentionInsert],
) -> Result<(), String> {
    let url = format!(
        "{}/rest/api/3/issue/{}/comment/{}",
        config.jira_url.trim_end_matches('/'),
        issue_key,
        comment_id
    );

    let payload = serde_json::json!({ "body": text_to_adf(body_text, mentions) });

    let client = reqwest::Client::new();
    let resp = client
        .put(&url)
        .basic_auth(&config.email, Some(&config.api_token))
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Jira API error {status}: {body}"));
    }

    Ok(())
}

pub async fn delete_comment(
    config: &Config,
    issue_key: &str,
    comment_id: &str,
) -> Result<(), String> {
    let url = format!(
        "{}/rest/api/3/issue/{}/comment/{}",
        config.jira_url.trim_end_matches('/'),
        issue_key,
        comment_id
    );

    let client = reqwest::Client::new();
    let resp = client
        .delete(&url)
        .basic_auth(&config.email, Some(&config.api_token))
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Jira API error {status}: {body}"));
    }

    Ok(())
}

// --- Transitions ---

pub struct Transition {
    pub id: String,
    pub name: String,
    pub to_status: String,
}

pub async fn fetch_transitions(
    config: &Config,
    issue_key: &str,
) -> Result<Vec<Transition>, String> {
    let url = format!(
        "{}/rest/api/3/issue/{}/transitions",
        config.jira_url.trim_end_matches('/'),
        issue_key
    );

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .basic_auth(&config.email, Some(&config.api_token))
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Jira API error {status}: {body}"));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse: {e}"))?;

    let transitions = json["transitions"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|t| Transition {
                    id: t["id"].as_str().unwrap_or("").to_string(),
                    name: t["name"].as_str().unwrap_or("").to_string(),
                    to_status: t["to"]["name"].as_str().unwrap_or("").to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(transitions)
}

pub async fn do_transition(
    config: &Config,
    issue_key: &str,
    transition_id: &str,
) -> Result<(), String> {
    let url = format!(
        "{}/rest/api/3/issue/{}/transitions",
        config.jira_url.trim_end_matches('/'),
        issue_key
    );

    let payload = serde_json::json!({
        "transition": { "id": transition_id }
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .basic_auth(&config.email, Some(&config.api_token))
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Jira API error {status}: {body}"));
    }

    Ok(())
}

// --- Tree building ---

fn build_tree(issues: Vec<JiraIssue>) -> Vec<JiraIssue> {
    use std::collections::HashMap;

    let mut parent_issues: Vec<JiraIssue> = Vec::new();
    let mut children: HashMap<String, Vec<JiraIssue>> = HashMap::new();

    for issue in issues {
        if let Some(ref pk) = issue.parent_key {
            children.entry(pk.clone()).or_default().push(issue);
        } else {
            parent_issues.push(issue);
        }
    }

    let mut result = Vec::new();
    for parent in parent_issues {
        let key = parent.key.clone();
        result.push(parent);
        if let Some(kids) = children.remove(&key) {
            for child in kids {
                result.push(child);
            }
        }
    }

    for (_key, orphans) in children {
        for mut orphan in orphans {
            orphan.parent_key = None;
            orphan.is_subtask = false;
            result.push(orphan);
        }
    }

    result
}
