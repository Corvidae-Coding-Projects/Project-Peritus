import type { Directory, Entry } from '../types';

export const DIRECTORY_PAGE_SIZE = 250;

export function waitForDirectoryPoll(signal: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal.aborted) {
      reject(signal.reason ?? new DOMException('Directory observation cancelled.', 'AbortError'));
      return;
    }
    let timer: ReturnType<typeof setTimeout>;
    const cancel = () => {
      clearTimeout(timer);
      reject(signal.reason ?? new DOMException('Directory observation cancelled.', 'AbortError'));
    };
    timer = setTimeout(() => {
      signal.removeEventListener('abort', cancel);
      resolve();
    }, 75);
    signal.addEventListener('abort', cancel, { once: true });
  });
}

export interface DirectoryPageRequest {
  directory: string | null;
  filter: string;
  offset: number;
  cursor: string;
  snapshot: string;
}

export interface ReconciledDirectoryPage {
  directory: string;
  entries: Entry[];
  addedIdentities: string[];
  next: string | null;
  snapshot: string;
  total: number;
}

export function reconcileDirectoryProgress(
  request: DirectoryPageRequest,
  page: Directory,
  retainedCursor: string,
): string {
  if (page.ready || !['pending','indexing','ordering'].includes(page.phase)) {
    throw new Error('The directory server returned an invalid indexing state. Refresh the explorer.');
  }
  if (request.offset !== 0 || request.snapshot || page.offset !== 0 || page.snapshot ||
      page.filter !== request.filter || page.entries.length || page.total !== null || page.next !== null ||
      !page.cursor || page.indexed < 0 || !Number.isSafeInteger(page.indexed)) {
    throw new Error('The directory indexing response does not match this request. Refresh the explorer.');
  }
  if (request.directory !== null && page.directory !== request.directory) {
    throw new Error('The requested directory changed identity. Refresh the explorer.');
  }
  if (retainedCursor && retainedCursor !== page.cursor) {
    throw new Error('The directory server replaced an active indexing owner. Refresh the explorer.');
  }
  return page.cursor;
}

export function reconcileDirectoryPage(
  current: Entry[],
  identities: ReadonlySet<string>,
  request: DirectoryPageRequest,
  page: Directory,
): ReconciledDirectoryPage {
  if (!page.ready || page.phase !== 'completed' || page.cursor !== null || page.total === null) {
    throw new Error('The directory server returned an incomplete page as a completed snapshot.');
  }
  if (page.filter !== request.filter || page.offset !== request.offset) {
    throw new Error('The directory server returned a page for a different search. Refresh the explorer.');
  }
  if (request.directory !== null && page.directory !== request.directory) {
    throw new Error('The requested directory changed identity. Refresh the explorer.');
  }
  if (!page.snapshot || (request.snapshot && page.snapshot !== request.snapshot)) {
    throw new Error('The directory changed while pages were loading. Refresh the explorer.');
  }
  if (request.offset > 0 && (!request.snapshot || request.offset !== current.length)) {
    throw new Error('The next directory page no longer follows the loaded entries. Refresh the explorer.');
  }
  if (request.offset > 0 && identities.size !== current.length) {
    throw new Error('The loaded directory identity index is incomplete. Refresh the explorer.');
  }
  if (page.entries.length > DIRECTORY_PAGE_SIZE) {
    throw new Error('The directory server returned an oversized page.');
  }
  const end = page.offset + page.entries.length;
  if (end > page.total || (end < page.total) !== (page.next !== null) || page.next === '') {
    throw new Error('The directory server returned an inconsistent page boundary. Refresh the explorer.');
  }
  if (request.offset > 0 && !request.cursor) {
    throw new Error('The next directory page is missing its durable cursor. Refresh the explorer.');
  }
  const addedIdentities: string[] = [];
  const pageIdentities = new Set<string>();
  for (const entry of page.entries) {
    if (pageIdentities.has(entry.path) || (request.offset > 0 && identities.has(entry.path))) {
      throw new Error('The directory server returned duplicate entry identities. Refresh the explorer.');
    }
    pageIdentities.add(entry.path);
    addedIdentities.push(entry.path);
  }
  const entries = request.offset === 0 ? page.entries : [...current, ...page.entries];
  return { directory: page.directory, entries, addedIdentities, next: page.next, snapshot: page.snapshot, total: page.total };
}
