import React, { useState, useEffect } from 'react';
import { listRuns } from './api';
import RunDetail from './RunDetail';

export default function App() {
  const [runs, setRuns] = useState([]);
  const [selectedId, setSelectedId] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);

  useEffect(() => {
    listRuns()
      .then(setRuns)
      .catch(setError)
      .finally(() => setLoading(false));
  }, []);

  if (selectedId) {
    return <RunDetail runId={selectedId} onBack={() => setSelectedId(null)} />;
  }

  return (
    <div style={{ maxWidth: 960, margin: '0 auto', padding: 24 }}>
      <h1 style={{ fontSize: 24, fontWeight: 600, marginBottom: 24, color: '#f0f6fc' }}>
        ⏱ genATP Debugger
      </h1>

      {loading && <p>Loading runs…</p>}
      {error && (
        <div style={{ color: '#f85149', background: '#2d0a0e', border: '1px solid #f85149', borderRadius: 6, padding: 12, marginBottom: 16 }}>
          <strong>Error:</strong> {error.message}
          <p style={{ marginTop: 8, fontSize: 13, opacity: 0.8 }}>
            Make sure the debugger API server is running: <code>genatp replay --serve</code>
          </p>
        </div>
      )}

      {!loading && !error && runs.length === 0 && (
        <div style={{ textAlign: 'center', padding: 48, color: '#8b949e' }}>
          <p style={{ fontSize: 18, marginBottom: 8 }}>No runs found</p>
          <p style={{ fontSize: 14 }}>
            No event logs were found in the runs directory. Run a task first using the MCP server or <code>genatp record</code>.
          </p>
        </div>
      )}

      {runs.length > 0 && (
        <div style={{ display: 'grid', gap: 8 }}>
          {runs.map((run) => (
            <div
              key={run.id}
              onClick={() => setSelectedId(run.id)}
              style={{
                background: '#161b22',
                border: '1px solid #30363d',
                borderRadius: 8,
                padding: 16,
                cursor: 'pointer',
                transition: 'border-color 0.15s',
              }}
              onMouseEnter={(e) => (e.currentTarget.style.borderColor = '#58a6ff')}
              onMouseLeave={(e) => (e.currentTarget.style.borderColor = '#30363d')}
            >
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <div>
                  <strong style={{ color: '#f0f6fc', fontSize: 16 }}>{run.id}</strong>
                  <span style={{ marginLeft: 12, fontSize: 13, color: '#8b949e' }}>{run.goal}</span>
                </div>
                <div style={{ display: 'flex', gap: 12, fontSize: 13, color: '#8b949e' }}>
                  <span>{run.step_count} steps</span>
                  <span>{run.entry_count} events</span>
                  <span
                    style={{
                      color: run.outcome === 'success' ? '#3fb950' : run.outcome === 'aborted' ? '#d29922' : '#f85149',
                    }}
                  >
                    {run.outcome}
                  </span>
                  {!run.hash_chain_valid && <span title="Hash chain integrity check failed" style={{ color: '#f85149' }}>⚠</span>}
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
