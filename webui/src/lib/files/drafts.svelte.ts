import {textFormat,type TextSnapshot} from './text';

export interface FileDraft {
  text:string; original:string; revision:string; bom:boolean; newline:string;
  editing:boolean; saving:boolean; error:string;
}
export const fileDrafts=$state<Record<string,FileDraft>>({});
export const fileKey=(project:string,path:string)=>JSON.stringify([project,path]);
export function retainFile(project:string,path:string,value:TextSnapshot) {
  const key=fileKey(project,path);
  if(fileDrafts[key])return fileDrafts[key]!;
  const format=textFormat(value.text);
  const text=value.text.replace(/^\ufeff/,'').replaceAll('\r\n','\n');
  fileDrafts[key]={text,original:text,revision:value.revision,bom:format.bom,newline:format.newline,editing:false,saving:false,error:''};
  return fileDrafts[key]!;
}
export function dirtyFile(project:string,path:string) {
  const draft=fileDrafts[fileKey(project,path)];return !!draft&&draft.text!==draft.original;
}
export function fileBytes(draft:FileDraft) {
  return (draft.bom?'\ufeff':'')+(draft.newline==='crlf'?draft.text.replaceAll('\n','\r\n'):draft.text);
}
export function forgetFile(project:string,path:string) {
  const key=fileKey(project,path),draft=fileDrafts[key];
  if(draft?.saving)return false;
  if(dirtyFile(project,path)&&!confirm(`Discard unsaved changes to ${path}?`))return false;
  delete fileDrafts[key];return true;
}
export function protectFileDrafts(event:BeforeUnloadEvent) {
  if(Object.values(fileDrafts).some(draft=>draft.saving||draft.text!==draft.original)) {
    event.preventDefault();event.returnValue='';
  }
}
