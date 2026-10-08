import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  fileBytes,
  fileDrafts,
  fileKey,
  forgetFile,
  retainFile,
} from './drafts.svelte';
import { recordTextChange, stepTextHistory } from './history';

afterEach(() => vi.unstubAllGlobals());

describe('file draft history ownership', () => {
  it('keeps baseline, revision and line-ending metadata outside reversible deltas', () => {
    const project = 'history-metadata-project';
    const path = 'notes.txt';
    const key = fileKey(project, path);
    delete fileDrafts[key];
    const draft = retainFile(project, path, {
      text: '\ufefforiginal\r\n',
      revision: 'revision-one',
      bytes: 13,
    });

    recordTextChange(draft.history, draft.text, 'changed\n');
    draft.text = 'changed\n';

    expect(draft.original).toBe('original\n');
    expect(draft.revision).toBe('revision-one');
    expect(draft.bom).toBe(true);
    expect(draft.newline).toBe('crlf');
    expect(fileBytes(draft)).toBe('\ufeffchanged\r\n');
    draft.text = stepTextHistory(draft.history, draft.text, true)!.text;
    expect(fileBytes(draft)).toBe('\ufefforiginal\r\n');
    delete fileDrafts[key];
  });

  it('retains history when discard is declined and replaces it after explicit discard', () => {
    const project = 'history-discard-project';
    const path = 'draft.txt';
    const key = fileKey(project, path);
    delete fileDrafts[key];
    const draft = retainFile(project, path, {
      text: 'before\n',
      revision: 'revision-one',
      bytes: 7,
    });
    recordTextChange(draft.history, draft.text, 'after\n');
    draft.text = 'after\n';

    vi.stubGlobal('confirm', vi.fn(() => false));
    expect(forgetFile(project, path)).toBe(false);
    expect(fileDrafts[key]).toBe(draft);
    expect(draft.history.undo).toHaveLength(1);

    vi.stubGlobal('confirm', vi.fn(() => true));
    expect(forgetFile(project, path)).toBe(true);
    const fresh = retainFile(project, path, {
      text: 'disk replacement\n',
      revision: 'revision-two',
      bytes: 17,
    });
    expect(fresh.history.undo).toHaveLength(0);
    expect(fresh.history.redo).toHaveLength(0);
    expect(fresh.original).toBe('disk replacement\n');
    expect(fresh.revision).toBe('revision-two');
    delete fileDrafts[key];
  });
});
