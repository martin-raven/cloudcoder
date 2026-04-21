import { execFile } from 'child_process'
import { randomBytes } from 'crypto'
import { existsSync, mkdirSync, writeFileSync } from 'fs'
import { homedir } from 'os'
import * as path from 'path'
import { logError } from '../utils/log.js'

// --- Types ---

export type SearXNGStatus = {
  running: boolean
  dockerInstalled: boolean
  dockerRunning: boolean
  imagePresent: boolean
  envConfigured: boolean
  containerRunning: boolean
  healthOk: boolean
  searchOk: boolean
  url: string
  port: number
}

export type DiagnosticResult = {
  checkpoint: string
  passed: boolean
  message: string
  fix?: string
}

type SearXNGConfig = {
  host: string
  port: number
  timeout: number
}

// --- Config ---

const DEFAULT_CONFIG: SearXNGConfig = {
  host: '127.0.0.1',
  port: 8888,
  timeout: 2000,
}

const SEARXNG_DIR = path.join(homedir(), '.config', 'openclaude', 'searxng')

// --- Cache ---

let healthCache: { result: boolean; timestamp: number } | null = null
const HEALTH_CACHE_POSITIVE_MS = 60_000
const HEALTH_CACHE_NEGATIVE_MS = 10_000
let autoStartAttempted = false

function invalidateHealthCache(): void {
  healthCache = null
}

// --- Shell exec helper ---

function execCommand(
  command: string,
  args: string[],
  timeout = 5000,
): Promise<{ stdout: string; stderr: string; exitCode: number }> {
  return new Promise((resolve) => {
    const child = execFile(command, args, { timeout }, (error, stdout, stderr) => {
      if (error) {
        resolve({
          stdout: stdout ?? '',
          stderr: stderr ?? '',
          exitCode: 1,
        })
      } else {
        resolve({
          stdout: stdout ?? '',
          stderr: stderr ?? '',
          exitCode: 0,
        })
      }
    })
    child.on('error', (error) => {
      resolve({ stdout: '', stderr: error.message, exitCode: 1 })
    })
  })
}

// --- Config directory setup ---

function ensureSearXNGDir(): string {
  if (!existsSync(SEARXNG_DIR)) {
    mkdirSync(SEARXNG_DIR, { recursive: true })
  }
  return SEARXNG_DIR
}

function getDockerComposePath(): string {
  return path.join(ensureSearXNGDir(), 'docker-compose.yml')
}

function getEnvPath(): string {
  return path.join(ensureSearXNGDir(), '.env')
}

function getSettingsDir(): string {
  const dir = path.join(ensureSearXNGDir(), 'searxng')
  if (!existsSync(dir)) {
    mkdirSync(dir, { recursive: true })
  }
  return dir
}

function getSettingsPath(): string {
  return path.join(getSettingsDir(), 'settings.yml')
}

function writeConfigFiles(): void {
  const composePath = getDockerComposePath()
  if (!existsSync(composePath)) {
    writeFileSync(composePath, `version: '3.8'

services:
  searxng:
    image: searxng/searxng:latest
    container_name: openclaude-searxng
    ports:
      - "127.0.0.1:${DEFAULT_CONFIG.port}:8080"
    environment:
      - SEARXNG_BASE_URL=http://${DEFAULT_CONFIG.host}:${DEFAULT_CONFIG.port}/
      - SEARXNG_SECRET=\${SEARXNG_SECRET:-changeme_replace_this_with_random_string}
      - SEARXNG_LIMITER=false
    volumes:
      - ./searxng:/etc/searxng:rw
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/healthz"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 10s
`)
  }

  const settingsPath = getSettingsPath()
  if (!existsSync(settingsPath)) {
    writeFileSync(settingsPath, `use_default_settings: true

general:
  instance_name: "Cloud Coder Search"
  enable_metrics: false

search:
  safe_search: 0
  autocomplete: "google"
  default_lang: "en"
  formats:
    - html
    - json

server:
  limiter: false
  image_proxy: true

ui:
  default_theme: simple
  default_locale: "en"

engines:
  - name: google
    disabled: false
  - name: bing
    disabled: false
  - name: duckduckgo
    disabled: false
  - name: brave
    disabled: false
  - name: qwant
    disabled: false
  - name: github
    disabled: false
  - name: stackoverflow
    disabled: false
  - name: npm
    disabled: false
  - name: pypi
    disabled: false
  - name: wikipedia
    disabled: false
  - name: google news
    disabled: false
  - name: hacker news
    disabled: false
  - name: reddit
    disabled: false
`)
  }

  const envPath = getEnvPath()
  if (!existsSync(envPath)) {
    const secret = randomBytes(32).toString('hex')
    writeFileSync(envPath, `SEARXNG_SECRET=${secret}\n`)
  }
}

// --- Health checks ---

export async function isDockerInstalled(): Promise<boolean> {
  const result = await execCommand('docker', ['--version'], 3000)
  return result.exitCode === 0
}

export async function isDockerRunning(): Promise<boolean> {
  const result = await execCommand('docker', ['info'], 5000)
  return result.exitCode === 0
}

export async function isSearXNGImagePresent(): Promise<boolean> {
  const result = await execCommand('docker', ['images', '-q', 'searxng/searxng:latest'], 5000)
  return result.exitCode === 0 && result.stdout.trim().length > 0
}

export function isSearXNGEnvConfigured(): boolean {
  return existsSync(getEnvPath())
}

export async function isContainerRunning(): Promise<boolean> {
  const result = await execCommand('docker', ['ps', '-q', '-f', 'name=openclaude-searxng'], 5000)
  return result.exitCode === 0 && result.stdout.trim().length > 0
}

export async function isSearXNGRunning(): Promise<boolean> {
  // Check cache
  if (healthCache) {
    const age = Date.now() - healthCache.timestamp
    const maxAge = healthCache.result ? HEALTH_CACHE_POSITIVE_MS : HEALTH_CACHE_NEGATIVE_MS
    if (age < maxAge) {
      return healthCache.result
    }
  }

  try {
    const controller = new AbortController()
    const timeout = setTimeout(() => controller.abort(), DEFAULT_CONFIG.timeout)
    const response = await fetch(`http://${DEFAULT_CONFIG.host}:${DEFAULT_CONFIG.port}/healthz`, {
      signal: controller.signal,
    })
    clearTimeout(timeout)
    const result = response.ok
    healthCache = { result, timestamp: Date.now() }
    return result
  } catch {
    healthCache = { result: false, timestamp: Date.now() }
    return false
  }
}

// --- Lifecycle ---

export async function startSearXNG(): Promise<{ success: boolean; message: string }> {
  writeConfigFiles()
  ensureSearXNGDir()

  // Pull image if not present
  const imagePresent = await isSearXNGImagePresent()
  if (!imagePresent) {
    const pull = await execCommand('docker', ['pull', 'searxng/searxng:latest'], 120_000)
    if (pull.exitCode !== 0) {
      return { success: false, message: `Failed to pull SearXNG image: ${pull.stderr}` }
    }
  }

  // Start container
  const composeFile = getDockerComposePath()
  const result = await execCommand('docker', ['compose', '-f', composeFile, 'up', '-d'], 30_000)
  if (result.exitCode !== 0) {
    return { success: false, message: `Failed to start SearXNG: ${result.stderr}` }
  }

  // Wait for healthz (up to 60 seconds)
  for (let i = 0; i < 30; i++) {
    const healthy = await isSearXNGRunning()
    if (healthy) {
      invalidateHealthCache()
      return { success: true, message: 'SearXNG started successfully' }
    }
    await new Promise((resolve) => setTimeout(resolve, 2000))
  }

  return { success: false, message: 'SearXNG container started but health check timed out after 60s' }
}

export async function stopSearXNG(): Promise<{ success: boolean; message: string }> {
  const composeFile = getDockerComposePath()
  const result = await execCommand('docker', ['compose', '-f', composeFile, 'down'], 30_000)
  invalidateHealthCache()
  if (result.exitCode !== 0) {
    return { success: false, message: `Failed to stop SearXNG: ${result.stderr}` }
  }
  return { success: true, message: 'SearXNG stopped' }
}

export async function restartSearXNG(): Promise<{ success: boolean; message: string }> {
  const stopResult = await stopSearXNG()
  if (!stopResult.success) {
    return stopResult
  }
  return startSearXNG()
}

// --- Auto-start ---

export async function autoStartSearXNG(): Promise<boolean> {
  if (autoStartAttempted) {
    return isSearXNGRunning()
  }
  autoStartAttempted = true

  // Check if already running
  if (await isSearXNGRunning()) {
    return true
  }

  // Check Docker
  if (!(await isDockerInstalled())) {
    logError('SearXNG auto-start: Docker is not installed. Run /host_websearch to diagnose.')
    return false
  }

  if (!(await isDockerRunning())) {
    logError('SearXNG auto-start: Docker daemon is not running. Run /host_websearch to diagnose.')
    return false
  }

  const result = await startSearXNG()
  if (!result.success) {
    logError(`SearXNG auto-start failed: ${result.message}. Run /host_websearch to diagnose.`)
  }
  return result.success
}

export function resetAutoStartFlag(): void {
  autoStartAttempted = false
}

// --- Search ---

export type SearXNGSearchResult = {
  title: string
  url: string
  content?: string
}

export async function searchSearXNG(
  query: string,
  options?: { allowed_domains?: string[]; blocked_domains?: string[] },
): Promise<SearXNGSearchResult[]> {
  const params = new URLSearchParams({
    q: query,
    format: 'json',
  })

  const url = `http://${DEFAULT_CONFIG.host}:${DEFAULT_CONFIG.port}/search?${params.toString()}`

  const controller = new AbortController()
  const timeout = setTimeout(() => controller.abort(), 10_000)

  try {
    const response = await fetch(url, { signal: controller.signal })
    clearTimeout(timeout)

    if (!response.ok) {
      invalidateHealthCache()
      throw new Error(`SearXNG returned ${response.status}: ${response.statusText}`)
    }

    const data = (await response.json()) as {
      results?: Array<{
        title?: string
        url?: string
        content?: string
      }>
    }

    let results = (data.results ?? []).map((r) => ({
      title: r.title ?? r.url ?? '',
      url: r.url ?? '',
      content: r.content ?? '',
    }))

    // Apply domain filtering
    if (options?.allowed_domains?.length) {
      const allowed = new Set(options.allowed_domains)
      results = results.filter((r) => {
        try {
          return allowed.has(new URL(r.url).hostname)
        } catch {
          return false
        }
      })
    }

    if (options?.blocked_domains?.length) {
      const blocked = new Set(options.blocked_domains)
      results = results.filter((r) => {
        try {
          return !blocked.has(new URL(r.url).hostname)
        } catch {
          return true
        }
      })
    }

    return results
  } catch (error) {
    clearTimeout(timeout)
    invalidateHealthCache()
    throw error
  }
}

// --- Status & Diagnostics ---

export async function getSearXNGStatus(): Promise<SearXNGStatus> {
  const [dockerInstalled, dockerRunning, imagePresent, containerRunning, healthOk] =
    await Promise.all([
      isDockerInstalled(),
      isDockerRunning(),
      isSearXNGImagePresent(),
      isContainerRunning(),
      isSearXNGRunning(),
    ])

  let searchOk = false
  if (healthOk) {
    try {
      const results = await searchSearXNG('test')
      searchOk = results.length > 0
    } catch {
      searchOk = false
    }
  }

  return {
    running: healthOk,
    dockerInstalled,
    dockerRunning,
    imagePresent,
    envConfigured: isSearXNGEnvConfigured(),
    containerRunning,
    healthOk,
    searchOk,
    url: `http://${DEFAULT_CONFIG.host}:${DEFAULT_CONFIG.port}`,
    port: DEFAULT_CONFIG.port,
  }
}

export async function runDiagnostics(): Promise<DiagnosticResult[]> {
  const results: DiagnosticResult[] = []

  // Check 1: Docker CLI installed
  const dockerInstalled = await isDockerInstalled()
  results.push({
    checkpoint: 'Docker CLI installed',
    passed: dockerInstalled,
    message: dockerInstalled ? 'Docker CLI is available' : 'Docker CLI not found',
    fix: dockerInstalled
      ? undefined
      : 'Install Docker: macOS → brew install --cask docker; Linux → apt/dnf/pacman install docker',
  })

  // Check 2: Docker daemon running
  let dockerRunning = false
  if (dockerInstalled) {
    dockerRunning = await isDockerRunning()
    results.push({
      checkpoint: 'Docker daemon running',
      passed: dockerRunning,
      message: dockerRunning ? 'Docker daemon is running' : 'Docker daemon is not responding',
      fix: dockerRunning
        ? undefined
        : 'Start Docker Desktop (macOS) or run: sudo systemctl start docker (Linux)',
    })
  } else {
    results.push({
      checkpoint: 'Docker daemon running',
      passed: false,
      message: 'Skipped — Docker CLI not installed',
      fix: 'Install Docker first',
    })
  }

  // Check 3: SearXNG image present
  let imagePresent = false
  if (dockerRunning) {
    imagePresent = await isSearXNGImagePresent()
    results.push({
      checkpoint: 'SearXNG image present',
      passed: imagePresent,
      message: imagePresent
        ? 'searxng/searxng:latest image exists'
        : 'Image not found locally',
      fix: imagePresent ? undefined : 'Run: docker pull searxng/searxng:latest',
    })
  } else {
    results.push({
      checkpoint: 'SearXNG image present',
      passed: false,
      message: 'Skipped — Docker not running',
      fix: 'Start Docker first',
    })
  }

  // Check 4: .env configured
  const envConfigured = isSearXNGEnvConfigured()
  results.push({
    checkpoint: 'SearXNG .env configured',
    passed: envConfigured,
    message: envConfigured
      ? '.env file exists with SEARXNG_SECRET'
      : '.env file missing',
    fix: envConfigured ? undefined : 'Run /host_websearch start to auto-configure',
  })

  // Check 5: Container running
  let containerRunning = false
  if (dockerRunning) {
    containerRunning = await isContainerRunning()
    results.push({
      checkpoint: 'Container running',
      passed: containerRunning,
      message: containerRunning
        ? 'openclaude-searxng container is running'
        : 'Container not running',
      fix: containerRunning ? undefined : 'Run: /host_websearch start',
    })
  } else {
    results.push({
      checkpoint: 'Container running',
      passed: false,
      message: 'Skipped — Docker not running',
      fix: 'Start Docker first',
    })
  }

  // Check 6: Health endpoint
  const healthOk = containerRunning && (await isSearXNGRunning())
  results.push({
    checkpoint: 'Health endpoint responds',
    passed: healthOk,
    message: healthOk
      ? `http://${DEFAULT_CONFIG.host}:${DEFAULT_CONFIG.port}/healthz returns 200`
      : 'Health check failed',
    fix: healthOk ? undefined : 'Check container logs: /host_websearch logs',
  })

  // Check 7: Search heartbeat
  let searchOk = false
  if (healthOk) {
    try {
      const searchResults = await searchSearXNG('test')
      searchOk = searchResults.length > 0
    } catch {
      searchOk = false
    }
  }
  results.push({
    checkpoint: 'Search heartbeat test',
    passed: searchOk,
    message: searchOk ? 'SearXNG returns search results' : 'Search query returned no results',
    fix: searchOk ? undefined : 'Check engine config in settings.yml',
  })

  // Check 8: WebSearchTool integration
  const integrated = healthOk && searchOk && shouldUseSearXNG()
  results.push({
    checkpoint: 'WebSearchTool integration',
    passed: integrated,
    message: integrated
      ? 'SearXNG is the active search backend'
      : 'SearXNG is not being used by WebSearchTool',
    fix: integrated ? undefined : 'SearXNG must be healthy for WebSearchTool to use it',
  })

  return results
}

export async function getSearXNGLogs(lines = 50): Promise<string> {
  const composeFile = getDockerComposePath()
  const result = await execCommand(
    'docker',
    ['compose', '-f', composeFile, 'logs', '--tail', String(lines)],
    5000,
  )
  return result.exitCode === 0 ? result.stdout : `Error fetching logs: ${result.stderr}`
}

// --- Integration helpers (used by WebSearchTool) ---

let hasWarnedSearXNGDown = false

export function shouldUseSearXNG(): boolean {
  // Synchronous check that relies on cached health state.
  // The actual async health probe happens in ensureSearXNGRunning().
  if (healthCache && healthCache.result) {
    const age = Date.now() - healthCache.timestamp
    if (age < HEALTH_CACHE_POSITIVE_MS) {
      return true
    }
  }
  return false
}

export async function ensureSearXNGRunning(): Promise<boolean> {
  // Check if already running (uses cache)
  if (await isSearXNGRunning()) {
    return true
  }

  // Try auto-start once
  const started = await autoStartSearXNG()
  if (!started && !hasWarnedSearXNGDown) {
    hasWarnedSearXNGDown = true
    logError('SearXNG is not running and could not auto-start. Run /host_websearch to diagnose.')
  }
  return started
}

export function getBaseUrl(): string {
  return `http://${DEFAULT_CONFIG.host}:${DEFAULT_CONFIG.port}`
}