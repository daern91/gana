# ☸ gana — Branding Guide

## Name
gana (GAH-nah)
Sanskrit: गण — "troop, attendant, follower"

## Tagline
"Orchestrate your divine helpers"

## Icon
☸ (Wheel of Dharma, U+2638)

## The Story
In Hindu mythology, ganas are troops of divine attendants who serve
Ganesha — the elephant-headed god, remover of obstacles. Ganesha
literally means "lord of the ganas." When you launch gana, you become
Ganesha: the orchestrator of divine helpers that remove obstacles and
get work done.

## Hierarchy
gana (the tool) = Ganesha, lord of all the ganas
  └── Session 1 = a gana (Claude agent team: lead + teammates + tasks)
  └── Session 2 = a gana (Claude agent team: lead + teammates + tasks)
  └── Session 3 = a gana (Claude agent team: lead + teammates + tasks)

## CLI Usage
$ gana              # launch TUI
$ gana new          # create a new session
$ gana --help       # see all commands
$ gana reset        # clean up all sessions
$ gana debug        # show config info

## --help Output
☸ gana — orchestrate your divine helpers
Usage: gana [COMMAND]
Commands: reset, debug, daemon, stop-daemon

## README Header
# ☸ gana
> Orchestrate AI agent teams like a god. Each session is a gana —
> a self-coordinating troop of Claude agents. You are Ganesha.

```
brew install gana
gana         # launch TUI
gana --help  # see all commands
```

## In-Use Patterns
☸  Spawning agent team...
☸  3 sessions active
☸  test-session ── ● running ── +15 -3

## Spinner
☸  thinking...    (idle)
☸⟳ working...     (frame 1)
☸↻ working...     (frame 2)

## Status Bar
☸ gana ─── 3 sessions ─── ● running  ⏸ paused  ○ ready

## Block Text (for splash screen fallback)
░██████╗░░█████╗░███╗░░██╗░█████╗░
██╔════╝░██╔══██╗████╗░██║██╔══██╗
██║░░██╗░███████║██╔██╗██║███████║
██║░░╚██╗██╔══██║██║╚████║██╔══██║
╚██████╔╝██║░░██║██║░╚███║██║░░██║
░╚═════╝░╚═╝░░╚═╝╚═╝░░╚══╝╚═╝░░╚═╝

## Key Facts
- 4 keystrokes
- ga<TAB> completes instantly (uncrowded prefix)
- crates.io/crates/gana: AVAILABLE
- GitHub: no conflicts in dev tools
- "gana" is a common Sanskrit noun (not a deity name)
- India's official name uses it: Bhārata Gaṇarājya (Republic of India)
