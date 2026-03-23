# Ursa

> **Terminal AI Coding Assistant — Persistent Memory, Skills, and Subagent Delegation**

[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Status](https://img.shields.io/badge/status-active-brightgreen.svg)]()

Ursa is a terminal-based AI coding assistant built in Rust. It runs a persistent agent loop with memory across sessions, reusable skills, and the ability to delegate subtasks to isolated subagents — so complex, multi-step coding work stays organized and resumable.

---

## Key Features

- **Persistent Memory** — Facts written once, recalled across sessions; no re-explaining project context
- **Session Continuity** — JSONL-based store; resume exactly where you left off with `--resume`
- **Skills System** — Reusable prompt templates invoked with `/skill-name`; define your own in `.skills/`
- **Subagent Delegation** — Spawn isolated subagents for parallel exploration without polluting main context
- **Task Tracking** — Visual todo list updated in real time as multi-step work progresses
- **Workspace Context** — Project file listing injected automatically; no manual `find` needed
- **Code Intelligence** — Tree-sitter symbol indexing for precise definition lookup across languages
- **Resilience** — Automatic retry, circuit breaker, and multi-key rotation so LLM failures don't break tasks
- **Delivery Queue** — Write-ahead notification queue with backoff; reliable task-completion alerts

---

## Architecture

```
┌────────────────────────────────────────────────────────────────────┐
│                          Ursa Agent Loop                           │
├────────────────────────────────────────────────────────────────────┤
│                                                                    │
│   User Input                                                       │
│       │                                                            │
│       ▼                                                            │
│   ┌────────────────────────────────────────────────────────────┐  │
│   │                      System Prompt                         │  │
│   │    SOUL.md  │  Workspace Files  │  Memory  │  Todos        │  │
│   └────────────────────────────────────────────────────────────┘  │
│       │                                                            │
│       ▼                                                            │
│   ┌───────────┐    tool_calls?    ┌────────────────────────────┐  │
│   │    LLM    │ ────────────────► │       Tool Registry        │  │
│   │ Provider  │ ◄──── results ─── │  bash  read  write  search │  │
│   └───────────┘                   │  memory_write  memory_search│  │
│       │                           │  todo_write  notify        │  │
│       │  no tool_calls            │  spawn_agent               │  │
│       ▼                           └─────────────┬──────────────┘  │
│   Final Response                                │                  │
│                                                 ▼                  │
│                                          ┌────────────┐           │
│                                          │  Subagent  │           │
│                                          │ (isolated) │           │
│                                          └────────────┘           │
│                                                                    │
│   LaneScheduler  │  SessionManager  │  DeliveryQueue  │ SymbolIndex│
└────────────────────────────────────────────────────────────────────┘
```

### Package Layout

```
cli/          REPL, command routing, skill invocation
core/         Pipeline engine, subagent, session, context, concurrency
llm/          LLM provider abstraction (OpenAI-compatible) + resilience
tools/        Tool implementations: bash, file I/O, memory, todo, notify
services/     MemoryStore, SkillsManager, DeliveryQueue, BootstrapLoader
treesitter/   Symbol indexing via tree-sitter + .scm query files
```

---

## Quick Start

```bash
git clone https://github.com/wonderhelios/ursa.git
cd ursa
cargo build --release
```

### Configuration

```bash
# DeepSeek (recommended)
export URSA_LLM_API_KEY="sk-..."
export URSA_LLM_BASE_URL="https://api.deepseek.com/v1"
export URSA_LLM_MODEL="deepseek-chat"

# Or any OpenAI-compatible API
export URSA_LLM_API_KEY="sk-..."
export URSA_LLM_BASE_URL="https://api.openai.com/v1"
export URSA_LLM_MODEL="gpt-4o"

# Optional: backup keys for automatic rotation on failure
export URSA_LLM_API_KEY_2="sk-..."
export URSA_LLM_API_KEY_3="sk-..."
```

### Run

```bash
# New session
cargo run -p ursa

# Resume the most recent session
cargo run -p ursa -- --resume

# Debug logs to file (keeps terminal output clean)
RUST_LOG=ursa_core=debug cargo run -p ursa 2>ursa.log
```

---

## Usage

```
Ursa Agent - type '/help' for commands, 'quit' to exit

> Refactor the run method in core/src/pipeline/engine.rs, extract tool execution logic

> /commit

> /review src/lib.rs

> Create an HTTP handler and notify me when done
```

### Built-in Commands

| Command | Description |
|---------|-------------|
| `/help` | Show available commands |
| `/skills` | List loaded skills |
| `/history` | List saved sessions |
| `/clear` | Clear current conversation history |
| `quit` | Exit |

---

## Skills

Skills are Markdown files with YAML frontmatter stored in `.skills/`. They let you define reusable agent behaviors for your project.

```markdown
# .skills/test.md
---
name: test
description: Run tests and summarize failures
tags: testing, rust
---

Run `cargo test 2>&1` and analyze any failures.
For each failure: identify the root cause and suggest a minimal fix.
Do not modify files — report only.
```

Invoke with `/test` or `/test <extra context>`.

### Built-in Skills

| Skill | Description |
|-------|-------------|
| `/commit` | Review staged changes and write a conventional commit message |
| `/review [file]` | Code review with severity-ranked findings |

---

## Memory

Ursa remembers facts across sessions. Tell it something once — it recalls it in future sessions without re-scanning files.

```
> We use DeepSeek as LLM, model is deepseek-chat
# Ursa calls memory_write automatically

# New session, next day:
> What LLM are we using?
# Ursa calls memory_search → answers in 1 iteration, no file scanning
```

Memory is stored at `.ursa/memory.json` and searched before every response.

---

## Workspace Bootstrap

Place a `SOUL.md` in your project root to give Ursa workspace-specific instructions:

```markdown
# SOUL.md
This is a Rust workspace using Tokio for async.
Always run `cargo check` after editing .rs files.
Prefer `anyhow` for error handling. Use `tracing` instead of `println!` for logging.
Tests live next to source files in `#[cfg(test)]` modules.
```

Ursa loads `SOUL.md` automatically on startup.

---

## Roadmap

| Feature | Description |
|---------|-------------|
| **Streaming responses** | Token-by-token output instead of waiting for the full LLM reply |
| **Plan stage** | Generate a complete execution plan before acting — reduces aimless tool exploration |
| **Review stage** | Validate results after each task; rollback on failure, reflect for learning |
| **SymbolGraphSource** | Wire tree-sitter symbol index into context — answer code structure questions without `bash find` |
| **Context compression** | Summarize old messages to keep long sessions within token limits |
| **Nag mechanism** | Auto-remind LLM when a todo item has been `in_progress` too long |

---

## Architecture Details

### TPAR Pipeline

Ursa is designed around the TPAR loop. `core/src/pipeline/stages/` defines each stage:

| Stage | Status | Description |
|-------|--------|-------------|
| Task | Planned | Classify intent: Query / Edit / Explain / Terminal |
| Plan | Planned | Generate a step sequence before acting |
| Act | Active | Agent loop with tool execution (current engine) |
| Review | Planned | Validate results, rollback on failure, reflect |

### Memory Architecture

```
Write:  user states a fact → memory_write → .ursa/memory.json
Read:   every run() → memory_search(user_input, top_k=5) → injected into system prompt
Decay:  entries scored by recency + access frequency
```

### Resilience Stack

```
Request → RetryPolicy (3x, exponential backoff)
              │ all keys exhausted?
              ▼
         AuthManager (rotate to next API key)
              │ repeated failures?
              ▼
         CircuitBreaker (open after 5 failures, reset after 60s)
```

---

## Development

```bash
# Check all packages
cargo check

# Run tests
cargo test

# Run specific package
cargo test -p ursa-tools
cargo test -p ursa-core

# Lint
cargo clippy -- -D warnings

# Release build
cargo build --release
```

### Data Files

```
.ursa/
  memory.json          Persistent memory entries
  sessions/            Conversation history (JSONL, one file per session)
  queue/               Pending delivery items
  failed/              Failed delivery items (after max retries)
.skills/               Skill definitions (Markdown with frontmatter)
SOUL.md                Workspace-specific system prompt
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `URSA_LLM_API_KEY` | Primary API key | required |
| `URSA_LLM_BASE_URL` | API base URL | `https://api.openai.com/v1` |
| `URSA_LLM_MODEL` | Model name | `gpt-4o` |
| `URSA_LLM_API_KEY_2` | Backup key (rotation) | optional |
| `URSA_LLM_API_KEY_3` | Backup key (rotation) | optional |

### Supported LLM Providers

Any OpenAI-compatible API works:

| Provider | Base URL |
|----------|----------|
| DeepSeek | `https://api.deepseek.com/v1` |
| OpenAI | `https://api.openai.com/v1` |
| SiliconFlow | `https://api.siliconflow.cn/v1` |
| OpenRouter | `https://openrouter.ai/api/v1` |

---

## License

MIT © [wonder](https://github.com/wonderhelios)

<p align="center">
  Built with Rust
</p>
