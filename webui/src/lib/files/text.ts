export interface TextSnapshot { text:string; revision:string; bytes:number }
export function textFormat(text:string) {
  let lf=0,crlf=0,cr=0;
  for(let i=0;i<text.length;i++) {
    if(text[i]==='\n') { if(text[i-1]==='\r')crlf++;else lf++; }
    if(text[i]==='\r'&&text[i+1]!=='\n')cr++;
  }
  return {bom:text.startsWith('\ufeff'),newline:cr||lf&&crlf?'mixed':crlf?'crlf':'lf',lines:lf+crlf+cr+1};
}
export function pageStarts(text:string,size=128_000) {
  const starts=[0];
  for(let at=size;at<text.length;) {
    const newline=text.lastIndexOf('\n',at);
    const end=newline>(starts.at(-1)??0)?newline+1:at;
    starts.push(end);at=end+size;
  }
  return starts;
}
export interface Change { at:number; removed:string; inserted:string }
export function difference(before:string,after:string):Change {
  let at=0,end=before.length,next=after.length;
  while(at<end&&at<next&&before[at]===after[at])at++;
  while(end>at&&next>at&&before[end-1]===after[next-1]){end--;next--;}
  return {at,removed:before.slice(at,end),inserted:after.slice(at,next)};
}
export function applyChange(text:string,change:Change,undo=false) {
  const remove=undo?change.inserted:change.removed,insert=undo?change.removed:change.inserted;
  return text.slice(0,change.at)+insert+text.slice(change.at+remove.length);
}
