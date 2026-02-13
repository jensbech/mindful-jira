# Mindful Jira

A terminal UI for viewing and managing your Jira issues, built with Rust and [ratatui](https://github.com/ratatui/ratatui).

## Setup

1. Copy `jira-config.json.example` to `jira-config.json` and fill in your Jira URL, email, and API token.
2. `cargo run`

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
