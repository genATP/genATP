import React, { useState, useEffect, useCallback } from 'react';
import { getRun, getTimeline, getStep, getSnapshot, getDiff } from './api';
import Timeline from './Timeline';
import SnapshotViewer from './SnapshotViewer';
import DiffViewer from './DiffViewer';

export default function RunDetail({ runId, onBack }) {
  const [run, setRun] = useState(null);
  const [timeline, setTimeline] = useState([]);
  const [selectedSeq, setSelectedSeq] = useState(null);
  const [step, setStep] = useState(null);
  const [snapshot, setSnapshot] = useState(null);
  const [diff, setDiff] = useState(null);
  const [compareSeq, setCompareSeq] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);

  // Load run and timeline on mount.
  useEffect(() => {
    Promise.all([getRun(runId), getTimeline(runId)])
      .then(([runData, timelineData]) => {
        setRun(runData);
        setTimeline(timelineData);
        // Select the last step by default.
        const lastAction = [...timelineData].reverse().find(e => e.type === 'action');
        if (lastAction) {
          setSelectedSeq(lastAction.seq);
        } else if (timelineData.length > 0) {
          setSelectedSeq(timelineData[timelineData.length - 1].seq);
        }
      })
      .catch(setError)
      .finally(() => setLoading(false));
  }, [runId]);

  // Load step detail when selection changes.
  useEffect(() => {
    if (selectedSeq === null) return;
    setStep(null);
    setSnapshot(null);
    setDiff(null);
    getStep(runId, selectedSeq).then(setStep).catch(() => {});
    getSnapshot(runId, selectedSeq).then(setSnapshot).catch(() => {});
  }, [runId, selectedSeq]);

  // Load diff when compare selection changes.
  useEffect(() => {
    if (compareSeq === null || compareSeq === selectedSeq) {
      setDiff(null);
      return;
    }
    getDiff(runId, Math.min(compareSeq, selectedSeq), Math.max(compareSeq, selectedSeq))
      .then(setDiff)
      .catch(() => {});
  }, [runId, compareSeq, selectedSeq]);

  if (loading) return <div style={{ padding: 24 }}>Loading run…</div>;
  if (error) return <div style={{ padding: 24, color: '#f85149' }}>Error: {error.message}</div>;
  if (!run) return <div style={{ padding: 24 }}>Run not found.</div>;

  const snapshotNodes = snapshot?.snapshot?.nodes || [];

  return (
    <div style={{ maxWidth: 1200, margin: '0 auto', padding: 24 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 20 }}>
        <button
          onClick={onBack}
          style={{
            background: '#21262d',
            border: '1px solid #30363d',
            borderRadius: 6,
            padding: '4px 12px',
            color: '#f0f6fc',
            cursor: 'pointer',
          }}
        >
          ← Back
        </button>
        <h1 style={{ fontSize: 22, fontWeight: 600, color: '#f0f6fc' }}>{run.id}</h1>
        <span style={{
          fontSize: 12,
          padding: '2px 8px',
          borderRadius: 12,
          fontWeight: 500,
          background: run.outcome === 'success' ? '#1b3a24' : run.outcome === 'aborted' ? '#3a2a1b' : '#3a1b1b',
          color: run.outcome === 'success' ? '#3fb950' : run.outcome === 'aborted' ? '#d29922' : '#f85149',
        }}>
          {run.outcome}
        </span>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: '1fr 300px', gap: 16, marginBottom: 20, fontSize: 13, color: '#8b949e' }}>
        <div>
          <strong>Goal:</strong> {run.goal}
        </div>
        <div style={{ textAlign: 'right' }}>
          {run.step_count} steps · {run.entry_count} events · hash chain: {run.hash_chain_valid ? '✓ valid' : '✗ broken'}
        </div>
      </div>

      {/* Timeline scrubber */}
      <div style={{ marginBottom: 20 }}>
        <Timeline
          events={timeline}
          selectedSeq={selectedSeq}
          onSelect={setSelectedSeq}
          compareSeq={compareSeq}
          onCompare={setCompareSeq}
        />
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: diff ? '1fr 1fr' : '1fr', gap: 16 }}>
        {/* Step detail + snapshot viewer */}
        <div>
          {step && (
            <div style={{
              background: '#161b22',
              border: '1px solid #30363d',
              borderRadius: 8,
              padding: 16,
              marginBottom: 16,
            }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 8 }}>
                <strong style={{ color: '#f0f6fc' }}>Step {step.seq}</strong>
                <span style={{ fontSize: 12, color: '#8b949e' }}>
                  {new Date(step.at).toLocaleString()}
                </span>
              </div>
              <div style={{ marginBottom: 4 }}>
                <span style={{ color: '#58a6ff', fontSize: 13, fontWeight: 500 }}>{step.event_type}</span>
              </div>
              <p style={{ color: '#e1e4e8', fontSize: 14 }}>{step.summary}</p>
              <pre style={{ marginTop: 8, fontSize: 12, maxHeight: 200, overflow: 'auto' }}>
                {JSON.stringify(step.raw_event, null, 2)}
              </pre>
            </div>
          )}

          {snapshot && snapshotNodes.length > 0 && (
            <SnapshotViewer snapshot={snapshot} />
          )}
        </div>

        {/* Diff viewer */}
        {diff && <DiffViewer diff={diff} />}
      </div>
    </div>
  );
}
