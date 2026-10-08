// Recovery applies retained deltas to spans, avoiding a complete string copy for every edit.
const WINDOW=32_000;
interface Leaf {kind:'leaf';source:string;start:number;end:number;units:number;height:1}
interface Branch {kind:'branch';left:Node;right:Node;units:number;height:number}
type Node=Leaf|Branch;
export type DraftText=Node|undefined;

function height(value:DraftText):number {return value?.height??0;}
export function textUnits(value:DraftText):number {return value?.units??0;}
function leaf(source:string,start=0,end=source.length):DraftText {
  return start===end?undefined:{kind:'leaf',source,start,end,units:end-start,height:1};
}
export function draftText(source:string):DraftText {return leaf(source);}
function branch(left:DraftText,right:DraftText):DraftText {
  if(!left)return right;if(!right)return left;
  return {kind:'branch',left,right,units:left.units+right.units,height:Math.max(left.height,right.height)+1};
}
function balance(left:DraftText,right:DraftText):DraftText {
  if(left?.kind==='branch'&&height(left)>height(right)+1){
    if(height(left.left)>=height(left.right))return branch(left.left,branch(left.right,right));
    if(left.right.kind==='branch'){
      const pivot=left.right;return branch(branch(left.left,pivot.left),branch(pivot.right,right));
    }
  }
  if(right?.kind==='branch'&&height(right)>height(left)+1){
    if(height(right.right)>=height(right.left))return branch(branch(left,right.left),right.right);
    if(right.left.kind==='branch'){
      const pivot=right.left;return branch(branch(left,pivot.left),branch(pivot.right,right.right));
    }
  }
  return branch(left,right);
}
function join(left:DraftText,right:DraftText):DraftText {
  if(!left)return right;if(!right)return left;
  if(left.kind==='branch'&&height(left)>height(right)+1)return balance(left.left,join(left.right,right));
  if(right.kind==='branch'&&height(right)>height(left)+1)return balance(join(left,right.left),right.right);
  return branch(left,right);
}
function split(value:DraftText,at:number):[DraftText,DraftText] {
  if(!value)return [undefined,undefined];
  if(at<=0)return [undefined,value];if(at>=value.units)return [value,undefined];
  if(value.kind==='leaf')return [leaf(value.source,value.start,value.start+at),leaf(value.source,value.start+at,value.end)];
  if(at<value.left.units){const [left,right]=split(value.left,at);return [left,join(right,value.right)];}
  const [left,right]=split(value.right,at-value.left.units);return [join(value.left,left),right];
}
function* leaves(root:DraftText):Generator<Leaf> {
  const pending:Node[]=root?[root]:[];
  while(pending.length){
    const node=pending.pop()!;
    if(node.kind==='leaf')yield node;else pending.push(node.right,node.left);
  }
}
async function sameText(root:DraftText,expected:string):Promise<boolean> {
  if(textUnits(root)!==expected.length)return false;
  let offset=0;
  for(const node of leaves(root)){
    for(let start=node.start;start<node.end;start+=WINDOW){
      const end=Math.min(node.end,start+WINDOW),units=end-start;
      if(node.source.slice(start,end)!==expected.slice(offset,offset+units))return false;
      offset+=units;
      if(offset<expected.length)await yieldWork();
    }
  }
  return offset===expected.length;
}
export async function editDraftText(root:DraftText,at:number,removed:string,inserted:string):Promise<DraftText> {
  if(!Number.isSafeInteger(at)||at<0||at+removed.length>textUnits(root))throw new Error('The retained draft edit is outside its original text.');
  const [before,remainder]=split(root,at),[preimage,after]=split(remainder,removed.length);
  if(!await sameText(preimage,removed))throw new Error('The retained draft edit differs from its exact original text.');
  return join(join(before,draftText(inserted)),after);
}
export async function restoreDraftText(root:DraftText):Promise<string> {
  const parts:string[]=[];let work=0;
  for(const node of leaves(root)){
    parts.push(node.source.slice(node.start,node.end));
    if(++work===256){work=0;await yieldWork();}
  }
  return parts.join('');
}
async function yieldWork():Promise<void> {await new Promise<void>(resolve=>setTimeout(resolve,0));}
