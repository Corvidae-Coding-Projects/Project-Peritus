import {describe,it,expect} from 'vitest';
import {difference,applyChange,pageStarts,textFormat} from './text';

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
  it('recognizes BOM and mixed newlines rather than silently normalizing them',()=>{
    expect(textFormat('\ufeffa\r\nb\r\n')).toEqual({bom:true,newline:'crlf',lines:3});
    expect(textFormat('a\nb\r\n').newline).toBe('mixed');
    expect(textFormat('a\rb').newline).toBe('mixed');
  });
});
