import React, { useState } from 'react';

export default function SnapshotViewer({ snapshot }) {
  const [expanded, setExpanded] = useState(true);

  if (!snapshot) return null;
  const { snapshot: data, hash, step_seq: stepSeq } = snapshot;
  const nodes = data?.nodes || [];

  return (
    <div style={{
      background: '#161b22',
      border: '1px solid #30363d',
      borderRadius: 8,
      overflow: 'hidden',
    }}>
      <div
        onClick={() => setExpanded(!expanded)}
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          padding: '8px 16px',
          background: '#1c2128',
          borderBottom: '1px solid #30363d',
          cursor: 'pointer',
          userSelect: 'none',
        }}
      >
        <strong style={{ color: '#f0f6fc', fontSize: 14 }}>
          Snapshot @ step {stepSeq}
        </strong>
        <span style={{ color: '#8b949e', fontSize: 12 }}>
          {nodes.length} nodes · {expanded ? '▼' : '▶'}
        </span>
      </div>

      {expanded && (
        <div style={{ padding: 12 }}>
          <div style={{ fontSize: 12, color: '#8b949e', marginBottom: 8 }}>
            Hash: <code style={{ color: '#58a6ff' }}>{hash.slice(0, 16)}…</code>
            {data.url && <span> · URL: {data.url}</span>}
          </div>

          {nodes.length === 0 && (
            <p style={{ color: '#6e7681', fontSize: 13, padding: 12, textAlign: 'center' }}>
              Empty snapshot (no interactive elements)
            </p>
          )}

          <div style={{ maxHeight: 400, overflow: 'auto', fontSize: 12 }}>
            {nodes.map((node, i) => (
              <div
                key={node.ref || i}
                style={{
                  padding: '6px 8px',
                  borderBottom: i < nodes.length - 1 ? '1px solid #21262d' : 'none',
                  display: 'flex',
                  gap: 8,
                  alignItems: 'flex-start',
                }}
              >
                <code style={{ color: '#58a6ff', minWidth: 50, flexShrink: 0 }}>{node.ref}</code>
                <span style={{ color: '#f0f6fc', minWidth: 80 }}>{node.role}</span>
                <span style={{ color: '#e1e4e8', flex: 1 }}>
                  {node.name || ''}
                  {node.value ? ` = "${node.value}"` : ''}
                </span>
                {node.parent && <code style={{ color: '#6e7681', fontSize: 11 }}>parent: {node.parent}</code>}
                {node.flags && node.flags.length > 0 && (
                  <span style={{ color: '#d29922', fontSize: 11 }}>
                    [{node.flags.join(', ')}]
                  </span>
                )}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
