export interface TextSnapshot { text:string; revision:string; bytes:number }
export interface TextFormat { bom:boolean; newline:'lf'|'crlf'|'mixed'; lines:number }
export const TEXT_PAGE_UNITS=128_000;
const TEXT_WORK_UNITS=32_000;

export interface TextPage { start:number; line:number; lineStart:number; counts?:{lf:number;crlf:number;cr:number} }
interface PageNode { entries:readonly (PageNode|TextPage|undefined)[] }
const PAGE_BRANCHES=64;

/** Immutable index frontiers share preceding pages instead of copying growing arrays. */
export class TextPages {
  private constructor(private readonly root:PageNode,private readonly height:number,readonly length:number) {}
  static empty():TextPages {return new TextPages({entries:[]},0,0);}
  at(position:number):TextPage|undefined {
    const index=position<0?this.length+position:position;
    if(!Number.isSafeInteger(index)||index<0||index>=this.length)return undefined;
    let node=this.root;
    for(let height=this.height;height>0;height--){
      node=node.entries[Math.floor(index/PAGE_BRANCHES**height)%PAGE_BRANCHES] as PageNode;
    }
    return node.entries[index%PAGE_BRANCHES] as TextPage;
  }
  append(page:TextPage):TextPages {
    let root=this.root,height=this.height;
    if(this.length===PAGE_BRANCHES**(height+1)){root={entries:[root]};height++;}
    return new TextPages(writePage(root,height,this.length,page),height,this.length+1);
  }
  prefix(length:number):TextPages {
    return new TextPages(this.root,this.height,Math.max(0,Math.min(this.length,Math.trunc(length))));
  }
}
function writePage(node:PageNode,height:number,index:number,page:TextPage):PageNode {
  const slot=Math.floor(index/PAGE_BRANCHES**height)%PAGE_BRANCHES,entries=[...node.entries];
  entries[slot]=height===0?page:writePage((entries[slot] as PageNode|undefined)??{entries:[]},height-1,index,page);
  return {entries};
}
export interface TextIndex {
  source:string;
  pages:TextPages;
  scanned:number;
  format:TextFormat;
  complete:boolean;
}

export function textFormat(text:string):TextFormat {
  let lf=0,crlf=0,cr=0;
  for(let i=0;i<text.length;i++) {
    if(text[i]==='\n') { if(text[i-1]==='\r')crlf++;else lf++; }
    if(text[i]==='\r'&&text[i+1]!=='\n')cr++;
  }
  return {bom:text.startsWith('\ufeff'),newline:cr||lf&&crlf?'mixed':crlf?'crlf':'lf',lines:lf+crlf+cr+1};
}

export function safeTextOffset(text:string,offset:number) {
  const at=Math.max(0,Math.min(text.length,offset));
  if(at>0&&at<text.length&&isHighSurrogate(text.charCodeAt(at-1))&&isLowSurrogate(text.charCodeAt(at)))return at+1;
  return at;
}

export function pageStarts(text:string,size=TEXT_PAGE_UNITS) {
  size=Math.max(1,Math.trunc(size)||TEXT_PAGE_UNITS);
  const starts=[0];
  for(let at=size;at<text.length;) {
    const newline=text.lastIndexOf('\n',at);
    const candidate=newline>(starts.at(-1)??0)?newline+1:at;
    const end=safeTextOffset(text,candidate);
    starts.push(end);at=end+size;
  }
  return starts;
}

export function emptyTextIndex(text=''):TextIndex {
  return {source:text,pages:TextPages.empty().append({start:0,line:1,lineStart:0,counts:{lf:0,crlf:0,cr:0}}),scanned:0,format:{bom:text.startsWith('\ufeff'),newline:'lf',lines:1},complete:text.length===0};
}

/** A computed edit proves that every preceding page still describes the same text. */
export function prefixTextIndex(index:TextIndex,before:string,after:string,change:Change):TextIndex {
  if(index.source!==before)return emptyTextIndex(after);
  const at=Math.max(0,change.at-1),position=pagePosition(index.pages,at),point=index.pages.at(position)!;
  if(!point.counts)return emptyTextIndex(after);
  const counts=point.counts;
  return {
    source:after,pages:index.pages.prefix(position+1),scanned:point.start,
    format:{bom:after.startsWith('\ufeff'),newline:counts.cr||counts.lf&&counts.crlf?'mixed':counts.crlf?'crlf':'lf',lines:point.line},
    complete:point.start===after.length,
  };
}

export async function indexText(
  text:string,
  publish:(index:TextIndex)=>void=()=>{},
  signal?:AbortSignal,
  pageSize=TEXT_PAGE_UNITS,
  prefix?:TextIndex,
):Promise<TextIndex> {
  pageSize=Math.max(1,Math.trunc(pageSize)||TEXT_PAGE_UNITS);
  const seed=prefix?.source===text?prefix.pages.at(-1):undefined;
  const resumed=seed?.counts!==undefined&&seed.start<=text.length;
  let pages=resumed?prefix!.pages:emptyTextIndex(text).pages;
  let lf=resumed?seed!.counts!.lf:0,crlf=resumed?seed!.counts!.crlf:0,cr=resumed?seed!.counts!.cr:0;
  let line=resumed?seed!.line:1,lineStart=resumed?seed!.lineStart:0,lastBreak=lineStart;
  let nextPage=(resumed?seed!.start:0)+pageSize,lastPublished=0;
  const snapshot=(scanned:number,complete=false):TextIndex=>({
    source:text,pages,scanned,
    format:{bom:text.startsWith('\ufeff'),newline:cr||lf&&crlf?'mixed':crlf?'crlf':'lf',lines:line},
    complete,
  });
  for(let from=resumed?seed!.start:0;from<text.length;from+=TEXT_WORK_UNITS) {
    throwIfAborted(signal);
    const end=Math.min(text.length,from+TEXT_WORK_UNITS);
    for(let at=from;at<end;at++) {
      const unit=text.charCodeAt(at);
      if(unit===10) {
        if(at>0&&text.charCodeAt(at-1)===13)crlf++;else lf++;
        line++;lineStart=at+1;lastBreak=lineStart;
      } else if(unit===13&&text.charCodeAt(at+1)!==10) {
        cr++;line++;lineStart=at+1;lastBreak=lineStart;
      }
      while(at>=nextPage&&nextPage<text.length) {
        const previous=pages.at(-1)!;
        const start=lastBreak>previous.start?lastBreak:safeTextOffset(text,nextPage);
        pages=pages.append({start,line,lineStart,counts:{lf,crlf,cr}});
        nextPage=start+pageSize;
      }
    }
    if(pages.length!==lastPublished){publish(snapshot(end));lastPublished=pages.length;}
    if(end<text.length)await yieldToBrowser(signal);
  }
  const result=snapshot(text.length,true);publish(result);return result;
}

export interface TextLocation { line:number; column:number }
export async function locateText(text:string,offset:number,index:TextIndex,signal?:AbortSignal):Promise<TextLocation> {
  const at=Math.max(0,Math.min(text.length,offset)),page=index.source===text?pageForOffset(index.pages,at):{start:0,line:1,lineStart:0};
  let line=page.line,lineStart=page.lineStart;
  for(let from=page.start;from<at;from+=TEXT_WORK_UNITS) {
    throwIfAborted(signal);
    const end=Math.min(at,from+TEXT_WORK_UNITS);
    for(let cursor=from;cursor<end;cursor++) {
      if(text[cursor]==='\n'||text[cursor]==='\r'&&text[cursor+1]!=='\n'){line++;lineStart=cursor+1;}
    }
    if(end<at)await yieldToBrowser(signal);
  }
  return {line,column:at-lineStart+1};
}

export async function offsetForLine(text:string,target:number,index:TextIndex,signal?:AbortSignal):Promise<number> {
  const wanted=Math.max(1,Math.trunc(target)||1);
  const pages=index.source===text?index.pages:emptyTextIndex(text).pages;
  let low=0,high=pages.length-1;
  while(low<high){const middle=Math.ceil((low+high)/2);if(pages.at(middle)!.line<=wanted)low=middle;else high=middle-1;}
  const page=pages.at(low)!;
  if(page.line===wanted)return page.lineStart;
  let line=page.line;
  for(let from=page.start;from<text.length;from+=TEXT_WORK_UNITS) {
    throwIfAborted(signal);
    const end=Math.min(text.length,from+TEXT_WORK_UNITS);
    for(let cursor=from;cursor<end;cursor++) {
      if(text[cursor]==='\n'||text[cursor]==='\r'&&text[cursor+1]!=='\n'){
        line++;if(line===wanted)return cursor+1;
      }
    }
    if(end<text.length)await yieldToBrowser(signal);
  }
  return text.length;
}

export interface TextMatch { at:number; length:number }
export async function findText(
  text:string,
  needle:string,
  from:number,
  previous:boolean,
  matchCase:boolean,
  signal?:AbortSignal,
):Promise<TextMatch|undefined> {
  if(!needle)return undefined;
  const start=Math.max(0,Math.min(text.length,from));
  if(previous){
    const direct=await findBackward(text,needle,start,matchCase,signal);
    return direct??(start<text.length?findBackward(text,needle,text.length,matchCase,signal):undefined);
  }
  const direct=await findForward(text,needle,start,text.length,matchCase,signal);
  return direct??(start>0?findForward(text,needle,0,start,matchCase,signal):undefined);
}

export function literalEquals(value:string,needle:string,matchCase:boolean) {
  return new RegExp(`^(?:${escapeRegExp(needle)})$`,matchCase?'u':'iu').test(value);
}

export interface ReplaceTextResult { text:string; count:number; change?:Change }
export async function replaceAllText(
  text:string,
  needle:string,
  replacement:string,
  matchCase:boolean,
  signal?:AbortSignal,
):Promise<ReplaceTextResult> {
  if(!needle)return {text,count:0};
  const expression=new RegExp(escapeRegExp(needle),matchCase?'gu':'giu');
  const parts:string[]=[];
  let cursor=0,count=0,outputLength=0;
  let firstChanged=-1,firstOutput=-1,lastChangedEnd=-1,lastOutputEnd=-1;
  for(let base=0;base<text.length;) {
    throwIfAborted(signal);
    const end=safeTextOffset(text,Math.min(text.length,base+TEXT_WORK_UNITS));
    const extended=safeTextOffset(text,Math.min(text.length,end+needle.length));
    const chunk=text.slice(base,extended);expression.lastIndex=0;
    for(let match=expression.exec(chunk);match;match=expression.exec(chunk)) {
      const at=base+match.index;if(at>=end)break;
      if(at<cursor)continue;
      const prefix=text.slice(cursor,at),matched=match[0];
      parts.push(prefix,replacement);outputLength+=prefix.length+replacement.length;count++;
      if(matched!==replacement) {
        if(firstChanged<0){firstChanged=at;firstOutput=outputLength-replacement.length;}
        lastChangedEnd=at+matched.length;lastOutputEnd=outputLength;
      }
      cursor=at+matched.length;
    }
    base=Math.max(end,cursor);if(base<text.length)await yieldToBrowser(signal);
  }
  if(!count)return {text,count:0};
  parts.push(text.slice(cursor));
  const result=parts.join('');
  if(firstChanged<0)return {text:result,count};
  return {text:result,count,change:{
    at:firstChanged,
    removed:text.slice(firstChanged,lastChangedEnd),
    inserted:result.slice(firstOutput,lastOutputEnd),
  }};
}

export async function normalizeFileText(text:string,signal?:AbortSignal):Promise<string> {
  const start=text.startsWith('\ufeff')?1:0;
  const parts:string[]=[];let from=start;
  while(from<text.length) {
    throwIfAborted(signal);
    let end=Math.min(text.length,from+TEXT_WORK_UNITS);
    if(end<text.length&&text[end-1]==='\r'&&text[end]==='\n')end--;
    parts.push(text.slice(from,end).replaceAll('\r\n','\n'));from=end;
    if(from<text.length)await yieldToBrowser(signal);
  }
  return parts.join('');
}

export interface Change { at:number; removed:string; inserted:string }
export function difference(before:string,after:string):Change {
  let at=0,end=before.length,next=after.length;
  while(at<end&&at<next&&before[at]===after[at])at++;
  while(end>at&&next>at&&before[end-1]===after[next-1]){end--;next--;}
  return {at,removed:before.slice(at,end),inserted:after.slice(at,next)};
}
export function replacementChange(before:string,after:string,start:number,end:number):Change|undefined {
  const at=Math.max(0,Math.min(before.length,start)),until=Math.max(at,Math.min(before.length,end));
  const insertedLength=after.length-before.length+(until-at);
  if(insertedLength<0||at+insertedLength>after.length)return undefined;
  return {at,removed:before.slice(at,until),inserted:after.slice(at,at+insertedLength)};
}
export function deletionChange(before:string,after:string,beforeStart:number,afterStart:number):Change|undefined {
  const removedLength=before.length-after.length;
  if(removedLength<0)return undefined;
  const at=Math.max(0,Math.min(before.length-removedLength,Math.min(beforeStart,afterStart)));
  return {at,removed:before.slice(at,at+removedLength),inserted:''};
}
export function applyChange(text:string,change:Change,undo=false) {
  const remove=undo?change.inserted:change.removed,insert=undo?change.removed:change.inserted;
  return text.slice(0,change.at)+insert+text.slice(change.at+remove.length);
}

async function findForward(text:string,needle:string,from:number,until:number,matchCase:boolean,signal?:AbortSignal):Promise<TextMatch|undefined> {
  const expression=new RegExp(escapeRegExp(needle),matchCase?'gu':'giu');
  for(let base=from;base<until;) {
    throwIfAborted(signal);
    const end=safeTextOffset(text,Math.min(until,base+TEXT_WORK_UNITS));
    const extended=safeTextOffset(text,Math.min(text.length,end+needle.length));
    const chunk=text.slice(base,extended);expression.lastIndex=0;
    const match=expression.exec(chunk);
    if(match&&base+match.index<until&&base+match.index<end)return {at:base+match.index,length:match[0].length};
    base=end;if(base<until)await yieldToBrowser(signal);
  }
  return undefined;
}
async function findBackward(text:string,needle:string,until:number,matchCase:boolean,signal?:AbortSignal):Promise<TextMatch|undefined> {
  const expression=new RegExp(escapeRegExp(needle),matchCase?'gu':'giu');
  for(let end=until;end>0;) {
    throwIfAborted(signal);
    let start=safeTextOffset(text,Math.max(0,end-TEXT_WORK_UNITS));
    if(start>end)start=end;
    const extended=safeTextOffset(text,Math.min(text.length,end+needle.length));
    const chunk=text.slice(start,extended);let found:TextMatch|undefined;expression.lastIndex=0;
    for(let match=expression.exec(chunk);match;match=expression.exec(chunk)){
      const at=start+match.index;if(at>=end)break;found={at,length:match[0].length};
      // Search semantics include overlapping literal matches (for example the
      // previous "aa" in "aaa" begins at index one).
      expression.lastIndex=Math.max(match.index+1,safeTextOffset(chunk,match.index+1));
    }
    if(found)return found;
    end=start;if(end>0)await yieldToBrowser(signal);
  }
  return undefined;
}
function pagePosition(pages:TextPages,offset:number):number {
  let low=0,high=pages.length-1;
  while(low<high){const middle=Math.ceil((low+high)/2);if(pages.at(middle)!.start<=offset)low=middle;else high=middle-1;}
  return low;
}
function pageForOffset(pages:TextPages,offset:number):TextPage {return pages.at(pagePosition(pages,offset))!;}
function escapeRegExp(value:string) {return value.replace(/[.*+?^${}()|[\]\\]/g,'\\$&');}
function isHighSurrogate(unit:number) {return unit>=0xd800&&unit<=0xdbff;}
function isLowSurrogate(unit:number) {return unit>=0xdc00&&unit<=0xdfff;}
function throwIfAborted(signal?:AbortSignal) {
  if(signal?.aborted){const error=new Error('The operation was aborted');error.name='AbortError';throw signal.reason??error;}
}
async function yieldToBrowser(signal?:AbortSignal) {
  await new Promise<void>(resolve=>setTimeout(resolve,0));throwIfAborted(signal);
}
