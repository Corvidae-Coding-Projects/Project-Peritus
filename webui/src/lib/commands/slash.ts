/** Slash parsing is local and has no execution or network side effects. */
export type SlashInput =
  | { kind: 'message'; text: string }
  | { kind: 'command'; name: string; args: readonly string[]; source: string }
  | { kind: 'error'; message: string; offset: number; source: string };

/**
 * Only a slash at the start of deliberately submitted input marks a command.
 * Quoted arguments preserve spaces. Backslashes escape quotes, whitespace, or
 * another backslash; other backslashes remain literal for filesystem paths.
 * Shell operators, substitutions, and variables are ordinary argument text.
 */
export function parseSlash(source: string): SlashInput {
  if (!source.startsWith('/')) return { kind: 'message', text: source };

  const words: string[] = [];
  let word = '';
  let started = false;
  let quote: '"' | "'" | null = null;
  let quoteOffset = 0;

  for (let offset = 1; offset < source.length; offset += 1) {
    const character = source[offset]!;
    const next = source[offset + 1];

    if (character === '\\' && quote !== "'") {
      if (next === undefined) {
        return {
          kind: 'error',
          message: 'Finish the escaped character, or quote the argument literally.',
          offset,
          source,
        };
      }
      if (next === '\\' || next === '"' || next === "'" || /\s/u.test(next)) {
        word += next;
        offset += 1;
      } else {
        word += character;
      }
      started = true;
      continue;
    }

    if (quote !== null) {
      if (character === quote) quote = null;
      else word += character;
      continue;
    }

    if (character === '"' || character === "'") {
      quote = character;
      quoteOffset = offset;
      started = true;
    } else if (/\s/u.test(character)) {
      if (started) words.push(word);
      word = '';
      started = false;
    } else {
      word += character;
      started = true;
    }
  }

  if (quote !== null) {
    return {
      kind: 'error',
      message: `Close the ${quote === '"' ? 'double' : 'single'} quote before running this command.`,
      offset: quoteOffset,
      source,
    };
  }
  if (started) words.push(word);

  const name = words.shift();
  if (!name || !/^[a-z][a-z0-9-]*$/u.test(name)) {
    return {
      kind: 'error',
      message: 'Enter a command name after /, or use /help to find a command.',
      offset: 1,
      source,
    };
  }
  return { kind: 'command', name, args: words, source };
}
