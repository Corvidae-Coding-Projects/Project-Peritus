import {TEXT_PAGE_UNITS,type TextSnapshot} from './text';

export interface TextReadProgress {text:string;bytes:number;verifying:boolean}

// Physical source frames are independent of the complete file size. Retain every
// frame, publish a usable first page, and join once the full preimage is verified.
export async function decodeTextSource(
  response:Response,
  publish:(value:TextReadProgress)=>void,
  signal?:AbortSignal,
):Promise<TextSnapshot> {
  if(!response.body)throw new Error('The gateway did not return a text stream.');
  const reader=response.body.getReader(),decoder=new TextDecoder('utf-8',{fatal:true});
  const encoder=new TextEncoder(),parts:string[]=[];
  let pending='',offset=0n,prefix='',verifying=false;
  function packet(line:string):TextSnapshot|undefined {
    const value:unknown=JSON.parse(line);
    if(!value||typeof value!=='object')throw new Error('Invalid file observation frame.');
    const record=value as Record<string,unknown>;
    if(typeof record.error==='string')throw new Error(record.error);
    if(record.complete===true){
      if(decimal(record.bytes)!==offset||typeof record.revision!=='string'||!/^[a-f0-9]{64}$/.test(record.revision))throw new Error('File observation completion does not match its source.');
      return {text:parts.join(''),revision:record.revision,bytes:Number(offset)};
    }
    if(typeof record.text==='string'){
      if(verifying||decimal(record.offset)!==offset)throw new Error('File observation slices are out of order.');
      const bytes=encoder.encode(record.text).length;
      if(bytes===0)throw new Error('File observation did not advance.');
      parts.push(record.text);offset+=BigInt(bytes);
      if(prefix.length<TEXT_PAGE_UNITS){
        let end=Math.min(record.text.length,TEXT_PAGE_UNITS-prefix.length);
        if(end>0&&end<record.text.length&&isHighSurrogate(record.text.charCodeAt(end-1)))end++;
        prefix+=record.text.slice(0,end);
      }
    }else if(record.verifying!==undefined){
      if(decimal(record.bytes)!==offset||decimal(record.verifying)>offset)throw new Error('Invalid source verification progress.');
      verifying=true;
    }else throw new Error('Unknown file observation frame.');
    publish({text:prefix,bytes:Number(offset),verifying});
    return undefined;
  }
  try{
    for(;;){
      signal?.throwIfAborted();
      const chunk=await reader.read();
      pending+=decoder.decode(chunk.value,{stream:!chunk.done});
      for(let end=pending.indexOf('\n');end>=0;end=pending.indexOf('\n')){
        const line=pending.slice(0,end);pending=pending.slice(end+1);
        const result=packet(line);if(result)return result;
        // Yield between retained physical slices, including coalesced network reads.
        await new Promise<void>(resolve=>setTimeout(resolve,0));signal?.throwIfAborted();
      }
      if(chunk.done){
        if(pending){const result=packet(pending);if(result)return result;}
        throw new Error('File observation ended before its complete source was verified. Reload to read the original file again.');
      }
    }
  }finally{await reader.cancel().catch(()=>{});reader.releaseLock();}
}

function decimal(value:unknown):bigint {
  if(typeof value!=='string'||!/^(0|[1-9][0-9]*)$/.test(value))throw new Error('Invalid source byte offset.');
  return BigInt(value);
}
function isHighSurrogate(unit:number){return unit>=0xd800&&unit<=0xdbff;}
