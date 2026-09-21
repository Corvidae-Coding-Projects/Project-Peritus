import type {Conversation} from './types';

// Activity identities are decimal u64 strings, including values beyond JS's safe integers.
export function latestConversation(current:Conversation|undefined,next:Conversation):Conversation {
  if(!current||current.run?.id!==next.run?.id)return next;
  const previous=BigInt(current.activities?.at(-1)?.id??'0');
  const incoming=BigInt(next.activities?.at(-1)?.id??'0');
  if(next.activities&&incoming<previous)return current;
  return {...current,...next};
}
