import { describe, test, expect, beforeEach } from 'bun:test';
import { HealthMonitorService, getHealthMonitor } from './HealthMonitor.js';

describe('HealthMonitorService', () => {
  let monitor: HealthMonitorService;

  beforeEach(() => {
    monitor = new HealthMonitorService();
  });

  test('should register and run health checks', async () => {
    monitor.register({
      name: 'test-check',
      check: async () => ({ ok: true, message: 'All good' }),
    });

    const report = await monitor.check();
    expect(report.healthy).toBe(true);
    expect(report.checks.get('test-check')).toEqual({
      ok: true,
      message: 'All good',
    });
    expect(report.summary.passed).toBe(1);
  });

  test('should handle failing checks', async () => {
    monitor.register({
      name: 'failing-check',
      check: async () => ({ ok: false, message: 'Something wrong' }),
    });

    const report = await monitor.check();
    expect(report.healthy).toBe(false);
    expect(report.summary.failures).toBe(1);
  });

  test('should handle check errors', async () => {
    monitor.register({
      name: 'error-check',
      check: async () => {
        throw new Error('Check failed');
      },
    });

    const report = await monitor.check();
    expect(report.healthy).toBe(false);
    expect(report.summary.failures).toBe(1);
    expect(report.checks.get('error-check')?.ok).toBe(false);
  });

  test('should apply thresholds', async () => {
    monitor.register({
      name: 'threshold-check',
      check: async () => ({ ok: true, value: 85 }),
      threshold: { warning: 80, critical: 90 },
    });

    const report = await monitor.check();
    const result = report.checks.get('threshold-check');
    expect(result?.ok).toBe(true);
    expect(result?.message).toContain('Warning');
    expect(report.summary.warnings).toBe(1);
  });

  test('should apply critical thresholds', async () => {
    monitor.register({
      name: 'critical-check',
      check: async () => ({ ok: true, value: 95 }),
      threshold: { warning: 80, critical: 90 },
    });

    const report = await monitor.check();
    const result = report.checks.get('critical-check');
    expect(result?.ok).toBe(false);
    expect(result?.message).toContain('Critical');
    expect(report.summary.failures).toBe(1);
  });

  test('should unregister checks', async () => {
    monitor.register({
      name: 'temp-check',
      check: async () => ({ ok: true }),
    });

    monitor.unregister('temp-check');

    const report = await monitor.check();
    expect(report.summary.total).toBe(0);
  });

  test('should get resource metrics', () => {
    const metrics = monitor.getMetrics();
    // Memory values may be 0 in test environment, just check structure
    expect(metrics.memory).toBeDefined();
    expect(typeof metrics.memory.used).toBe('number');
    expect(typeof metrics.memory.percent).toBe('number');
    expect(metrics.memory.percent).toBeGreaterThanOrEqual(0);
    expect(metrics.memory.percent).toBeLessThanOrEqual(100);
  });

  test('should store last report', async () => {
    monitor.register({
      name: 'test',
      check: async () => ({ ok: true }),
    });

    await monitor.check();

    const lastReport = monitor.getLastReport();
    expect(lastReport).not.toBeNull();
    expect(lastReport?.summary.total).toBe(1);
  });

  test('should perform service health check', async () => {
    await monitor.initialize();

    const health = await monitor.healthCheck();
    expect(health.healthy).toBe(true);
    expect(health.checks.has('health_monitor')).toBe(true);
  });

  test('should support singleton', () => {
    const instance1 = getHealthMonitor();
    const instance2 = getHealthMonitor();
    expect(instance1).toBe(instance2);
  });

  test('should dispose correctly', async () => {
    await monitor.initialize();
    await monitor.dispose();

    const report = await monitor.check();
    expect(report.summary.total).toBe(0);
    // Last report is kept for reference, but checks should be empty
    expect(report.checks.size).toBe(0);
  });
});
