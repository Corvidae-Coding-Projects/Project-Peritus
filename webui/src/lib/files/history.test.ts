import { describe, expect, it } from 'vitest';
import { createTextHistory, recordTextChange, recordTextDelta, stepTextHistory } from './history';

describe('draft-owned text history', () => {
  it('immediately reverses a single accepted paste larger than the former budget', () => {
    const history = createTextHistory();
    const before = 'exact baseline\n';
    const after = `${before}${'資料'.repeat(1_000_001)}`;

    expect(recordTextChange(history, before, after)).toBe(true);
    expect(history.undo).toHaveLength(1);
    expect(stepTextHistory(history, after, true)?.text).toBe(before);
  });

  it('retains every accepted delta through more than two hundred edits', () => {
    const history = createTextHistory();
    let text = '';
    for (let index = 0; index < 256; index += 1) {
      const next = `${text}${index},`;
      recordTextChange(history, text, next);
      text = next;
    }

    expect(history.undo).toHaveLength(256);
    for (let index = 0; index < 256; index += 1) {
      text = stepTextHistory(history, text, true)!.text;
    }
    expect(text).toBe('');
    expect(history.redo).toHaveLength(256);
  });

  it('keeps history on the reusable owner and clears redo after a new edit', () => {
    const history = createTextHistory();
    recordTextChange(history, 'one', 'one two');
    const undone = stepTextHistory(history, 'one two', true)!;

    const retainedAcrossRemount = history;
    recordTextChange(retainedAcrossRemount, undone.text, 'one three');

    expect(retainedAcrossRemount.undo).toHaveLength(1);
    expect(retainedAcrossRemount.redo).toHaveLength(0);
    expect(stepTextHistory(retainedAcrossRemount, 'one three', true)?.text).toBe('one');
  });

  it('records an input-owned delta without rescanning the unchanged document', () => {
    const history = createTextHistory();
    const text = `${'unchanged'.repeat(100_000)}!`;
    recordTextDelta(history, { at: text.length - 1, removed: '!', inserted: '?' });
    expect(stepTextHistory(history, `${text.slice(0,-1)}?`, true)?.text).toBe(text);
  });
});
