# Mindful Jira

A terminal UI for viewing and managing your Jira issues, built with Rust and [ratatui](https://github.com/ratatui/ratatui).

![Issue list with notes, highlights, and parent/child grouping](screenshots/issue-list.png)

![Ticket detail with description, code blocks, and blockquotes](screenshots/ticket-detail.png)

![Comments section with inline markdown and comment input](screenshots/comments.png)

## Install

```bash
curl -fsSL https://git.bechsor.no/jens/mindful-jira/raw/branch/main/install | bash
```

## Setup

1. Copy `jira-config.json.example` to `jira-config.json` and fill in your Jira URL, email, and API token.
2. `mindful-jira`

## Development

```bash
cargo build                # debug build
just build                 # release build (current arch)
just release               # release binary for distribution
just lint                  # format + clippy
```

## Keybindings

### Issue List

| Key | Action |
|-----|--------|
| `j/k` / arrows | Navigate issues |
| `Enter` | Open issue in browser |
| `d` | View ticket detail |
| `n` | Edit local note |
| `f` | Filter editor |
| `p` | Toggle parent issues |
| `r` | Refresh |
| `?` | Toggle legend |
| `q` / `Esc` | Quit |

### Ticket Detail

| Key | Action |
|-----|--------|
| `j/k` / arrows | Scroll |
| `n/p` | Next/previous comment |
| `c` | Add comment |
| `e` | Edit comment |
| `x` | Delete comment |
| `y` | Copy ticket to clipboard |
| `Enter` | Open in browser |
| `Esc` | Close detail |
