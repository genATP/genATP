//! Type definitions for genATP Node.js SDK.

class BudgetSpend {
  constructor(tokens, usd, eventCount) {
    this.tokens = tokens;
    this.usd = usd;
    this.eventCount = eventCount;
  }
  toString() {
    return `BudgetSpend(tokens=${this.tokens}, usd=${this.usd.toFixed(4)}, events=${this.eventCount})`;
  }
}

class RunResult {
  constructor(native) {
    this.outcome = native.outcome;
    this.summary = native.summary;
    this.steps = native.steps;
    this.spend = new BudgetSpend(native.tokens, native.usd, native.eventCount);
  }
  get ok() { return this.outcome === 'success'; }
  toString() {
    return `RunResult(outcome=${this.outcome}, steps=${this.steps}, ${this.spend})`;
  }
}

class ProviderInfo {
  constructor(native) {
    this.kind = native.kind;
    this.name = native.name;
    this.model = native.model;
  }
  toString() {
    return `ProviderInfo(kind=${this.kind}, name=${this.name}, model=${this.model})`;
  }
}

class ToolDescriptor {
  constructor(name, description, inputSchemaJson) {
    this.name = name;
    this.description = description;
    this.inputSchema = JSON.parse(inputSchemaJson);
  }
  toString() { return `ToolDescriptor(name=${this.name})`; }
}

class SaveAuthSummary {
  constructor(native) {
    this.cookies = native.cookies;
    this.localStorage = native.localStorage;
    this.sessionStorage = native.sessionStorage;
    this.cookieNames = native.cookieNames;
  }
}

class LoadAuthSummary {
  constructor(native) {
    this.cookies = native.cookies;
    this.localStorage = native.localStorage;
    this.sessionStorage = native.sessionStorage;
  }
}

class AuthDomain {
  constructor(domain, entryCount) {
    this.domain = domain;
    this.entryCount = entryCount;
  }
}

const DEFAULT_START_URL = 'https://www.google.com';

module.exports = {
  BudgetSpend,
  RunResult,
  ProviderInfo,
  ToolDescriptor,
  SaveAuthSummary,
  LoadAuthSummary,
  AuthDomain,
  DEFAULT_START_URL,
};
