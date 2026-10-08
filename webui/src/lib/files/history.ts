import { applyChange, difference, type Change } from './text';

export interface TextHistory {
  undo: Change[];
  redo: Change[];
}

export interface TextHistoryStep {
  text: string;
  change: Change;
}

export function createTextHistory(): TextHistory {
  return { undo: [], redo: [] };
}

export function recordTextChange(history: TextHistory, before: string, after: string): boolean {
  if (before === after) return false;
  return recordTextDelta(history, difference(before, after));
}

export function recordTextDelta(history: TextHistory, change: Change): boolean {
  if (change.removed === change.inserted) return false;
  history.undo.push(change);
  history.redo.length = 0;
  return true;
}

export function stepTextHistory(
  history: TextHistory,
  text: string,
  back: boolean,
): TextHistoryStep | undefined {
  const source = back ? history.undo : history.redo;
  const change = source.at(-1);
  if (!change) return undefined;
  const next = applyChange(text, change, back);
  source.pop();
  (back ? history.redo : history.undo).push(change);
  return { text: next, change };
}
