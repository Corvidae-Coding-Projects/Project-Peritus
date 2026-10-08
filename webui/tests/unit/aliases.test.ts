import { describe, expect, it } from 'vitest';
import { resolveAlias } from '../../src/lib/commands/aliases';

describe('command alias expansion', () => {
  it('resolves acyclic chains without an arbitrary depth boundary', () => {
    const aliases: Record<string, string> = {};
    for (let index = 0; index < 32; index += 1) {
      aliases[`alias-${index}`] = `/alias-${index + 1} argument-${index}`;
    }
    aliases['alias-32'] = '/open final';

    const resolved = resolveAlias('alias-0', ['requested'], aliases);

    expect(resolved.id).toBe('open');
    expect(resolved.args).toEqual([
      'final',
      ...Array.from({ length: 32 }, (_, index) => `argument-${31 - index}`),
      'requested',
    ]);
  });

  it('rejects only a repeated alias identity', () => {
    expect(() => resolveAlias('first', [], {
      first: '/second',
      second: '/third',
      third: '/first',
    })).toThrow('Command alias cycle. Check [aliases] in your dotfile.');
    expect(() => resolveAlias('self', [], { self: '/self' })).toThrow('Command alias cycle.');
  });

  it('preserves expansion argument order and dispatcher fallback semantics', () => {
    expect(resolveAlias('ship', ['feature branch'], {
      ship: '/publish --review',
      publish: '/git push origin',
    })).toEqual({
      id: 'git',
      args: ['push', 'origin', '--review', 'feature branch'],
    });
    expect(resolveAlias('git', ['status'], { git: '/open "unfinished' }))
      .toEqual({ id: 'git', args: ['status'] });
    expect(resolveAlias('review', ['focus'], {
      review: '/git diff',
      git: 'plain text',
    })).toEqual({ id: 'git', args: ['diff', 'focus'] });
  });
});
