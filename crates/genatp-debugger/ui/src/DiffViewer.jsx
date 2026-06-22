import React from 'react';

const OP_COLORS = {
  add: { bg: '#1b3a24', color: '#3fb950', label: 'ADD' },
  remove: { bg: '#3a1b1b', color: '#f85149', label: 'REMOVE' },
  replace: { bg: '#2d1b00', color: '#d29922', label: 'REPLACE' },
};

export default function DiffViewer({ diff }) {
  if (!diff) return null;

  const { from_seq: fromSeq, to_seq: toSeq, from_url: fromUrl, to_url: toUrl, op_count: opCount, ops } = diff;

  return (
    <div style={{
      background: '#161b22',
      border: '1px solid #30363d',
      borderRadius: 8,
      overflow: 'hidden',
    }}>
      <div style={{
        padding: '8px 16px',
        background: '#1c2128',
        borderBottom: '1px solid #30363d',
      }}>
        <strong style={{ color: '#f0f6fc', fontSize: 14 }}>
          Diff: step {fromSeq} → step {toSeq}
        </strong>
        <span style={{ marginLeft: 8, color: '#8b949e', fontSize: 12 }}>
          ({opCount} operations)
        </span>
        {fromUrl && toUrl && fromUrl !== toUrl && (
          <div style={{ fontSize: 11, color: '#6e7681', marginTop: 2 }}>
            URL changed: {fromUrl} → {toUrl}
          </div>
        )}
      </div>

      {ops.length === 0 && (
        <div style={{ padding: 24, textAlign: 'center', color: '#8b949e', fontSize: 13 }}>
          No changes — snapshots are identical or no content-addressed snapshots available.
        </div>
      )}

      <div style={{ maxHeight: 500, overflow: 'auto', fontSize: 13 }}>
        {ops.map((op, i) => {
          const colors = OP_COLORS[op.op] || { bg: '#1c2128', color: '#8b949e', label: op.op.toUpperCase() };
          return (
            <div
              key={i}
              style={{
                padding: '8px 12px',
                borderBottom: i < ops.length - 1 ? '1px solid #21262d' : 'none',
                background: colors.bg,
                display: 'flex',
                gap: 8,
                alignItems: 'center',
              }}
            >
              <span style={{
                display: 'inline-block',
                padding: '1px 6px',
                borderRadius: 3,
                fontSize: 10,
                fontWeight: 600,
                background: colors.color + '22',
                color: colors.color,
                minWidth: 60,
                textAlign: 'center',
              }}>
                {colors.label}
              </span>
              <code style={{ color: '#58a6ff', minWidth: 50 }}>{op.ref}</code>
              {op.role && <span style={{ color: '#f0f6fc', minWidth: 80 }}>{op.role}</span>}
              {op.name && <span style={{ color: '#e1e4e8' }}>{op.name}</span>}
            </div>
          );
        })}
      </div>
    </div>
  );
}
