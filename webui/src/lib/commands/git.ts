export interface GitCommand {kind:string;paths?:string[];message?:string;options?:Record<string,unknown>;confirmation?:string|undefined}
export function gitCommand(args:string[]):GitCommand {
  const [kind,sub,...rest]=args;
  if(kind==='status'&&(!sub||sub==='--short')&&!rest.length)return{kind:'status'};
  if(['add','unstage','diff','diff-staged'].includes(kind??''))return{kind:kind!,paths:args.slice(1)};
  if(kind==='commit')return{kind,message:args.slice(1).filter(arg=>arg!=='-m').join(' ')};
  if(['fetch','pull','push'].includes(kind??''))return{kind:kind!,options:{remote:sub??'',branch:rest.find(arg=>arg!=='--set-upstream')??'',setUpstream:rest.includes('--set-upstream')}};
  if(kind==='switch'&&sub)return{kind:'branch-switch',options:{branch:sub}};
  if(kind==='branch'&&sub==='create'&&rest[0])return{kind:'branch-create',options:{branch:rest[0],start:rest[1]??''}};
  if(kind==='branch'&&sub==='rename'&&rest[0]&&rest[1])return{kind:'branch-rename',options:{branch:rest[0],name:rest[1]}};
  if(kind==='branch'&&sub==='delete'&&rest[0])return{kind:'branch-delete',options:{branch:rest[0],confirmed:true},confirmation:`Delete local branch ${rest[0]}? Git will refuse unmerged work.`};
  if(kind==='remote'&&rest[0]){
    const action={add:'remote-add','set-url':'remote-url',rename:'remote-rename',remove:'remote-remove'}[sub??''];
    if(action&&(sub==='remove'||rest[1]))return{kind:action,options:{remote:rest[0],url:rest[1],name:rest[1],confirmed:sub==='remove'},confirmation:sub==='remove'?`Remove remote ${rest[0]} from this repository?`:undefined};
  }
  throw new Error('Use /git switch NAME, /git branch create|rename|delete NAME, /git remote add|set-url|rename|remove NAME, or /git fetch|pull|push [REMOTE] [BRANCH].');
}
