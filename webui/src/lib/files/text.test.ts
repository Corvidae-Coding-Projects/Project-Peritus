import {describe,it,expect} from 'vitest';
import {applyChange,difference,findText,indexText,locateText,pageStarts,replaceAllText,textFormat} from './text';

describe('quick text editing',()=>{
  it('reverses insertion, deletion, Unicode and replacements',()=>{
    for(const [before,after] of [['abc','abXc'],['abXc','abc'],['😀 old\n','😀 new\n'],['','first'],['last','']]){
      const change=difference(before!,after!);
      expect(applyChange(before!,change)).toBe(after);
      expect(applyChange(after!,change,true)).toBe(before);
    }
  });
  it('preserves page access without constructing a DOM row per line',()=>{
    const text='a\n'.repeat(100_000),starts=pageStarts(text);
    expect(starts.length).toBe(2);
    expect(starts.map((at,i)=>text.slice(at,starts[i+1])).join('')).toBe(text);
    expect(pageStarts('x'.repeat(256_001))).toEqual([0,128_000,256_000]);
  });
  it('never places a page boundary inside a surrogate pair',async()=>{
    const text=`${'x'.repeat(127_999)}😀${'y'.repeat(128_000)}`;
    const starts=pageStarts(text),indexed=await indexText(text);
    expect(starts[1]).toBe(128_001);
    expect(indexed.pages[1]?.start).toBe(128_001);
    expect(starts.map((at,index)=>text.slice(at,starts[index+1])).join('')).toBe(text);
  });
  it('indexes line locations incrementally and keeps literal search and replacement exact',async()=>{
    const text=`${'prefix\n'.repeat(20_000)}Straße and STRASSE\ntail`;
    const index=await indexText(text);
    expect(await locateText(text,text.indexOf('tail'),index)).toEqual({line:20_002,column:1});
    expect(await findText(text,'straße',0,false,false)).toEqual({at:text.indexOf('Straße'),length:6});
    const replaced=await replaceAllText(text,'STRASSE','street',false);
    expect(replaced.count).toBe(1);
    expect(replaced.change&&applyChange(text,replaced.change)).toBe(replaced.text);
    expect(replaced.text.endsWith('Straße and street\ntail')).toBe(true);
    expect(await findText('aaa','aa',3,true,true)).toEqual({at:1,length:2});
  });
  it('recognizes BOM and mixed newlines rather than silently normalizing them',()=>{
    expect(textFormat('\ufeffa\r\nb\r\n')).toEqual({bom:true,newline:'crlf',lines:3});
    expect(textFormat('a\nb\r\n').newline).toBe('mixed');
    expect(textFormat('a\rb').newline).toBe('mixed');
  });
});
