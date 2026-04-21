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
| Slash command | `src/commands/searxng/index.ts` | `/searxng` with subcommands: start, stop, restart, status, install, logs |

## SearXNG Manager Service

File: `src/services/searxng.ts`

### Functions

- `isSearXNGRunning(): Promise<boolean>` — GET `/healthz` with 2s timeout. Caches result for 30s.
- `isDockerInstalled(): boolean` — checks `docker` command exists
- `isDockerRunning(): boolean` — checks `docker info` succeeds
- `installDocker(): Promise<void>` — platform-specific install:
  - macOS: `brew install --cask docker` + launch Docker Desktop
  - Linux: detect package manager, run appropriate install command
- `startSearXNG(): Promise<void>` — run `docker compose up -d` in `docker/searxng/`, wait up to 60s for healthz
- `stopSearXNG(): Promise<void>` — run `docker compose down` in `docker/searxng/`
- `restartSearXNG(): Promise<void>` — stop + start
- `getSearXNGStatus(): Promise<Status>` — returns object: `{ running, dockerInstalled, dockerRunning, containerState, url, port }`
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
async function runSearXNGSearch(input: Input): Promise<Output>
```
- Calls `searchSearXNG(input.query, { allowed_domains, blocked_domains })`
- Maps response to `Output` type
- On network error (not rate limit), marks cache as stale and returns error message suggesting `/searxng start`

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

When WebSearchTool is enabled but SearXNG is NOT running (checked once per session):
- Log: "SearXNG is not running. Start it with /searxng start for better search results."
- Uses a module-level `hasWarnedSearXNGDown` flag to fire once only

## `/searxng` Slash Command

File: `src/commands/searxng/index.ts` + `src/commands/searxng/searxng.tsx`

### Registration

Type: `local-jsx`, name: `searxng`
Argument hint: `[start|stop|restart|status|install|logs]`

### Subcommands

| Subcommand | Action |
|------------|--------|
| (none) / `status` | Show Docker + container status, URL, uptime |
| `start` | Check Docker → install if missing → start container → wait for health → show status |
| `stop` | Stop container, confirm stopped |
| `restart` | Stop + start |
| `install` | Install Docker if missing, pull SearXNG image, prepare .env |
| `logs` | Display last 50 lines of container logs |

### UI

The JSX component renders a themed status panel:
- Docker status (installed/running/not installed)
- Container status (running/stopped/starting)
- SearXNG URL
- Search backend currently being used
- Action buttons/results for the subcommand

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
5. **Slash command**: `/searxng status` shows correct state, `/searxng start` starts container
6. **Integration**: Full WebSearchTool call uses SearXNG when available