import React, { useRef, useEffect } from 'react';

const TYPE_COLORS = {
  run_start: '#3fb950',
  snapshot: '#58a6ff',
  action: '#d29922',
  llm_call: '#bc8cff',
  budget_event: '#f85149',
  run_finish: '#7ee787',
};

const TYPE_LABELS = {
  run_start: 'Start',
  snapshot: 'Snapshot',
  action: 'Action',
  llm_call: 'LLM',
  budget_event: 'Budget',
  run_finish: 'Finish',
};

export default function Timeline({ events, selectedSeq, onSelect, compareSeq, onCompare }) {
  const containerRef = useRef(null);

  // Auto-scroll to selected event.
  useEffect(() => {
    if (containerRef.current && selectedSeq !== null) {
      const el = containerRef.current.querySelector(`[data-seq="${selectedSeq}"]`);
      if (el) el.scrollIntoView({ behavior: 'smooth', block: 'nearest', inline: 'center' });
    }
  }, [selectedSeq]);

  if (!events || events.length === 0) {
    return <div style={{ color: '#8b949e', fontSize: 13, textAlign: 'center', padding: 16 }}>No timeline events</div>;
  }

  return (
    <div>
      <div style={{ marginBottom: 8, fontSize: 13, color: '#8b949e' }}>
        Timeline ({events.length} events)
        {onCompare && <span style={{ marginLeft: 12 }}>· Click an event to select, Ctrl+click to diff</span>}
      </div>
      <div
        ref={containerRef}
        style={{
          display: 'flex',
          gap: 4,
          overflowX: 'auto',
          padding: '8px 0',
          scrollbarWidth: 'thin',
        }}
      >
        {events.map((event) => {
          const isSelected = event.seq === selectedSeq;
          const isCompared = event.seq === compareSeq;
          const color = TYPE_COLORS[event.type] || '#8b949e';
          return (
            <div
              key={event.seq}
              data-seq={event.seq}
              onClick={(e) => {
                if (e.ctrlKey || e.metaKey) {
                  onCompare && onCompare(isCompared ? null : event.seq);
                } else {
                  onSelect(event.seq);
                }
              }}
              title={`${event.type}: ${event.summary || event.goal || ''}`}
              style={{
                display: 'flex',
                flexDirection: 'column',
                alignItems: 'center',
                minWidth: 56,
                padding: '6px 8px',
                borderRadius: 6,
                cursor: 'pointer',
                background: isCompared ? '#2d1b00' : isSelected ? '#1b2a3a' : 'transparent',
                border: `1px solid ${isCompared ? '#d29922' : isSelected ? color : 'transparent'}`,
                transition: 'background 0.15s, border-color 0.15s',
                flexShrink: 0,
              }}
              onMouseEnter={(e) => { if (!isSelected) e.currentTarget.style.background = '#1c2128'; }}
              onMouseLeave={(e) => { if (!isSelected) e.currentTarget.style.background = 'transparent'; }}
            >
              <div style={{
                width: 8,
                height: 8,
                borderRadius: '50%',
                background: color,
                marginBottom: 4,
              }} />
              <span style={{ fontSize: 10, color: '#8b949e', fontWeight: 500 }}>
                {event.seq}
              </span>
              <span style={{ fontSize: 9, color: '#6e7681' }}>
                {TYPE_LABELS[event.type] || event.type}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
