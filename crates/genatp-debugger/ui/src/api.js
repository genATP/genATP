const BASE = '/api';

async function fetchJson(url) {
  const res = await fetch(`${BASE}${url}`);
  if (!res.ok) {
    const body = await res.text();
    throw new Error(`${res.status} ${res.statusText}: ${body}`);
  }
  return res.json();
}

export async function listRuns() {
  return fetchJson('/runs');
}

export async function getRun(id) {
  return fetchJson(`/runs/${encodeURIComponent(id)}`);
}

export async function getTimeline(id) {
  return fetchJson(`/runs/${encodeURIComponent(id)}/timeline`);
}

export async function getStep(id, seq) {
  return fetchJson(`/runs/${encodeURIComponent(id)}/steps/${seq}`);
}

export async function getSnapshot(id, seq) {
  return fetchJson(`/runs/${encodeURIComponent(id)}/steps/${seq}/snapshot`);
}

export async function getDiff(id, from, to) {
  return fetchJson(`/runs/${encodeURIComponent(id)}/diff?from=${from}&to=${to}`);
}
