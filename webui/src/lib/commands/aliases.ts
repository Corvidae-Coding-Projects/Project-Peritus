import { parseSlash } from './slash';

export interface AliasResolution {
  id: string;
  args: string[];
}

export function resolveAlias(
  id: string,
  args: readonly string[],
  aliases: Readonly<Record<string, string>>,
): AliasResolution {
  const visited = new Set<string>();
  const argumentsByExpansion: string[][] = [];
  let current = id;

  while (Object.hasOwn(aliases, current)) {
    if (visited.has(current)) {
      throw new Error('Command alias cycle. Check [aliases] in your dotfile.');
    }
    visited.add(current);

    const parsed = parseSlash(aliases[current]!);
    if (parsed.kind !== 'command') break;
    argumentsByExpansion.push([...parsed.args]);
    current = parsed.name;
  }

  const resolvedArgs: string[] = [];
  for (let index = argumentsByExpansion.length - 1; index >= 0; index -= 1) {
    for (const argument of argumentsByExpansion[index]!) resolvedArgs.push(argument);
  }
  for (const argument of args) resolvedArgs.push(argument);
  return { id: current, args: resolvedArgs };
}
