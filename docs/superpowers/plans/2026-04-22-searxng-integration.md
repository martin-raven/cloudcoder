# SearXNG Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Integrate SearXNG as the primary web search backend for Cloud Coder, with Docker auto-install, automatic startup, fallback chain, and a `/host_websearch` diagnostic slash command.

**Architecture:** Three components: (1) `src/services/searxng.ts` — SearXNG Manager service that handles Docker checks, container lifecycle, health probes, and search queries; (2) modifications to `src/tools/WebSearchTool/WebSearchTool.ts` — insert SearXNG at the top of the search backend priority chain; (3) `src/commands/host_websearch/` — a `local-jsx` slash command with checkpoint diagnostics UI.

**Tech Stack:** TypeScript, Ink (React for CLI), Docker Compose, SearXNG JSON API, Zod schemas

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `src/services/searxng.ts` | Create | SearXNG Manager: health probe, Docker check/install, start/stop, search |
| `src/tools/WebSearchTool/WebSearchTool.ts` | Modify | Add SearXNG priority in search chain, auto-start on first use |
| `src/commands/host_websearch/index.ts` | Create | Slash command registration (local-jsx) |
| `src/commands/host_websearch/host_websearch.tsx` | Create | JSX diagnostic UI component |
| `src/commands.ts` | Modify | Register `host_websearch` command |

---

### Task 1: Create SearXNG Manager Service

**Files:**
- Create: `src/services/searxng.ts`

- [ ] **Step 1: Create the SearXNG Manager service with all functions**

Create `src/services/searxng.ts` with the complete implementation. This file contains:

- Types: `SearXNGStatus`, `DiagnosticResult`, `SearXNGSearchResult`
- Config: `DEFAULT_CONFIG` (host 127.0.0.1, port 8888, timeout 2000ms)
- Config directory: `~/.config/openclaude/searxng/` for docker-compose.yml, .env, settings.yml
- Health probe caching: 60s positive, 10s negative, with invalidation
- Docker checks: `isDockerInstalled()`, `isDockerRunning()`, `isSearXNGImagePresent()`, `isContainerRunning()`
- Container lifecycle: `startSearXNG()`, `stopSearXNG()`, `restartSearXNG()`
- Auto-start: `autoStartSearXNG()` — one attempt per session, falls back gracefully
- Search: `searchSearXNG(query, options)` — GET `/search?q=...&format=json` with domain filtering
- Integration helpers: `shouldUseSearXNG()`, `ensureSearXNGRunning()`, `resetAutoStartFlag()`, `getBaseUrl()`
- Diagnostics: `getSearXNGStatus()`, `runDiagnostics()` — 8-checkpoint walk
- Config generation: `writeConfigFiles()` — writes docker-compose.yml, settings.yml, .env to config dir
- Shell exec: `execCommand()` — Promise wrapper around `execFile` with timeout

Key implementation details:
- `writeConfigFiles()` generates docker-compose.yml, settings.yml, and .env (with random secret from `crypto.randomBytes`) into `~/.config/openclaude/searxng/`
- `startSearXNG()` calls `writeConfigFiles()` first, then pulls image if not present, then `docker compose up -d`, then polls healthz for up to 60s
- `shouldUseSearXNG()` is synchronous — checks cached health state only
- `ensureSearXNGRunning()` is async — calls `autoStartSearXNG()` on first use, logs one-time warning on failure
- `runDiagnostics()` walks 8 checkpoints: Docker CLI, Docker daemon, image, .env, container, health, search, integration
- All Docker commands use `execFile('docker', [...])` not shell commands (no injection risk)

- [ ] **Step 2: Verify the service file compiles**

Run: `cd /Users/admin/Documents/Personal\ work/openclaude && npx tsc --noEmit src/services/searxng.ts 2>&1 | head -30`

Expected: May have import resolution errors (ESM paths). Will fix during full build.

- [ ] **Step 3: Commit**

```bash
git add src/services/searxng.ts
git commit -m "feat(searxng): add SearXNG Manager service with Docker lifecycle and search"
```

---

### Task 2: Modify WebSearchTool to add SearXNG as top priority

**Files:**
- Modify: `src/tools/WebSearchTool/WebSearchTool.ts`

- [ ] **Step 1: Add SearXNG imports to WebSearchTool.ts**

At the top of the file (after existing imports, around line 16), add:

```typescript
import {
  shouldUseSearXNG,
  ensureSearXNGRunning,
  searchSearXNG,
} from '../../services/searxng.js'
```

- [ ] **Step 2: Add `runSearXNGSearch` function**

After the existing `runDuckDuckGoSearch` function (around line 197) and before `runFirecrawlSearch`, add a `runSearXNGSearch` function that:
- Calls `searchSearXNG(input.query, { allowed_domains, blocked_domains })`
- Maps results to `SearchResult` with `tool_use_id: 'searxng-search'`
- Builds a text summary string from snippets (format: `**title** -- snippet (url)`)
- Returns `Output` with `results: [snippets, searchResults]`
- On error, invalidates health cache and throws with `/host_websearch` suggestion

- [ ] **Step 3: Modify `call()` method — add auto-start and SearXNG priority**

At the beginning of `call()` method, add `await ensureSearXNGRunning()` before the priority chain. Then add SearXNG check at the very top of the existing priority chain (before Firecrawl):

```typescript
// SearXNG — highest priority
if (shouldUseSearXNG()) {
  try {
    return { data: await runSearXNGSearch(input) }
  } catch {
    // Fall through to next backend on SearXNG error
  }
}
```

- [ ] **Step 4: Modify `isEnabled()` to include SearXNG**

Add at the top of `isEnabled()`:

```typescript
if (shouldUseSearXNG()) return true
```

- [ ] **Step 5: Modify `prompt()` to remove US-only disclaimer for SearXNG**

Add `shouldUseSearXNG()` to the existing condition that removes the "Web search is only available in the US" line.

- [ ] **Step 6: Commit**

```bash
git add src/tools/WebSearchTool/WebSearchTool.ts
git commit -m "feat(searxng): add SearXNG as top-priority search backend in WebSearchTool"
```

---

### Task 3: Create `/host_websearch` slash command

**Files:**
- Create: `src/commands/host_websearch/index.ts`
- Create: `src/commands/host_websearch/host_websearch.tsx`
- Modify: `src/commands.ts`

- [ ] **Step 1: Create command registration file**

Create `src/commands/host_websearch/index.ts`:

```typescript
import type { Command } from '../../commands.js'

const hostWebsearch: Command = {
  name: 'host_websearch',
  description: 'Manage local SearXNG web search — status, start, stop, restart, logs',
  type: 'local-jsx',
  argumentHint: '[start|stop|restart|status|logs]',
  isEnabled: () => true,
  isHidden: false,
  load: () => import('./host_websearch.js'),
}

export default hostWebsearch
```

- [ ] **Step 2: Create command JSX component**

Create `src/commands/host_websearch/host_websearch.tsx` with:
- Imports from `../../services/searxng.js` and `../../types/command.js`
- `call: LocalJSXCommandCall` — parses subcommand from `args` and renders appropriate view
- `DiagnosticsView` — calls `runDiagnostics()`, shows 8 checkpoints with pass/fail indicators
- `StatusView` — calls `getSearXNGStatus()`, shows status summary
- `StartView` — calls `startSearXNG()`, shows success/error
- `StopView` — calls `stopSearXNG()`, shows success/error
- `RestartView` — calls `restartSearXNG()`, shows success/error
- `LogsView` — calls `getSearXNGLogs(50)`, shows last 50 lines

Each view uses Ink's `<Box>`, `<Text>` components with colors (green for pass, red for fail, yellow for loading, cyan for URLs).

- [ ] **Step 3: Register the command in `src/commands.ts`**

Add import near other command imports: `import hostWebsearch from './commands/host_websearch/index.js'`
Add `hostWebsearch` to the `COMMANDS` array near the `doctor` command.

- [ ] **Step 4: Commit**

```bash
git add src/commands/host_websearch/index.ts src/commands/host_websearch/host_websearch.tsx src/commands.ts
git commit -m "feat(searxng): add /host_websearch diagnostic slash command"
```

---

### Task 4: Build and fix compilation errors

**Files:**
- May modify: any files from previous tasks

- [ ] **Step 1: Run TypeScript compilation check**

Run: `cd /Users/admin/Documents/Personal\ work/openclaude && npx tsc --noEmit 2>&1 | head -60`

- [ ] **Step 2: Fix any compilation errors**

Common issues: import path resolution (`.js` extensions), missing type exports, Ink/React component type issues, unused imports.

- [ ] **Step 3: Run build**

Run: `cd /Users/admin/Documents/Personal\ work/openclaude && bun run build 2>&1 | tail -20`

Expected: Build completes successfully.

- [ ] **Step 4: Commit any fixes**

```bash
git add -A
git commit -m "fix(searxng): resolve compilation errors"
```

---

### Task 5: Test all flows via sub-agents

**Files:**
- No file changes — testing only

- [ ] **Step 1: Test SearXNG service functions with Docker available**

Verify in a sub-agent:
- `isDockerInstalled()` returns true
- `isDockerRunning()` returns correct value
- `isSearXNGImagePresent()` returns correct value
- `isContainerRunning()` returns correct value

- [ ] **Step 2: Test SearXNG start/stop lifecycle**

Verify:
- `startSearXNG()` successfully starts the container
- `isSearXNGRunning()` returns true after start
- `stopSearXNG()` successfully stops the container
- `isSearXNGRunning()` returns false after stop
- `restartSearXNG()` works correctly

- [ ] **Step 3: Test SearXNG search API**

Verify:
- `searchSearXNG('test')` returns results when SearXNG is running
- Domain filtering works (allowed_domains, blocked_domains)
- Error handling for network errors

- [ ] **Step 4: Test auto-start flow**

Verify:
- `autoStartSearXNG()` starts SearXNG when Docker is available
- `shouldUseSearXNG()` reflects cached health state
- `ensureSearXNGRunning()` works correctly with auto-start

- [ ] **Step 5: Test `/host_websearch` slash command**

Verify:
- `/host_websearch status` runs diagnostics
- `/host_websearch start` starts SearXNG
- `/host_websearch stop` stops SearXNG
- `/host_websearch restart` restarts SearXNG
- `/host_websearch logs` shows container logs

- [ ] **Step 6: Test WebSearchTool integration**

Verify:
- WebSearchTool uses SearXNG when running
- WebSearchTool falls back when SearXNG is stopped
- Auto-start triggers on first WebSearchTool use

---

### Task 6: Final integration and cleanup

**Files:**
- May modify: any files from previous tasks

- [ ] **Step 1: Run full build and verify no errors**

Run: `cd /Users/admin/Documents/Personal\ work/openclaude && bun run build 2>&1`

Expected: Clean build with no errors.

- [ ] **Step 2: Run existing tests (if any)**

Run: `cd /Users/admin/Documents/Personal\ work/openclaude && bun test 2>&1 | tail -30`

Expected: All existing tests still pass.

- [ ] **Step 3: Final commit with any remaining fixes**

```bash
git add -A
git commit -m "feat(searxng): complete SearXNG integration with Docker lifecycle, search backend, and slash command"
```