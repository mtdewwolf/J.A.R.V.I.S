import {test,expect} from 'bun:test';
import {strictParse} from './strict-json';
import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
function canonical(v:unknown):string {
  if(Array.isArray(v))return `[${v.map(canonical).join(',')}]`;
  if(v!==null&&typeof v==='object')return `{${Object.keys(v).sort().map(k=>`${JSON.stringify(k)}:${canonical((v as Record<string,unknown>)[k])}`).join(',')}}`;
  return JSON.stringify(v);
}
test('duplicate keys and excessive nesting fail closed',()=>{
 expect(()=>strictParse('{"args":{"path":"a","path":"b"}}')).toThrow();
 expect(()=>strictParse('{"a":1,"\\u0061":2}')).toThrow();
 expect(()=>strictParse('['.repeat(65)+'0'+']'.repeat(65))).toThrow();
 expect(strictParse('{"x":[true,null,"a\\\"b",1e-7]}')).toEqual({x:[true,null,'a"b',1e-7]});
});
test('canonicalization vectors independently match JavaScript number and UTF-16 rules',()=>{
 const fixture=JSON.parse(readFileSync(new URL('../../tests/contract-vectors/canonicalization.json',import.meta.url),'utf8'));
 for(const v of fixture.vectors){expect(canonical(v.input),v.name).toBe(v.canonical);expect(createHash('sha256').update(canonical(v.input)).digest('hex')).toHaveLength(64);}
});
