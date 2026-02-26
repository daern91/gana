# ☸ gana — Roadmap & TODO

## Shipped (v0.1.0 + v0.1.1)

- [x] Full Rust port of claude-squad
- [x] Async TUI — background tick, no UI freezing
- [x] Animated Ganesha loading screen (swaying + status messages)
- [x] Ctrl+Q attach/detach with full-screen PTY sizing
- [x] Session persistence across restarts
- [x] Dead session detection (marks Ready when tmux dies)
- [x] Enter on Ready session restarts Claude in same worktree
- [x] Pause/resume (`p` key)
- [x] Push & create PR (`P` key)
- [x] Repo name display when multiple repos in use
- [x] Session restart with options overlay (`r` key)
  - `--dangerously-skip-permissions` toggle
  - Resume last conversation toggle
- [x] Branch name = session name (no prefix mangling)
- [x] 64-char input limit for branch names
- [x] ANSI escape code stripping in preview pane
- [x] Trust prompt auto-response (Claude, Aider, Gemini)
- [x] First-run help overlay
- [x] Menu key highlighting (500ms yellow flash)
- [x] Auto-update from GitHub Releases (silent, background)
- [x] Install script (`curl | bash`)
- [x] Release pipeline (version bump → auto-tag → build 4 platforms → publish)
- [x] 161 tests (151 unit + 10 integration)
- [x] CI pipeline (check, test, clippy, fmt, build)
- [x] README, LICENSE (MIT), branding

## Priority: Bug Fixes

- [ ] **Fix CI test failures** — CI `test` and `clippy` jobs are failing. Likely needs tmux installed in CI or has platform-specific issues. Check GitHub Actions logs.
- [ ] **Clean up orphaned tmux processes** — `tmux attach-session` processes leak when sessions are created/restored. Each `restore_session()` spawns a `tmux attach-session` subprocess that never gets cleaned up. Need to track child PIDs and kill them on session close/restart.
- [ ] **Preview pane ANSI handling** — the `-e` flag on `capture-pane` preserves colors, but we strip them. Consider either: removing `-e` flag entirely (simpler), or parsing ANSI codes into ratatui styles (colored preview).

## Priority: Most Requested Features (from claude-squad issues)

- [ ] **Choose AI assistant per session** (#84, 3 votes, 6 comments) — when creating a session, let user pick the program (claude, aider, gemini, codex, amp) instead of always using the default. Could be a dropdown in the new session flow, or a config option, or both.
- [ ] **Configurable worktree path** (#121, 6 votes, 7 comments) — let users set `worktree_pattern` in config, e.g. `~/repos/worktrees/{title}` or `{repo_root}/.worktrees/{title}`. Currently hardcoded to `~/.gana/worktrees/{session_id}_{timestamp}`.
- [ ] **Custom config dir via env var** (#245, #246) — `GANA_HOME` environment variable to override `~/.gana/`. Useful for running multiple gana configs or non-standard home dirs.

## Next: New Features

- [ ] **Live agent team status** — the big differentiator from claude-squad. When a session uses Claude Code agent teams, show teammate count, task progress, messages inside each gana session. Would need to parse tmux pane content or read the team config at `~/.claude/teams/`.
- [ ] **Terminal tab** (#172, #247) — third tab alongside Preview/Diff that gives interactive shell access to the worktree directory. Run commands, check logs, run tests without attaching to the full session.
- [ ] **Configurable key mappings** (#213) — let users remap keys via config file.
- [ ] **Multi-repo support** (#56) — run gana from any directory, manage sessions across different repos.

## Polish

- [ ] **README with screenshots/GIF** — show the Ganesha animation, TUI layout, attach flow, restart overlay. A good demo GIF would help adoption.
- [ ] **Colored preview** — parse ANSI codes from tmux and render with ratatui styles instead of stripping them. Would make the preview look like the actual terminal.
- [ ] **Responsive layout** (#146) — adjust list/preview pane split based on terminal width. Currently hardcoded 30/70.
- [ ] **Toast notifications** (#209, #240) — brief feedback messages for git operations (push success, PR created, etc.) instead of error bar.

## Architecture Notes

### Key files
- `src/app/mod.rs` — Main TUI app, event loop, state machine, key handling (~1400 lines)
- `src/session/instance.rs` — Instance lifecycle (start, kill, pause, resume, restart)
- `src/session/tmux/mod.rs` — Tmux session management, PTY, attach/detach
- `src/session/git/worktree*.rs` — Git worktree operations
- `src/ui/` — All ratatui widgets (list, preview, diff, menu, overlays)

### How sessions work
```
gana (TUI)
  └── Instance (title, branch, status)
        ├── GitWorktree (repo_path, worktree_dir, branch)
        └── TmuxSession (PTY fd, sanitized name)
              └── AI assistant process (claude, aider, etc.)
```

### Background threading
- `schedule_background_updates()` — spawns threads for preview capture + diff computation every 500ms
- `process_background_updates()` — drains mpsc channel with non-blocking try_recv()
- `create_instance()` — spawns thread for worktree + tmux creation (async loading spinner)
- Session restart — spawns thread for kill + recreate flow

### Release pipeline
```
Bump Cargo.toml version → push → CI detects new version →
creates git tag → builds binaries (linux x86/arm, macOS x86/arm) →
publishes GitHub Release → users auto-update on next launch
```
