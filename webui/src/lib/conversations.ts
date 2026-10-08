import type {Conversation} from './types';

// Native revisions are decimal u64 strings, including values beyond JS's safe integers.
function revision(value:string|undefined):bigint {
  try{return BigInt(value??'0');}catch{return 0n;}
}
function later(left:string|undefined,right:string|undefined):string|undefined {
  if(right===undefined)return left;
  return revision(right)>=revision(left)?right:left;
}
function activities(current:Conversation|undefined,next:Conversation,authoritative:boolean){
  if(next.activities===undefined)return current?.activities;
  if(current?.activities===undefined)return next.activities;
  const previous=revision(current.activities.at(-1)?.id);
  const incoming=revision(next.activities.at(-1)?.id);
  return incoming>previous||incoming===previous&&authoritative?next.activities:current.activities;
}
function workbench(current:Conversation|undefined,next:Conversation,authoritative:boolean){
  if(next.workbench===undefined)return current?.workbench;
  if(current?.workbench===undefined)return authoritative?next.workbench:undefined;
  if(current.workbench.conversation!==next.workbench.conversation)return authoritative?next.workbench:current.workbench;
  return revision(next.workbench.revision)>=revision(current.workbench.revision)?next.workbench:current.workbench;
}

// Request ownership fences fields without a native monotonic projection revision. Activity,
// input-lifecycle, and prepared-workbench revisions continue to merge by their own authorities.
export function latestConversation(current:Conversation|undefined,next:Conversation,authoritative=true):Conversation {
  const lineageMismatch=current?.run?.id!==undefined&&next.run?.id!==undefined&&current.run.id!==next.run.id;
  const lineageChanged=authoritative&&lineageMismatch;
  const history=lineageChanged?undefined:current;
  const merged:Conversation=authoritative?{...current,...next}:{...current};
  if(authoritative||!lineageMismatch){
    const received=later(history?.received,next.received);
    if(received===undefined)delete merged.received;else merged.received=received;
    const incorporated=later(history?.incorporated,next.incorporated);
    if(incorporated===undefined)delete merged.incorporated;else merged.incorporated=incorporated;
    const activity=activities(history,next,authoritative);
    if(activity===undefined)delete merged.activities;else merged.activities=activity;
  }
  const prepared=workbench(history,next,authoritative);
  if(prepared===undefined)delete merged.workbench;else merged.workbench=prepared;
  if(authoritative&&next.run!==undefined)delete merged.workbench;
  if(authoritative&&next.workbench!==undefined)delete merged.run;
  return merged;
}
