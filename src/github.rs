use serde::Deserialize;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct GithubPR {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub html_url: String,
    pub user: String,
}

#[derive(Deserialize)]
struct RawPR {
    number: u64,
    title: String,
    state: String,
    url: String,
    author: RawAuthor,
}

#[derive(Deserialize)]
struct RawAuthor {
    login: String,
}

/// Fetch PRs associated with `ticket_key` using the `gh` CLI.
///
/// If `repo` is provided ("owner/repo"), searches within that repo via
/// `gh pr list`. Otherwise does a global search via `gh search prs`.
pub async fn fetch_prs_for_ticket(
    repo: Option<&str>,
    ticket_key: &str,
) -> Result<Vec<GithubPR>, String> {
    let output = if let Some(r) = repo {
        Command::new("gh")
            .args([
                "pr", "list",
                "--search", ticket_key,
                "--state", "all",
                "--json", "number,title,state,url,author",
                "--limit", "20",
                "--repo", r,
            ])
            .output()
            .await
            .map_err(|e| format!("Failed to run gh: {e}"))?
    } else {
        Command::new("gh")
            .args([
                "search", "prs",
                ticket_key,
                "--json", "number,title,state,url,author",
                "--limit", "20",
            ])
            .output()
            .await
            .map_err(|e| format!("Failed to run gh: {e}"))?
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let raw: Vec<RawPR> = serde_json::from_str(&stdout)
        .map_err(|e| format!("Failed to parse gh output: {e}"))?;

    Ok(raw
        .into_iter()
        .map(|r| GithubPR {
            number: r.number,
            title: r.title,
            state: r.state.to_lowercase(),
            html_url: r.url,
            user: r.author.login,
        })
        .collect())
}
