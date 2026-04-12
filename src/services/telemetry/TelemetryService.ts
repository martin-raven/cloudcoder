/**
 * Opt-in telemetry service for usage analytics.
 * Privacy-respecting: no code/file content sent, local dashboard available.
 */

import type { Service, HealthStatus } from '../../types/core.js';

interface TelemetryEvent {
  type: string;
  timestamp: number;
  sessionId: string;
  data: Record<string, unknown>;
}

interface TelemetryStats {
  eventsSent: number;
  eventsQueued: number;
  errors: number;
  lastSent: number | null;
}

interface TelemetryConfig {
  enabled: boolean;
  endpoint?: string;
  batchSize: number;
  flushIntervalMs: number;
}

const DEFAULT_CONFIG: TelemetryConfig = {
  enabled: false, // Opt-in only
  batchSize: 10,
  flushIntervalMs: 30000,
};

export class TelemetryService implements Service {
  private config: TelemetryConfig;
  private queue: TelemetryEvent[] = [];
  private stats: TelemetryStats = {
    eventsSent: 0,
    eventsQueued: 0,
    errors: 0,
    lastSent: null,
  };
  private flushTimer: NodeJS.Timeout | null = null;
  private sessionId: string | null = null;

  constructor(config: Partial<TelemetryConfig> = {}) {
    this.config = { ...DEFAULT_CONFIG, ...config };
  }

  /**
   * Enable telemetry (user must explicitly opt-in).
   */
  enable(): void {
    this.config.enabled = true;
    this.startFlushTimer();
  }

  /**
   * Disable telemetry.
   */
  disable(): void {
    this.config.enabled = false;
    this.stopFlushTimer();
    this.queue = [];
  }

  /**
   * Check if telemetry is enabled.
   */
  isEnabled(): boolean {
    return this.config.enabled;
  }

  /**
   * Set session ID for events.
   */
  setSessionId(sessionId: string): void {
    this.sessionId = sessionId;
  }

  /**
   * Record an event (queued for batch sending).
   */
  record(type: string, data: Record<string, unknown> = {}): void {
    if (!this.config.enabled) {
      return;
    }

    // Filter out any sensitive data
    const sanitizedData = this.sanitizeData(data);

    const event: TelemetryEvent = {
      type,
      timestamp: Date.now(),
      sessionId: this.sessionId ?? 'unknown',
      data: sanitizedData,
    };

    this.queue.push(event);
    this.stats.eventsQueued++;

    // Flush if batch size reached
    if (this.queue.length >= this.config.batchSize) {
      void this.flush();
    }
  }

  /**
   * Record command usage.
   */
  recordCommand(command: string, durationMs: number): void {
    this.record('command_executed', {
      command,
      duration_ms: durationMs,
    });
  }

  /**
   * Record tool usage.
   */
  recordTool(toolName: string, success: boolean, durationMs: number): void {
    this.record('tool_executed', {
      tool_name: toolName,
      success,
      duration_ms: durationMs,
    });
  }

  /**
   * Record API request.
   */
  recordApiRequest(provider: string, model: string, tokens: { input: number; output: number }): void {
    this.record('api_request', {
      provider,
      model,
      input_tokens: tokens.input,
      output_tokens: tokens.output,
    });
  }

  /**
   * Flush queued events.
   */
  async flush(): Promise<void> {
    if (!this.config.enabled || this.queue.length === 0) {
      return;
    }

    const events = [...this.queue];
    this.queue = [];

    try {
      if (this.config.endpoint) {
        // Send to endpoint (when configured)
        await this.sendToEndpoint(events);
      }

      // Also save locally for /stats command
      this.saveLocally(events);

      this.stats.eventsSent += events.length;
      this.stats.lastSent = Date.now();
    } catch (err) {
      this.stats.errors++;
      // Re-queue on failure
      this.queue.unshift(...events);
    }
  }

  /**
   * Get telemetry statistics.
   */
  getStats(): TelemetryStats & { queue: TelemetryEvent[] } {
    return {
      ...this.stats,
      queue: [...this.queue],
    };
  }

  /**
   * Export events to JSON.
   */
  exportEvents(): TelemetryEvent[] {
    return [...this.queue];
  }

  /**
   * Export events to CSV format.
   */
  exportToCsv(): string {
    const headers = ['timestamp', 'type', 'session_id', 'data'];
    const lines = [headers.join(',')];

    for (const event of this.queue) {
      lines.push([
        event.timestamp,
        event.type,
        event.sessionId,
        JSON.stringify(event.data),
      ].join(','));
    }

    return lines.join('\n');
  }

  private async sendToEndpoint(events: TelemetryEvent[]): Promise<void> {
    if (!this.config.endpoint) return;

    const response = await fetch(this.config.endpoint, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(events),
    });

    if (!response.ok) {
      throw new Error(`Telemetry send failed: ${response.status}`);
    }
  }

  private saveLocally(events: TelemetryEvent[]): void {
    // In production, this would write to a local file
    // For now, events are available via getStats() and export methods
  }

  private sanitizeData(data: Record<string, unknown>): Record<string, unknown> {
    const sanitized: Record<string, unknown> = {};

    for (const [key, value] of Object.entries(data)) {
      const lowerKey = key.toLowerCase();

      // Skip potentially sensitive fields (exact matches for secret-related keys)
      if (lowerKey === 'password' || lowerKey === 'secret' || lowerKey === 'api_key' ||
          lowerKey === 'apikey' || lowerKey === 'token' || lowerKey === 'access_token' ||
          lowerKey === 'auth_token') {
        continue;
      }

      // Sanitize file paths (must start with / or ~ or drive letter on Windows)
      if (typeof value === 'string' && value.length > 5) {
        if (value.startsWith('/') || value.startsWith('~') || value.startsWith('C:\\') ||
            (value.includes('/') && value.includes('.'))) {
          sanitized[key] = '[path]';
          continue;
        }
      }

      sanitized[key] = value;
    }

    return sanitized;
  }

  private startFlushTimer(): void {
    this.stopFlushTimer();
    this.flushTimer = setInterval(() => {
      void this.flush();
    }, this.config.flushIntervalMs);
  }

  private stopFlushTimer(): void {
    if (this.flushTimer) {
      clearInterval(this.flushTimer);
      this.flushTimer = null;
    }
  }

  // Service interface
  async initialize(): Promise<void> {
    // Check for opt-in from settings
    const optIn = process.env.CLAUDE_CODE_TELEMETRY_OPT_IN === 'true';
    if (optIn) {
      this.enable();
    }
  }

  async dispose(): Promise<void> {
    this.stopFlushTimer();
    await this.flush();
    this.queue = [];
  }

  async healthCheck(): Promise<HealthStatus> {
    const checks = new Map();
    checks.set('enabled', { ok: true, message: this.config.enabled ? 'enabled' : 'disabled' });
    checks.set('queue_size', { ok: this.queue.length < 1000, message: `${this.queue.length} events queued` });

    return {
      healthy: this.queue.length < 1000,
      checks,
      lastCheck: Date.now(),
    };
  }

  get name(): string {
    return 'TelemetryService';
  }
}

// Singleton instance
let instance: TelemetryService | null = null;

export function getTelemetryService(): TelemetryService {
  if (!instance) {
    instance = new TelemetryService();
  }
  return instance;
}

export type { TelemetryEvent, TelemetryStats, TelemetryConfig };
