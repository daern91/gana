# ☸ gana

> Orchestrate your AI agent teams

```
░██████╗░░█████╗░███╗░░██╗░█████╗░
██╔════╝░██╔══██╗████╗░██║██╔══██╗
██║░░██╗░███████║██╔██╗██║███████║
██║░░╚██╗██╔══██║██║╚████║██╔══██║
╚██████╔╝██║░░██║██║░╚███║██║░░██║
░╚═════╝░╚═╝░░╚═╝╚═╝░░╚══╝╚═╝░░╚═╝
```

**gana** manages multiple [Claude Code agent teams](https://code.claude.com/docs/en/agent-teams) in parallel, each in its own isolated git worktree. Create sessions, monitor progress, view diffs, and switch between teams — all from a terminal UI.

Named after the Sanskrit word गण (*gaṇa*) — a troop of attendants. In Hindu mythology, Ganesha (lord of the ganas) orchestrates these troops to remove obstacles. Each session you create is a gana: a self-coordinating team of AI agents.

## Features

- **Multiple sessions** — Run several Claude Code agent teams simultaneously
- **Git worktree isolation** — Each session works in its own worktree, no conflicts
- **Live preview** — Watch agent output in real-time with ANSI color support
- **Git diff view** — See what changed at a glance (+N/-M stats)
- **Ctrl+Q attach/detach** — Drop into any session, Ctrl+Q to return
- **Trust prompt auto-response** — Handles Claude/Aider/Gemini trust prompts automatically
- **Background daemon** — Monitor sessions and auto-respond when you're away
- **Vim-style navigation** — j/k, scroll, tabs — feels like home

## Install

### Quick install

```bash
curl -fsSL https://raw.githubusercontent.com/daern91/gana/master/install.sh | bash
```

### From source (requires Rust)

```bash
cargo install --git https://github.com/daern91/gana.git
```

### Prerequisites

- **tmux** — `brew install tmux` (macOS) or `apt install tmux` (Linux)
- **Claude Code** — or any supported AI assistant (Aider, Gemini, Codex, Amp)

## Quick Start

```bash
gana              # Launch the TUI
```

### Key Bindings

| Key | Action |
|-----|--------|
| `n` | New session |
| `N` | New session with prompt |
| `Enter` / `a` | Attach to session (Ctrl+Q to detach) |
| `j/k` or `Up/Down` | Navigate sessions |
| `Tab` | Switch Preview/Diff |
| `K/J` | Scroll preview up/down |
| `Esc` | Reset scroll |
| `d` | Delete session |
| `D` | Kill session (force) |
| `?` | Toggle help |
| `q` | Quit |

### CLI Commands

```bash
gana                # Launch TUI
gana reset          # Clean up all sessions
gana debug          # Show config info
gana daemon         # Start background daemon
gana stop-daemon    # Stop daemon
```

## Configuration

Config file: `~/.gana/config.json`

```json
{
  "default_program": "claude",
  "auto_yes": false,
  "daemon_poll_interval": 1000,
  "branch_prefix": "gana/"
}
```

| Option | Default | Description |
|--------|---------|-------------|
| `default_program` | `"claude"` | AI assistant to launch (`claude`, `aider`, `gemini`, `codex`, `amp`) |
| `auto_yes` | `false` | Auto-respond to trust/permission prompts |
| `daemon_poll_interval` | `1000` | Daemon poll interval in milliseconds |
| `branch_prefix` | `"gana/"` | Prefix for git branch names |

## Architecture

gana is a Rust port of [claude-squad](https://github.com/smtg-ai/claude-squad) (Go), rebuilt with:

- **ratatui** — Terminal UI (replaces Bubble Tea)
- **crossterm** — Cross-platform terminal handling
- **tokio** — Async runtime
- **serde** — JSON config/state persistence
- **nix** — Unix signal handling, PTY management
- **portable-pty** — PTY session management

```
gana (the tool)
  └── Session 1 → Claude agent team (lead + teammates + tasks)
  └── Session 2 → Claude agent team (lead + teammates + tasks)
  └── Session 3 → Claude agent team (lead + teammates + tasks)
```

## Development

```bash
cargo test             # Run 147 tests
cargo clippy           # Lint
cargo fmt              # Format
cargo build --release  # Build release binary
```

## License

MIT
