// Renderer/navigation overhead only; never invokes a backend or starts a model.
import {performance} from 'node:perf_hooks';
import {render,diffFrame} from '../src/terminal.mjs';
const screen={title:'sessionsation',items:[{label:'Codex: New session'},{label:'Resume saved session'},{label:'Plain shell'}],selected:0};
const results=[];
for(const [cols,rows] of [[80,24],[120,40],[200,70]]) {
  const samples=[];let previous,totalBytes=0;
  for(let n=0;n<120;n++) {
    const started=performance.now();
    const frame=render({...screen,selected:n%3},{cols,rows,elapsed:n*50,clock:new Date(0)});
    const output=diffFrame(frame.cells,previous,cols);
    samples.push(performance.now()-started);totalBytes+=Buffer.byteLength(output);previous=frame.cells;
  }
  samples.sort((a,b)=>a-b);
  results.push({cols,rows,frames:samples.length,median_ms:+samples[60].toFixed(3),p95_ms:+samples[114].toFixed(3),max_ms:+samples.at(-1).toFixed(3),mean_output_bytes:Math.round(totalBytes/120)});
}
console.log(JSON.stringify({scope:'synchronous renderer + navigation diff; excludes backend, provider startup and terminal transport',results},null,2));
