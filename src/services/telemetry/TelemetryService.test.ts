import { describe, test, expect, beforeEach } from 'bun:test';
import { TelemetryService, getTelemetryService } from './TelemetryService.js';

describe('TelemetryService', () => {
  let telemetry: TelemetryService;

  beforeEach(() => {
    telemetry = new TelemetryService();
  });

  test('should be disabled by default (opt-in)', () => {
    expect(telemetry.isEnabled()).toBe(false);
  });

  test('should enable when explicitly turned on', () => {
    telemetry.enable();
    expect(telemetry.isEnabled()).toBe(true);
  });

  test('should disable when turned off', () => {
    telemetry.enable();
    telemetry.disable();
    expect(telemetry.isEnabled()).toBe(false);
  });

  test('should queue events when enabled', () => {
    telemetry.enable();
    telemetry.setSessionId('test-session');

    telemetry.record('test_event', { foo: 'bar' });

    const stats = telemetry.getStats();
    expect(stats.eventsQueued).toBe(1);
    expect(stats.queue).toHaveLength(1);
  });

  test('should not queue events when disabled', () => {
    telemetry.record('test_event', { foo: 'bar' });

    const stats = telemetry.getStats();
    expect(stats.eventsQueued).toBe(0);
  });

  test('should sanitize sensitive data', () => {
    telemetry.enable();

    telemetry.record('test', {
      password: 'secret123',
      api_key: 'sk-123',
      token: 'abc',
      safe_field: 'visible',
    });

    const stats = telemetry.getStats();
    const event = stats.queue[0];
    expect(event?.data.password).toBeUndefined();
    expect(event?.data.api_key).toBeUndefined();
    expect(event?.data.token).toBeUndefined();
    expect(event?.data.safe_field).toBe('visible');
  });

  test('should sanitize file paths', () => {
    telemetry.enable();

    telemetry.record('file_access', {
      path: '/home/user/project/src/index.ts',
      action: 'read',
    });

    const stats = telemetry.getStats();
    const event = stats.queue[0];
    expect(event?.data.path).toBe('[path]');
    expect(event?.data.action).toBe('read');
  });

  test('should flush on batch size', async () => {
    telemetry = new TelemetryService({ batchSize: 3 });
    telemetry.enable();
    telemetry.setSessionId('test');

    telemetry.record('e1');
    telemetry.record('e2');
    telemetry.record('e3');

    // Wait for async flush
    await new Promise(resolve => setTimeout(resolve, 10));

    const stats = telemetry.getStats();
    expect(stats.eventsSent).toBe(3);
    expect(stats.queue).toHaveLength(0);
  });

  test('should record command usage', () => {
    telemetry.enable();

    telemetry.recordCommand('/help', 100);

    const stats = telemetry.getStats();
    const event = stats.queue[0];
    expect(event?.type).toBe('command_executed');
    expect(event?.data.command).toBe('/help');
    expect(event?.data.duration_ms).toBe(100);
  });

  test('should record tool usage', () => {
    telemetry.enable();

    telemetry.recordTool('BashTool', true, 50);

    const stats = telemetry.getStats();
    const event = stats.queue[0];
    expect(event?.type).toBe('tool_executed');
    expect(event?.data.tool_name).toBe('BashTool');
    expect(event?.data.success).toBe(true);
  });

  test('should record API requests', () => {
    telemetry.enable();

    telemetry.recordApiRequest('anthropic', 'claude-sonnet-4', { input: 1000, output: 500 });

    const stats = telemetry.getStats();
    const event = stats.queue[0];
    expect(event?.type).toBe('api_request');
    expect(event?.data.provider).toBe('anthropic');
    expect(event?.data.input_tokens).toBe(1000);
  });

  test('should export events to CSV', () => {
    telemetry.enable();
    telemetry.setSessionId('test');

    telemetry.record('event1', { a: 1 });
    telemetry.record('event2', { b: 2 });

    const csv = telemetry.exportToCsv();
    expect(csv).toContain('timestamp,type,session_id,data');
    expect(csv).toContain('event1');
    expect(csv).toContain('event2');
  });

  test('should support singleton', () => {
    const instance1 = getTelemetryService();
    const instance2 = getTelemetryService();
    expect(instance1).toBe(instance2);
  });

  test('should perform health check', async () => {
    const health = await telemetry.healthCheck();
    expect(health.healthy).toBe(true);
    expect(health.checks.has('enabled')).toBe(true);
    expect(health.checks.has('queue_size')).toBe(true);
  });

  test('should dispose and flush', async () => {
    telemetry.enable();
    telemetry.record('before_dispose');

    await telemetry.dispose();

    const stats = telemetry.getStats();
    expect(stats.eventsSent).toBe(1);
    expect(stats.queue).toHaveLength(0);
  });
});
