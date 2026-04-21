# SearXNG Integration Design

**Date:** 2026-04-16
**Status:** Approved

## Goal

Make SearXNG (self-hosted, Docker-based) the primary web search backend for Cloud Coder, with automatic fallback to existing backends when unavailable.

## Background

Cloud Coder's WebSearchTool already supports multiple backends (DuckDuckGo, Firecrawl, Codex, native Anthropic). A local SearXNG instance provides unlimited, rate-limit-free, private search by aggregating Google/Bing/DDG/Brave/etc. The Docker setup already exists at `docker/searxng/` but is not wired into the tool.

## Architecture

### Priority Chain

When WebSearchTool executes a search:

```
1. SearXNG (local:127.0.0.1:8888) — if running
2. Firecrawl — if FIRECRAWL_API_KEY set
3. DuckDuckGo — for non-Claude models
4. Codex — for OpenAI provider
5. Native Anthropic — for firstParty/Vertex/Foundry
```

SearXNG probes `http://127.0.0.1:8888/healthz` with a 2-second timeout. If it responds, use SearXNG. Otherwise, fall through silently.

### Components

| Component | Location | Purpose |
|-----------|----------|---------|
| SearXNG Manager | `src/services/searxng.ts` | Shared service: health probe, start/stop, Docker check/install, search |
| WebSearchTool changes | `src/tools/WebSearchTool/WebSearchTool.ts` | Add `shouldUseSearXNG()` + `runSearXNGSearch()` at top of priority chain |
| Slash command | `src/commands/host_websearch/index.ts` | `/host_websearch` diagnostic command with full checkpoint verification |

## SearXNG Manager Service

File: `src/services/searxng.ts`

### Functions

- `isSearXNGRunning(): Promise<boolean>` — GET `/healthz` with 2s timeout. Caches result for 30s.
- `isDockerInstalled(): boolean` — checks `docker` command exists
- `isDockerRunning(): boolean` — checks `docker info` succeeds
- `isSearXNGImagePresent(): boolean` — checks `docker images` for `searxng/searxng:latest`
- `isSearXNGEnvConfigured(): boolean` — checks `docker/searxng/.env` exists
- `isContainerRunning(): boolean` — checks `docker ps` for `openclaude-searxng`
- `installDocker(): Promise<void>` — platform-specific install:
  - macOS: `brew install --cask docker` + launch Docker Desktop
  - Linux: detect package manager, run appropriate install command
- `startSearXNG(): Promise<void>` — run `docker compose up -d` in `docker/searxng/`, wait up to 60s for healthz
- `stopSearXNG(): Promise<void>` — run `docker compose down` in `docker/searxng/`
- `restartSearXNG(): Promise<void>` — stop + start
- `autoStartSearXNG(): Promise<boolean>` — attempt to start SearXNG from scratch (Docker check → start container → wait). Returns true if SearXNG is healthy by the end.
- `getSearXNGStatus(): Promise<Status>` — returns object: `{ running, dockerInstalled, dockerRunning, imagePresent, envConfigured, containerRunning, healthOk, searchOk, url, port }`
- `runDiagnostics(): Promise<DiagnosticResult[]>` — walks all 8 checkpoints, returns pass/fail + fix for each
- `searchSearXNG(query, options): Promise<SearchResult>` — GET `/search?q=...&format=json`, parse response into `Output` type
- `getSearXNGLogs(lines?: number): Promise<string>` — `docker compose logs --tail N`

### Health Probe Caching

- Cache positive results for 60s (SearXNG stays up)
- Cache negative results for 10s (may come online soon)
- Cache invalidation: explicit on start/stop/restart

### Search Implementation

SearXNG JSON API: `GET /search?q=<query>&format=json`

Response parsing maps SearXNG results to the existing `Output` type:
```
results[].title  → SearchHit.title
results[].url    → SearchHit.url
results[].content → snippet text (used in formatted summary)
```

Domain filtering (`allowed_domains`/`blocked_domains`) applied client-side from SearXNG results, matching existing DDG implementation pattern.

## WebSearchTool Changes

File: `src/tools/WebSearchTool/WebSearchTool.ts`

### New Functions

```typescript
function shouldUseSearXNG(): boolean
```
- Probes `isSearXNGRunning()` (uses cached value)
- Returns true if SearXNG responds to health check
- Highest priority — even overrides native Anthropic search

```typescript
async function ensureSearXNGRunning(): Promise<boolean>
```
- Called once at session start or on first WebSearchTool use
- If SearXNG is already running: return true
- If not: calls `autoStartSearXNG()` to attempt one automatic start
- If auto-start succeeds: return true, SearXNG is now the backend
- If auto-start fails: log warning about `/host_websearch`, return false
- Subsequent calls return cached result (no repeated auto-start attempts)

```typescript
async function runSearXNGSearch(input: Input): Promise<Output>
```
- Calls `searchSearXNG(input.query, { allowed_domains, blocked_domains })`
- Maps response to `Output` type
- On network error (not rate limit), marks cache as stale and returns error message suggesting `/host_websearch` to diagnose

### Priority Chain Changes

In `call()` method:
```typescript
async call(input, context, ...) {
  if (shouldUseSearXNG()) {
    return { data: await runSearXNGSearch(input) }
  }
  // ... existing fallback chain unchanged
}
```

In `isEnabled()`:
```typescript
isEnabled() {
  if (shouldUseSearXNG()) return true
  // ... existing logic unchanged
}
```

In `prompt()`:
```typescript
async prompt() {
  if (shouldUseSearXNG() || shouldUseDuckDuckGo() || ...) {
    return getWebSearchPrompt().replace(/\n\s*-\s*Web search is only available in the US/, '')
  }
  return getWebSearchPrompt()
}
```

### One-Time Warning

After auto-start fails, if SearXNG is NOT running (checked once per session):
- Log: "SearXNG is not running. Run /host_websearch to diagnose and fix."
- Uses a module-level `hasWarnedSearXNGDown` flag to fire once only

## `/host_websearch` Slash Command

File: `src/commands/host_websearch/index.ts` + `src/commands/host_websearch/host_websearch.tsx`

### Concept

This is a **diagnostic and fix** command. SearXNG starts automatically by default — this command is for when something went wrong. It walks through every checkpoint in order, reports status at each step, and attempts to fix failures.

### Registration

Type: `local-jsx`, name: `host_websearch`
Argument hint: `[start|stop|restart|status|logs]`

### Auto-Start Behavior

When `shouldUseSearXNG()` is called and SearXNG is NOT running:
1. **First attempt**: Automatically try to start SearXNG (check Docker → start container → wait for health)
2. **If auto-start succeeds**: Proceed with SearXNG search normally
3. **If auto-start fails**: Fall back to next backend, log one-time warning pointing to `/host_websearch`

This means SearXNG "just works" by default. The slash command is only needed when auto-start fails.

### Subcommands

| Subcommand | Action |
|------------|--------|
| (none) / `status` | Run full checkpoint diagnostic (see below) |
| `start` | Force start (same checkpoint walk, but stops on first failure with fix instructions) |
| `stop` | Stop container |
| `restart` | Stop + start with checkpoint verification |
| `logs` | Display last 50 lines of container logs |

### Checkpoint Diagnostic

When `/host_websearch` or `/host_websearch status` runs, it walks these checkpoints in order and reports pass/fail for each:

| # | Checkpoint | Check | Fix if failed |
|---|-----------|-------|---------------|
| 1 | Docker CLI installed | `docker --version` exits 0 | Offer to install Docker (brew/apt/dnf) |
| 2 | Docker daemon running | `docker info` exits 0 | Suggest starting Docker Desktop / `systemctl start docker` |
| 3 | SearXNG image present | `docker images searxng/searxng:latest` has entry | Run `docker compose pull` |
| 4 | SearXNG .env configured | `docker/searxng/.env` exists with SEARXNG_SECRET | Copy `.env.example` → `.env`, generate secret |
| 5 | Container running | `docker ps` shows `openclaude-searxng` | Run `docker compose up -d` |
| 6 | Health endpoint responds | GET `http://127.0.0.1:8888/healthz` returns 200 | Wait and retry, or check logs |
| 7 | Search heartbeat test | GET `http://127.0.0.1:8888/search?q=test&format=json` returns results | Check engine config in `settings.yml` |
| 8 | WebSearchTool integration | `shouldUseSearXNG()` returns true | Check routing logic in WebSearchTool.ts |

Each checkpoint shows a green check or red X. On failure, the fix is shown and (where safe to automate) offered for immediate execution.

### UI

The JSX component renders a diagnostic checklist:
- Each checkpoint with pass/fail indicator
- Failed checkpoints show the detected issue and the automated fix command
- Summary line: "X/8 checkpoints passed — SearXNG is [ready/unavailable]"
- Current search backend being used (SearXNG / DDG / Firecrawl / native)

## Docker Auto-Install

The manager's `installDocker()` handles:

### macOS
1. Check `brew` — if missing, prompt user to install Homebrew first
2. `brew install --cask docker`
3. Launch Docker Desktop (`open -a Docker`)
4. Wait up to 120s for `docker info` to succeed
5. Proceed with SearXNG start

### Linux
1. Detect package manager (apt/dnf/pacman)
2. Run appropriate install command
3. Start Docker daemon
4. Proceed with SearXNG start

### Error Handling
- If Docker install fails: show error, suggest manual install URL
- If Docker daemon won't start: show error, suggest platform-specific fix
- If user denies install: proceed with fallback search backends

## Test Plan

All flows tested via sub-agents in separate shell instances:

1. **Docker check**: Verify `isDockerInstalled()` and `isDockerRunning()` return correct values
2. **Health probe**: Verify `isSearXNGRunning()` correctly detects running/stopped SearXNG
3. **SearXNG search**: Verify `searchSearXNG()` returns parsed results from the JSON API
4. **Fallback chain**: Start SearXNG → search uses it. Stop SearXNG → search falls back to DDG
5. **Auto-start**: With SearXNG stopped, trigger WebSearchTool → auto-start fires → SearXNG becomes backend
6. **Auto-start failure**: With Docker stopped, trigger WebSearchTool → auto-start fails → fallback + warning logged
7. **Slash command**: `/host_websearch` runs all 8 checkpoints, reports pass/fail correctly
8. **Slash command fix**: `/host_websearch start` fixes a stopped container
9. **Integration**: Full WebSearchTool call uses SearXNG when available