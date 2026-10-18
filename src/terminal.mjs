import {emitKeypressEvents} from 'node:readline';
import {hostname} from 'node:os';
import {performance} from 'node:perf_hooks';
import {display} from './client.mjs';

const segmenter=new Intl.Segmenter(undefined,{granularity:'grapheme'});
const clamp=(x,a=0,b=1)=>Math.max(a,Math.min(b,x));
const smooth=x=>{x=clamp(x);return x*x*(3-2*x);};
export const glyphs=s=>[...segmenter.segment(display(s))].map(x=>x.segment);
export function cellWidth(s) {
  return glyphs(s).reduce((n,g)=>n+glyphWidth(g),0);
}
function glyphWidth(g) {
  const c=g.codePointAt(0);
  if (/^\p{Mark}+$/u.test(g)) return 0;
  return /\p{Emoji_Presentation}|\uFE0F|\u200D/u.test(g) || c>=0x1100 && (c<=0x115f || c>=0x2e80&&c<=0xa4cf || c>=0xac00&&c<=0xd7af || c>=0xf900&&c<=0xfaff || c>=0xfe10&&c<=0xfe6f || c>=0xff01&&c<=0xff60 || c>=0x20000) ? 2:1;
}
export function clip(s,width) {
  let text='',used=0;
  for(const g of glyphs(s)){const w=glyphWidth(g);if(used+w>width)break;text+=g;used+=w;}
  return text;
}
const hash=(x,y)=>{let n=Math.imul(x+19,374761393)^Math.imul(y+7,668265263);n=Math.imul(n^(n>>>13),1274126177);return (n^(n>>>16))>>>0;};
function rgb(h,s,l) {
  h=((h%360)+360)%360/30;
  const a=s*Math.min(l,1-l);
  return [0,8,4].map(n=>Math.round(255*(l-a*Math.max(-1,Math.min((n+h)%12-3,9-(n+h)%12,1)))));
}
function colorAt(x,y,cols,rows,elapsed,{accent=false,selected=false}={}) {
  const cycle=Math.floor(elapsed/4000),phase=elapsed%4000;
  const depth=(y-2)/Math.max(1,rows-3);
  const distance=.45*x/Math.max(1,cols-1)+.55*clamp(depth);
  const arrival=1167*(1-Math.sqrt(1-clamp(distance)));
  const age=phase-arrival;
  const flash=age>=0&&age<300?smooth(age<60?age/60:1-(age-60)/240):0;
  const hue=(accent?80:260)+137.508*(age>=0?cycle:cycle-1);
  const base=selected?[240,240,245]:rgb(hue,accent?.72:.30,accent?.65:.65-.58*clamp(depth));
  const peak=255*clamp((1-clamp(depth))/.7,.025,1);
  return base.map(v=>Math.round(v+(Math.max(v,peak)-v)*flash));
}

export function layout(cols,rows,count,selected=0) {
  cols=Math.max(1,Math.floor(cols));rows=Math.max(1,Math.floor(rows));
  const compact=cols<22||rows<7;
  const hints=cols>=76?['↑/↓: Move   Enter/Space: Select   ←/→: Back/Options   Esc: Back']:
    cols>=42?['↑/↓ Move   Enter/Space Select','←/→ Back/Options   Esc Back']:['↑↓ Move  Enter Select','Esc Back'];
  const hintStart=Math.max(0,rows-hints.length);
  const backRow=Math.max(0,hintStart-1),start=rows>=12?5:3;
  const visible=Math.max(0,backRow-start-1);
  const first=visible?Math.max(0,Math.min(Math.max(0,count-visible),selected-visible+1)):0;
  return {cols,rows,compact,hints,hintStart,backRow,start,visible,first};
}

// Independent terminal-cell renderer, visually following Ana's cli-main-menu.
// The field's geometry depends on terminal height, never option count.
export function render(screen,{cols=80,rows=24,elapsed=0,clock=new Date(),host=hostname().split('.')[0]}={}) {
  const items=screen.items??[],selected=screen.selected??0;
  const g=layout(cols,rows,items.length,selected);({cols,rows}=g);
  const cells=Array.from({length:cols*rows},()=>({char:' ',fg:[200,200,210],bold:false,bg:null}));
  for(let y=2;y<rows;y++)for(let x=0;x<cols;x++) {
    const seed=hash(x,y),depth=(y-2)/Math.max(1,rows-3);
    if(seed%10000>600+2200*depth*depth)continue;
    const pulse=.6+.4*Math.sin(elapsed/400+(seed%1000));
    const fg=colorAt(x,y,cols,rows,elapsed).map(v=>Math.max(1,Math.round(v*pulse/4)*4));
    cells[y*cols+x]={char:'.+*'[Math.floor((seed%900+elapsed/35)%900/300)],fg,bold:false,bg:null};
  }
  function text(y,x,value,style={}) {
    if(y<0||y>=rows)return;
    for(const char of glyphs(value)) {
      const w=glyphWidth(char);if(x+w>cols)break;
      if(x>=0&&w){cells[y*cols+x]={char,fg:style.fg??colorAt(x,y,cols,rows,elapsed,style),bold:!!style.bold,bg:style.bg??null};
        if(w===2)cells[y*cols+x+1]={char:'',fg:[0,0,0],bold:false,bg:style.bg??null};}
      x+=w;
    }
  }
  const center=(y,s,style)=>text(y,Math.max(0,Math.floor((cols-cellWidth(clip(s,cols)))/2)),clip(s,cols),style);
  const bg=rgb(260+Math.floor(elapsed/4000)*137.508,.22,.32);
  for(let x=0;x<cols;x++)cells[x]={char:' ',fg:[245,245,250],bold:false,bg};
  text(0,1,clip(`${clock.toLocaleTimeString('en-GB')} | ${host} | ${screen.cwd??process.cwd()}`,cols-2),{fg:[245,245,250],bg});
  if(g.compact){center(Math.min(2,rows-1),'Enlarge terminal',{selected:true});}
  else {
    center(2,screen.title??'sessionsation',{accent:true,bold:true});
    if(screen.description&&g.start>3)center(3,screen.description,{fg:[165,165,180]});
    if(screen.kind==='text') {
      const width=Math.max(1,cols-8),chars=glyphs(screen.value??'');
      const cursor=screen.cursor??chars.length;
      let first=cursor;
      while(first>0&&cellWidth(chars.slice(first-1,cursor).join(''))<width-1)first--;
      text(g.start,3,'› ',{accent:true,bold:true});
      const value=clip(chars.slice(first).join(''),width);
      text(g.start,5,value,{selected:true});
      const cursorCol=5+cellWidth(chars.slice(first,cursor).join(''));
      text(g.start,cursorCol,chars[cursor]??' ',{fg:[10,10,20],bg:[215,190,245]});
    } else if(screen.kind==='message') {
      center(g.start,screen.message??'',{selected:true});
    } else {
      const width=Math.min(cols-8,Math.max(18,...items.map(i=>cellWidth(i.label)+3)));
      const left=Math.max(2,Math.floor((cols-width)/2));
      for(let i=g.first;i<Math.min(items.length,g.first+g.visible);i++) {
        const item=items[i],y=g.start+i-g.first,isSelected=i===selected&&!screen.back;
        text(y,left,isSelected?'●':'○',{accent:true,bold:isSelected});
        text(y,left+2,clip(item.label,width-2),item.disabled?{fg:[95,95,110]}:{accent:!isSelected,selected:isSelected,bold:isSelected});
      }
      if(!items.length)center(g.start,screen.empty??'No available choices',{fg:[180,180,195]});
      if(items.length>g.visible)center(g.backRow-1,`${g.first+1}–${Math.min(items.length,g.first+g.visible)} of ${items.length}`,{fg:[160,160,180]});
    }
    if(screen.error)center(g.backRow-1,screen.error,{fg:[255,150,165]});
    center(g.backRow,screen.back?`◀— ${screen.home?'Exit':'Back'}`:'◀—',{selected:!!screen.back,bold:!!screen.back,accent:!screen.back});
  }
  const hints=screen.kind==='text'?['Enter Accept   Esc Back   Ctrl-U Clear']:g.hints;
  const start=rows-hints.length;
  hints.forEach((s,i)=>center(start+i,s,{accent:true}));
  return {cells,geometry:g};
}

export function diffFrame(frame,previous,cols) {
  let out='',style='',at=-1;
  for(let i=0;i<frame.length;i++) {
    const cell=frame[i],old=previous?.[i];
    if(cell.char==='')continue;
    const token=`${cell.bold?1:22};38;2;${cell.fg.join(';')};${cell.bg?'48;2;'+cell.bg.join(';'):49}`;
    if(old&&old.char===cell.char&&old.bold===cell.bold&&String(old.fg)===String(cell.fg)&&String(old.bg)===String(cell.bg))continue;
    if(i!==at||i%cols===0)out+=`\x1b[${Math.floor(i/cols)+1};${i%cols+1}H`;
    if(token!==style){out+=`\x1b[${token}m`;style=token;}
    out+=cell.char;at=i+glyphWidth(cell.char);
  }
  return out;
}

export class Cancelled extends Error {}
export const CANCELLED=Symbol('cancelled');
export class Terminal {
  constructor({input=process.stdin,output=process.stdout,signals=process,now=()=>performance.now()}={}) {
    this.input=input;this.output=output;this.signals=signals;this.now=now;this.origin=now();
    this.screen={title:'sessionsation',items:[]};this.closed=false;this.raw=input.isRaw??false;this.flowing=input.readableFlowing;
    this.keys=[];
    this.key=(s,k={})=>{
      if(k.name==='paste-start'){this.pasting=true;return;}
      if(k.name==='paste-end'){this.pasting=false;return;}
      if(this.pasting){if(this.screen.kind!=='text'||!s||display(s)!==s)return;k={};}
      const cancel=k.name==='escape'||k.ctrl&&['c','d'].includes(k.name);
      if(this.waiting&&cancel){this.keys=[];this.onKey?.(s,k);return;}
      if(this.keys.length<256)this.keys.push([s,k]);
      this.flushKeys();
    };
    this.resize=()=>{this.previous=null;this.paint();};
    this.end=()=>this.reject?.(new Cancelled());
    this.signal=()=>{this.reject?.(new Cancelled());this.close();};
    this.drain=()=>{this.blocked=false;this.paint();};
    emitKeypressEvents(input,{escapeCodeTimeout:35});
    input.setRawMode(true);input.on('keypress',this.key);input.on('end',this.end);input.resume();
    output.on('resize',this.resize);output.on('drain',this.drain);
    for(const name of ['SIGINT','SIGTERM','SIGHUP'])signals.on(name,this.signal);
    output.write('\x1b[?1049h\x1b[?25l\x1b[?7l\x1b[?2004h');
    this.timer=setInterval(()=>this.paint(),50);
  }
  paint() {
    if(this.closed||this.blocked)return;
    const cols=this.output.columns||80,rows=this.output.rows||24;
    const {cells}=render(this.screen,{cols,rows,elapsed:this.now()-this.origin});
    const out=diffFrame(cells,this.previous,cols);this.previous=cells;
    if(out)this.blocked=!this.output.write(out);
  }
  flushKeys() {
    while(this.onKey&&!this.waiting&&this.keys.length){const [s,k]=this.keys.shift();this.onKey(s,k);}
  }
  prompt(screen,handler) {
    this.screen=screen;this.paint();
    return new Promise((resolve,reject)=>{
      this.reject=reject;
      const done=value=>{this.onKey=null;resolve(value);};
      this.onKey=(text,key)=>{
        if(key.ctrl&&['c','d'].includes(key.name)){reject(new Cancelled());return;}
        handler(text,key,done,reject);this.paint();
      };
      queueMicrotask(()=>this.flushKeys());
    }).finally(()=>{this.onKey=null;this.reject=null;});
  }
  async choose(title,items,{description='',home=false,selected=0,onKey,empty}={}) {
    if(items[selected]?.disabled)selected=Math.max(0,items.findIndex(i=>!i.disabled));
    const screen={title,items,selected,description,home,empty};
    return this.prompt(screen,(text,k,done)=>{
      if(k.name==='escape'){done(null);return;}
      const g=layout(this.output.columns||80,this.output.rows||24,items.length,screen.selected);
      if(g.compact)return;
      if(onKey?.(k,screen,done))return;
      if(k.name==='left'){screen.back=true;return;}
      if(k.name==='right'){screen.back=false;return;}
      if(['up','down','home','end','pageup','pagedown'].includes(k.name)) {
        screen.back=false;
        let index=k.name==='home'?0:k.name==='end'?items.length-1:screen.selected+({up:-1,down:1,pageup:-Math.max(1,g.visible),pagedown:Math.max(1,g.visible)}[k.name]);
        for(let n=0;n<items.length;n++){index=(index+items.length)%items.length;if(!items[index].disabled)break;index+=k.name==='up'?-1:1;}
        screen.selected=Math.max(0,index);return;
      }
      if(k.name==='return'||text===' ') {
        if(screen.back)done(null);
        else if(items[screen.selected]&&!items[screen.selected].disabled)done({item:items[screen.selected],index:screen.selected});
      }
    });
  }
  async text(title,value='',{description=''}={}) {
    const screen={title,kind:'text',value,description,cursor:glyphs(value).length};
    return this.prompt(screen,(text,k,done)=>{
      let chars=glyphs(screen.value);
      if(k.name==='escape'){done(null);return;}
      if(layout(this.output.columns||80,this.output.rows||24,0).compact)return;
      if(k.name==='return'){done(screen.value);return;}
      if(k.ctrl&&k.name==='u'){chars=[];screen.cursor=0;}
      else if(k.name==='left')screen.cursor=Math.max(0,screen.cursor-1);
      else if(k.name==='right')screen.cursor=Math.min(chars.length,screen.cursor+1);
      else if(k.name==='home'||k.ctrl&&k.name==='a')screen.cursor=0;
      else if(k.name==='end'||k.ctrl&&k.name==='e')screen.cursor=chars.length;
      else if(k.name==='backspace'&&screen.cursor>0)chars.splice(--screen.cursor,1);
      else if(k.name==='delete')chars.splice(screen.cursor,1);
      else if(text&&!k.ctrl&&!k.meta&&display(text)===text){const added=glyphs(text);chars.splice(screen.cursor,0,...added);screen.cursor+=added.length;}
      screen.value=chars.join('');
    });
  }
  async wait(title,work) {
    const controller=new AbortController();
    try {
      this.screen={title,kind:'message',message:'Reading…'};this.paint();
      return await new Promise((resolve,reject)=>{
        this.waiting=true;
        this.reject=error=>{controller.abort();reject(error);};
        this.onKey=(_text,k)=>{
          if(k.ctrl&&['c','d'].includes(k.name))this.reject(new Cancelled());
          else if(k.name==='escape'){controller.abort();resolve(CANCELLED);}
        };
        Promise.resolve().then(()=>controller.signal.aborted?CANCELLED:work(controller.signal)).then(resolve,reject);
      });
    } finally {controller.abort();this.waiting=false;this.onKey=null;this.reject=null;}
  }
  close() {
    if(this.closed)return;this.closed=true;clearInterval(this.timer);
    this.input.removeListener('keypress',this.key);this.input.removeListener('end',this.end);
    this.output.removeListener('resize',this.resize);this.output.removeListener('drain',this.drain);
    for(const name of ['SIGINT','SIGTERM','SIGHUP'])this.signals.removeListener(name,this.signal);
    this.input.setRawMode(this.raw);if(!this.flowing)this.input.pause();
    this.output.write('\x1b[0m\x1b[?2004l\x1b[?7h\x1b[?25h\x1b[?1049l');
  }
}
