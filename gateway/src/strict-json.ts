/** Reject duplicate keys before native JSON parsing can discard them. */
export function strictParse(source: string): unknown {
  if (source.length > 262144) throw new Error('JSON exceeds limit');
  let i=0;
  const ws=()=>{while(/[\t\n\r ]/.test(source[i]??'') && i<source.length)i++;};
  const string=():string=>{
    if(source[i]!== '"')throw new Error('expected string');
    const start=i++;
    while(i<source.length){if(source[i]==='\\'){i+=2;continue;}if(source[i++]==='"')return JSON.parse(source.slice(start,i));}
    throw new Error('unterminated string');
  };
  const value=(depth:number):void=>{
    if(depth>64)throw new Error('JSON nesting exceeds limit');ws();
    if(source[i]==='"'){string();return;}
    if(source[i]==='{'){
      i++;ws();const seen=new Set<string>();if(source[i]==='}'){i++;return;}
      while(true){ws();const key=string();if(seen.has(key))throw new Error('duplicate JSON key');seen.add(key);ws();if(source[i++]!==':')throw new Error('expected colon');value(depth+1);ws();const end=source[i++];if(end==='}')return;if(end!==',')throw new Error('expected comma');}
    }
    if(source[i]==='['){i++;ws();if(source[i]===']'){i++;return;}while(true){value(depth+1);ws();const end=source[i++];if(end===']')return;if(end!==',')throw new Error('expected comma');}}
    const match=/^(?:true|false|null|-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?)/.exec(source.slice(i));
    if(!match)throw new Error('invalid JSON value');i+=match[0].length;
  };
  value(0);ws();if(i!==source.length)throw new Error('trailing JSON');return JSON.parse(source);
}
