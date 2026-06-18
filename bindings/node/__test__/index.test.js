//! Unit tests for genATP Node SDK.
//!
//! These tests exercise the pure-JS wrapper layer.
//! Run with: npx vitest run

const { describe, it, expect } = require('vitest');

// Pure JS wrapper tests — no native addon required.
describe('package structure', () => {
  it('exports should be accessible via destructuring', () => {
    const mod = require('../index.js');
    expect(mod.Agent).toBeDefined();
    expect(mod.BrowserSession).toBeDefined();
    expect(mod.version).toBeDefined();
    expect(mod.listTools).toBeDefined();
    expect(mod.buildInfo).toBeDefined();
    expect(mod.resolveProviderFromEnv).toBeDefined();
  });

  it('exports proper type classes', () => {
    const { RunResult, BudgetSpend, ProviderInfo, ToolDescriptor } = require('../types.js');
    expect(RunResult).toBeDefined();
    expect(BudgetSpend).toBeDefined();
    expect(ProviderInfo).toBeDefined();
    expect(ToolDescriptor).toBeDefined();

    const spend = new BudgetSpend(1000, 0.05, 42);
    expect(spend.tokens).toBe(1000);
    expect(spend.usd).toBe(0.05);
    expect(spend.eventCount).toBe(42);
  });

  it('RunResult wraps native result correctly', () => {
    const { RunResult } = require('../types.js');
    const native = {
      outcome: 'success',
      summary: 'Task completed',
      steps: 5,
      tokens: 2500,
      usd: 0.12,
      eventCount: 15,
    };
    const result = new RunResult(native);
    expect(result.outcome).toBe('success');
    expect(result.summary).toBe('Task completed');
    expect(result.steps).toBe(5);
    expect(result.ok).toBe(true);
    expect(result.spend.tokens).toBe(2500);
    expect(result.spend.usd).toBe(0.12);
  });

  it('RunResult.ok is false for aborted outcome', () => {
    const { RunResult } = require('../types.js');
    const result = new RunResult({
      outcome: 'aborted',
      summary: 'Budget exceeded',
      steps: 3,
      tokens: 1000,
      usd: 0,
      eventCount: 8,
    });
    expect(result.ok).toBe(false);
  });

  it('ToolDescriptor parses input schema JSON', () => {
    const { ToolDescriptor } = require('../types.js');
    const schema = '{"type":"object","properties":{"url":{"type":"string"}},"required":["url"]}';
    const tool = new ToolDescriptor('navigate', 'Navigate to a URL', schema);
    expect(tool.name).toBe('navigate');
    expect(tool.inputSchema.type).toBe('object');
  });

  it('Agent constructor stores options', () => {
    const { Agent } = require('../agent.js');
    const agent = new Agent({
      model: 'gpt-4',
      maxSteps: 50,
      headless: false,
    });
    expect(agent.model).toBe('gpt-4');
    expect(agent.maxSteps).toBe(50);
    expect(agent.headless).toBe(false);
  });

  it('Agent constructor applies defaults', () => {
    const { Agent } = require('../agent.js');
    const agent = new Agent();
    expect(agent.model).toBeNull();
    expect(agent.maxSteps).toBe(30);
    expect(agent.headless).toBe(true);
  });

  it('BrowserSession constructor stores options', () => {
    const { BrowserSession } = require('../browser.js');
    const session = new BrowserSession({ headless: true });
    expect(session._session).toBeDefined();
  });

  it('DEFAULT_START_URL is accessible', () => {
    const { DEFAULT_START_URL } = require('../types.js');
    expect(DEFAULT_START_URL).toBe('https://www.google.com');
  });
});

// Integration tests that require the native addon to be built.
describe('native addon (integration)', () => {
  it('version returns a semver string', () => {
    const mod = require('../index.js');
    const v = mod.version();
    expect(v).toBeDefined();
    expect(typeof v).toBe('string');
    expect(v.split('.').length).toBeGreaterThanOrEqual(3);
  });

  it('buildInfo returns expected keys', () => {
    const mod = require('../index.js');
    const info = mod.buildInfo();
    expect(info).toBeDefined();
    ['genatp_node', 'genatp_core', 'genatp_llm', 'genatp_browser', 'mcp_tools', 'providers'].forEach(
      (key) => { expect(info).toHaveProperty(key); }
    );
    expect(info.providers).toContain('mock');
    expect(info.mcp_tools).toContain('navigate');
    expect(info.mcp_tools).toContain('snapshot');
  });

  it('listTools returns tool descriptors', () => {
    const mod = require('../index.js');
    const tools = mod.listTools();
    expect(Array.isArray(tools)).toBe(true);
    expect(tools.length).toBeGreaterThanOrEqual(9);
    const names = tools.map((t) => t.name);
    expect(names).toContain('navigate');
    expect(names).toContain('click');
    expect(names).toContain('fill');
    expect(names).toContain('snapshot');
  });

  it('resolveProviderFromEnv handles missing credentials', () => {
    const mod = require('../index.js');
    const info = mod.resolveProviderFromEnv();
    expect(info).toBeDefined();
    expect(['ollama', 'mock']).toContain(info.kind);
  });
});
