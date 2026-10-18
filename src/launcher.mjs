import {readdir,stat,mkdir,realpath} from 'node:fs/promises';
import {resolve,dirname,basename,join} from 'node:path';
import {homedir} from 'node:os';
import {Terminal,Cancelled,CANCELLED} from './terminal.mjs';
import {targetArgs,display,validatePlan} from './client.mjs';

export class DirectoryPicker {
  constructor(start,{fs={readdir,stat,mkdir,realpath}}={}) {
    this.path=resolve(start);this.fs=fs;this.history=new Map();this.selected=0;
  }
  async entries() {
    const all=await this.fs.readdir(this.path,{withFileTypes:true});
    const dirs=await Promise.all(all.map(async e=>{
      if(e.isDirectory())return e.name;
      if(e.isSymbolicLink())try{if((await this.fs.stat(join(this.path,e.name))).isDirectory())return e.name;}catch{}
      return null;
    }));
    return [{id:'select',label:'Use this directory'},{id:'type',label:'Type a path…'},{id:'mkdir',label:'Create a directory…'},
      ...dirs.filter(x=>x!==null).sort((a,b)=>a.localeCompare(b)).map(name=>({id:'directory',name,label:`${display(name)}/`}))];
  }
  async visit(path,index=this.selected) {
    const next=await this.fs.realpath(path);
    if(!(await this.fs.stat(next)).isDirectory())throw new Error('Choose an existing directory');
    this.history.set(this.path,index);this.path=next;this.selected=this.history.get(next)??index;
  }
  async parent(index) {
    const child=basename(this.path),parent=dirname(this.path);
    if(parent===this.path)return;
    await this.visit(parent,index);
    const entries=await this.entries();this.selected=Math.max(0,entries.findIndex(x=>x.name===child));
  }
  async typed(value) {
    const path=value==='~'?homedir():value.startsWith('~/')?join(homedir(),value.slice(2)):resolve(this.path,value);
    await this.visit(path);
  }
  async create(name) {
    if(!name||name==='.'||name==='..'||name.includes('/')||name.includes('\0'))throw new Error('Enter one new folder name');
    const path=join(this.path,name);await this.fs.mkdir(path);await this.visit(path,0);
  }
}

export async function pickDirectory(ui,start) {
  const picker=new DirectoryPicker(start);let error='';
  while(true) {
    let entries;
    try {entries=await ui.wait('Start directory',()=>picker.entries());}
    catch(e){if(e instanceof Cancelled)throw e;await ui.choose('Cannot read directory',[{id:'back',label:'Back'}],{description:e.message});return null;}
    if(entries===CANCELLED)return null;
    picker.selected=Math.min(picker.selected,entries.length-1);
    const choice=await ui.choose('Start directory',entries,{description:error||picker.path,selected:picker.selected,onKey:(k,screen,done)=>{
      if(screen.back)return false;
      if(k.name==='left'&&screen.selected>0&&dirname(picker.path)!==picker.path){done({navigate:'parent',index:screen.selected});return true;}
      if((k.name==='right'||k.name==='tab')&&entries[screen.selected]?.id==='directory'){done({navigate:'child',index:screen.selected,item:entries[screen.selected]});return true;}
      return false;
    }});
    if(!choice)return null;
    picker.selected=choice.index??picker.selected;
    try {
      error='';
      if(choice.navigate==='parent'){if(await ui.wait('Parent directory',()=>picker.parent(choice.index))===CANCELLED)return null;continue;}
      if(choice.item.id==='directory'){if(await ui.wait('Open directory',()=>picker.visit(join(picker.path,choice.item.name),choice.index))===CANCELLED)return null;continue;}
      if(choice.item.id==='select') {
        const path=await ui.wait('Choose directory',()=>picker.fs.realpath(picker.path));
        return path===CANCELLED?null:path;
      }
      const value=await ui.text(choice.item.id==='type'?'Type directory path':'Create directory',choice.item.id==='type'?picker.path:'',{description:picker.path});
      if(value===null)continue;
      const result=await ui.wait('Open directory',()=>choice.item.id==='type'?picker.typed(value):picker.create(value));
      if(result===CANCELLED)return null;
    }catch(e){if(e instanceof Cancelled)throw e;error=display(e.message);}
  }
}

// This is a launcher only: no account, credential or agent-lifecycle mutations.
export async function runLauncher(client,{ui=new Terminal(),cwd=process.cwd()}={}) {
  try {
    while(true) {
      const action=await ui.choose('sessionsation',[
        {id:'new',label:'Codex: New session'},
        {id:'resume',label:'Resume saved session'},
        {id:'shell',label:'Plain shell'},
      ],{home:true,description:cwd});
      if(!action)return null;
      try {
        let directory=cwd,args=['launcher',action.item.id];
        if(action.item.id==='new') {
          directory=await pickDirectory(ui,directory);if(directory===null)continue;
          const accounts=await ui.wait('Choose account',signal=>client.call(['account','list'],{signal}));
          if(accounts===CANCELLED)continue;
          const selected=await ui.choose('New session: account',accounts.map(a=>({id:a.id,label:`${a.id}${a.plan?'  ['+a.plan+']':''}${!a.selectable?'  unavailable':''}`,disabled:!a.selectable})),{description:directory,empty:'No selectable accounts — use the management CLI'});
          if(!selected)continue;
          args.push('--account',selected.item.id);
        } else if(action.item.id==='resume') {
          const state=await ui.wait('Resume saved session',signal=>client.call(['state'],{signal}));
          if(state===CANCELLED)continue;
          const selected=await ui.choose('Resume saved session',state.sessions.filter(s=>s.capabilities?.resume&&s.availability==='saved').map(s=>({id:s.id,label:`${s.name}  [${s.account_id}]`,session:s})),{empty:'No saved resumable sessions'});
          if(!selected)continue;
          args.push(...targetArgs(selected.item.session));
        }
        const plan=await ui.wait('Preparing terminal',signal=>client.call(args,{signal}));
        if(plan===CANCELLED)continue;
        validatePlan(plan);
        // Never modify opaque argv. New inherits the chosen directory when the
        // backend plan leaves cwd unset; Resume's backend-owned cwd takes priority.
        return {plan,directory:action.item.id==='new'&&plan.cwd===null?directory:null};
      }catch(e) {
        if(e instanceof Cancelled)throw e;
        await ui.choose('Launcher could not continue',[{id:'back',label:'Back to launcher'}],{description:`${e.code??'UI_ERROR'}: ${display(e.message)}`});
      }
    }
  }catch(e){if(!(e instanceof Cancelled))throw e;return null;}
  finally{ui.close();}
}
