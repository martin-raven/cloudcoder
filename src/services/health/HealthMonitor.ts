/**
 * Health monitoring service for system diagnostics.
 * Monitors CPU, memory, disk, and API health.
 */

import type { Service, HealthStatus } from '../../types/core.js';

interface HealthCheck {
  name: string;
  check(): Promise<{ ok: boolean; message?: string; value?: number }>;
  threshold?: {
    warning?: number;
    critical?: number;
  };
}

interface HealthReport {
  healthy: boolean;
  checks: Map<string, { ok: boolean; message?: string; value?: number }>;
  lastCheck: number;
  summary: {
    total: number;
    passed: number;
    warnings: number;
    failures: number;
  };
}

interface ResourceMetrics {
  memory: {
    used: number;
    total: number;
    percent: number;
  };
  cpu?: {
    percent: number;
  };
  disk?: {
    used: number;
    total: number;
    percent: number;
  };
}

export class HealthMonitorService implements Service {
  private checks: Map<string, HealthCheck> = new Map();
  private lastReport: HealthReport | null = null;
  private checkInterval: NodeJS.Timeout | null = null;
  private readonly intervalMs: number;

  constructor(options: { intervalMs?: number } = {}) {
    this.intervalMs = options.intervalMs ?? 30000; // 30 seconds default
  }

  /**
   * Register a health check.
   */
  register(check: HealthCheck): void {
    this.checks.set(check.name, check);
  }

  /**
   * Unregister a health check.
   */
  unregister(name: string): void {
    this.checks.delete(name);
  }

  /**
   * Run all health checks and return report.
   */
  async check(): Promise<HealthReport> {
    const checks = new Map();
    let passed = 0;
    let warnings = 0;
    let failures = 0;

    for (const [name, healthCheck] of this.checks) {
      try {
        const result = await healthCheck.check();
        checks.set(name, result);

        // Check thresholds if provided
        if (result.value !== undefined && healthCheck.threshold) {
          const { warning, critical } = healthCheck.threshold;
          if (critical !== undefined && result.value >= critical) {
            result.ok = false;
            result.message = `Critical: ${result.value} >= ${critical}`;
          } else if (warning !== undefined && result.value >= warning) {
            warnings++;
            result.message = `Warning: ${result.value} >= ${warning}`;
          }
        }

        if (result.ok) {
          passed++;
        } else {
          failures++;
        }
      } catch (err) {
        checks.set(name, {
          ok: false,
          message: (err as Error).message,
        });
        failures++;
      }
    }

    const report: HealthReport = {
      healthy: failures === 0,
      checks,
      lastCheck: Date.now(),
      summary: {
        total: this.checks.size,
        passed,
        warnings,
        failures,
      },
    };

    this.lastReport = report;
    return report;
  }

  /**
   * Start periodic health monitoring.
   */
  startMonitoring(): void {
    if (this.checkInterval) {
      clearInterval(this.checkInterval);
    }

    this.checkInterval = setInterval(() => {
      void this.check();
    }, this.intervalMs);
  }

  /**
   * Stop periodic health monitoring.
   */
  stopMonitoring(): void {
    if (this.checkInterval) {
      clearInterval(this.checkInterval);
      this.checkInterval = null;
    }
  }

  /**
   * Get current resource metrics.
   */
  getMetrics(): ResourceMetrics {
    const memUsage = process.memoryUsage();
    const totalMemory = process.platform === 'darwin' || process.platform === 'linux'
      ? memUsage.heapTotal // Simplified - in production would use os.totalmem()
      : memUsage.heapTotal;

    return {
      memory: {
        used: Math.round(memUsage.heapUsed / 1024 / 1024), // MB
        total: Math.round(totalMemory / 1024 / 1024), // MB
        percent: Math.round((memUsage.heapUsed / memUsage.heapTotal) * 100),
      },
    };
  }

  /**
   * Get last health report.
   */
  getLastReport(): HealthReport | null {
    return this.lastReport;
  }

  // Service interface
  async initialize(): Promise<void> {
    // Register default checks
    this.register({
      name: 'memory',
      check: async () => {
        const metrics = this.getMetrics();
        const percent = metrics.memory.percent;
        return {
          ok: percent < 90,
          value: percent,
          message: `${percent}% memory used`,
          threshold: { warning: 80, critical: 90 },
        };
      },
    });

    this.register({
      name: 'event_loop',
      check: async () => {
        return new Promise((resolve) => {
          const start = process.hrtime.bigint();
          setImmediate(() => {
            const end = process.hrtime.bigint();
            const delay = Number(end - start) / 1_000_000; // Convert to ms
            resolve({
              ok: delay < 100,
              value: delay,
              message: `${delay.toFixed(2)}ms event loop delay`,
              threshold: { warning: 50, critical: 100 },
            });
          });
        });
      },
    });
  }

  async dispose(): Promise<void> {
    this.stopMonitoring();
    this.checks.clear();
    this.lastReport = null;
  }

  async healthCheck(): Promise<HealthStatus> {
    const report = await this.check();

    const checks = new Map();
    checks.set('health_monitor', {
      ok: report.healthy,
      message: `${report.summary.passed}/${report.summary.total} checks passed`,
    });

    return {
      healthy: report.healthy,
      checks,
      lastCheck: Date.now(),
    };
  }

  get name(): string {
    return 'HealthMonitorService';
  }
}

// Singleton instance
let instance: HealthMonitorService | null = null;

export function getHealthMonitor(): HealthMonitorService {
  if (!instance) {
    instance = new HealthMonitorService();
  }
  return instance;
}

export type { HealthCheck, HealthReport, ResourceMetrics };
