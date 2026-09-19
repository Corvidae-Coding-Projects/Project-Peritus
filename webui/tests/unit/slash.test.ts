import { describe, expect, it } from 'vitest';
import { parseSlash } from '../../src/lib/commands/slash';

function command(source: string, name: string, args: string[]) {
  expect(parseSlash(source)).toEqual({ kind: 'command', name, args, source });
}

describe('deliberately submitted slash input', () => {
  it('preserves ordinary messages, indentation, and embedded commands', () => {
    for (const text of ['Discuss /stop', ' /stop', '```\n/stop\n```', '', 'hello\n/git push']) {
      expect(parseSlash(text)).toEqual({ kind: 'message', text });
    }
  });

  it('keeps the candidate commit distinct from an ordinary Git commit', () => {
    command('/commit', 'commit', []);
    command('/git commit "Fix nested tabs"', 'git', ['commit', 'Fix nested tabs']);
  });

  it('preserves quoted filenames, Unicode, and empty arguments', () => {
    command('/open "src/a b.ts"', 'open', ['src/a b.ts']);
    command("/open '資料/a b.md'", 'open', ['資料/a b.md']);
    command('/memory revise ""', 'memory', ['revise', '']);
  });

  it('supports escaped whitespace and quotes without executing anything', () => {
    command(String.raw`/open a\ b.ts`, 'open', ['a b.ts']);
    command(String.raw`/git commit "Say \"hello\""`, 'git', ['commit', 'Say "hello"']);
    command('/open $(touch) ; | $HOME', 'open', ['$(touch)', ';', '|', '$HOME']);
  });

  it('preserves path separators that are not escape sequences', () => {
    command(String.raw`/open C:\Projects\Peritus\main.rs`, 'open', [String.raw`C:\Projects\Peritus\main.rs`]);
    command(String.raw`/open 'C:\Project Files\'`, 'open', [String.raw`C:\Project Files` + '\\']);
  });

  it('accepts whitespace separators and adjacent quoted segments', () => {
    command('/git\tstatus\n--short  ', 'git', ['status', '--short']);
    command('/open src/"file name".ts', 'open', ['src/file name.ts']);
  });

  it('returns incomplete input intact so the composer can retain its draft', () => {
    for (const source of ['/open "unfinished', "/open 'unfinished", '/open path' + '\\']) {
      expect(parseSlash(source)).toMatchObject({ kind: 'error', source });
    }
    expect(parseSlash('/open "unfinished')).toMatchObject({ offset: 6 });
  });

  it('rejects missing or malformed names locally', () => {
    for (const source of ['/', '/   ', '/123', '//stop', '/git;push']) {
      expect(parseSlash(source)).toMatchObject({ kind: 'error', source });
    }
  });

  it('leaves unknown well-formed names to the command registry', () => {
    command('/future-command argument', 'future-command', ['argument']);
  });
});
