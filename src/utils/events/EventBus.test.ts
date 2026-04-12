import { describe, test, expect } from 'bun:test';
import { EventBus } from './EventBus.js';
import type { CloudCoderEvent, EventType } from '../../types/core.js';

describe('EventBus', () => {
  test('should subscribe and receive events', () => {
    const bus = new EventBus();
    const received: string[] = [];

    bus.subscribe('tool_call_start', (event) => {
      received.push(event.type);
    });

    bus.emit({
      type: 'tool_call_start',
      payload: { toolName: 'BashTool' },
      timestamp: Date.now(),
      source: 'test'
    });

    expect(received).toEqual(['tool_call_start']);
  });

  test('should unsubscribe correctly', () => {
    const bus = new EventBus();
    let callCount = 0;

    const subscription = bus.subscribe('tool_call_complete', () => {
      callCount++;
    });

    bus.emit({
      type: 'tool_call_complete',
      payload: {},
      timestamp: Date.now(),
      source: 'test'
    });

    subscription.unsubscribe();

    bus.emit({
      type: 'tool_call_complete',
      payload: {},
      timestamp: Date.now(),
      source: 'test'
    });

    expect(callCount).toBe(1);
  });

  test('should buffer events for late subscribers', () => {
    const bus = new EventBus({ maxBufferedEvents: 10 });

    bus.emit({
      type: 'session_start',
      payload: { sessionId: 'test-123' },
      timestamp: Date.now(),
      source: 'test'
    });

    const received: unknown[] = [];
    bus.subscribe('session_start', (event) => {
      received.push(event.payload);
    });

    expect(received).toHaveLength(1);
    expect(received[0]).toEqual({ sessionId: 'test-123' });
  });

  test('should track statistics', () => {
    const bus = new EventBus();

    bus.emit({
      type: 'tool_call_start',
      payload: {},
      timestamp: Date.now(),
      source: 'test'
    });

    bus.emit({
      type: 'tool_call_start',
      payload: {},
      timestamp: Date.now(),
      source: 'test'
    });

    bus.emit({
      type: 'api_request_complete',
      payload: {},
      timestamp: Date.now(),
      source: 'test'
    });

    const stats = bus.getStats();
    expect(stats.totalEventsEmitted).toBe(3);
    expect(stats.eventsByType.get('tool_call_start')).toBe(2);
    expect(stats.eventsByType.get('api_request_complete')).toBe(1);
  });

  test('should subscribe to all events', () => {
    const bus = new EventBus();
    const allEvents: EventType[] = [];

    bus.subscribeAll((event) => {
      allEvents.push(event.type);
    });

    bus.emit({
      type: 'tool_call_start',
      payload: {},
      timestamp: Date.now(),
      source: 'test'
    });

    bus.emit({
      type: 'settings_change',
      payload: {},
      timestamp: Date.now(),
      source: 'test'
    });

    expect(allEvents).toEqual(['tool_call_start', 'settings_change']);
  });

  test('should clear all subscriptions', () => {
    const bus = new EventBus();
    let callCount = 0;

    bus.subscribe('tool_call_start', () => {
      callCount++;
    });

    bus.clear();

    bus.emit({
      type: 'tool_call_start',
      payload: {},
      timestamp: Date.now(),
      source: 'test'
    });

    expect(callCount).toBe(0);
    expect(bus.getStats().totalListeners).toBe(0);
  });

  test('should handle async event handlers', async () => {
    const bus = new EventBus();
    const results: number[] = [];

    bus.subscribe('tool_call_complete', async (event) => {
      await new Promise(resolve => setTimeout(resolve, 10));
      results.push(1);
    });

    bus.emit({
      type: 'tool_call_complete',
      payload: {},
      timestamp: Date.now(),
      source: 'test'
    });

    // Wait for async handler to complete
    await new Promise(resolve => setTimeout(resolve, 50));

    expect(results).toEqual([1]);
  });

  test('should handle errors in event handlers without crashing', () => {
    const bus = new EventBus();
    let secondHandlerCalled = false;

    bus.subscribe('tool_call_start', () => {
      throw new Error('Test error');
    });

    bus.subscribe('tool_call_start', () => {
      secondHandlerCalled = true;
    });

    bus.emit({
      type: 'tool_call_start',
      payload: {},
      timestamp: Date.now(),
      source: 'test'
    });

    // Second handler should still be called despite first throwing
    expect(secondHandlerCalled).toBe(true);
  });

  test('should respect max buffered events limit', () => {
    const bus = new EventBus({ maxBufferedEvents: 3 });

    // Emit 5 events
    for (let i = 0; i < 5; i++) {
      bus.emit({
        type: 'session_start',
        payload: { index: i },
        timestamp: Date.now(),
        source: 'test'
      });
    }

    const received: number[] = [];
    bus.subscribe('session_start', (event) => {
      received.push((event.payload as { index: number }).index);
    });

    // Should only receive last 3 events (buffer limit)
    expect(received).toEqual([2, 3, 4]);
  });
});
