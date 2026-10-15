delete globalThis.Atomics; delete globalThis.SharedArrayBuffer;
// A small document/event adapter for native books, not a browser host.
const __nativeData=JSON.parse(__nativeInput);
class NativeNode {
  constructor(data,index){this.data=data;this.index=index;this.listeners={};const set=(k,v)=>{let a=this._styles();a[k]=String(v);this.data.attrs.style=Object.entries(a).map(([k,v])=>k+':'+v).join(';');};const remove=k=>{let a=this._styles();delete a[k];this.data.attrs.style=Object.entries(a).map(([k,v])=>k+':'+v).join(';');};this.style=new Proxy({}, {get:(_,k)=>k==='setProperty'?set:k==='removeProperty'?remove:this._styles()[k.replace(/[A-Z]/g,c=>'-'+c.toLowerCase())]||'',set:(_,k,v)=>{set(k.replace(/[A-Z]/g,c=>'-'+c.toLowerCase()),v);return true;}});}

  _styles(){return Object.fromEntries((this.data.attrs.style||'').split(';').filter(s=>s.includes(':')).map(s=>{let i=s.indexOf(':');return[s.slice(0,i).trim(),s.slice(i+1).trim()];}));}
  get dataset(){const attrs=this.data.attrs;return new Proxy({}, {get:(_,k)=>attrs['data-'+k.replace(/[A-Z]/g,c=>'-'+c.toLowerCase())],set:(_,k,v)=>{attrs['data-'+k.replace(/[A-Z]/g,c=>'-'+c.toLowerCase())]=String(v);return true;}});}
  get checked(){return 'checked' in this.data.attrs;} set checked(v){if(v)this.data.attrs.checked='';else delete this.data.attrs.checked;}
  get value(){return this.data.attrs.value||'';} set value(v){this.data.attrs.value=String(v);}
  get id(){return this.data.attrs.id||'';} set id(v){this.data.attrs.id=String(v);}
  get tagName(){return this.data.tag.toUpperCase();}
  get textContent(){return this.data.changed===null||this.data.changed===undefined?this.data.text:this.data.changed;} set textContent(v){this.data.changed=String(v).slice(0,65536);}
  get innerText(){return this.textContent;} set innerText(v){this.textContent=v;}
  get hidden(){return 'hidden' in this.data.attrs;} set hidden(v){if(v)this.data.attrs.hidden='';else delete this.data.attrs.hidden;}
  get className(){return this.data.attrs.class||'';} set className(v){this.data.attrs.class=String(v);}
  get parentNode(){return this.data.parent===null?null:__nativeNodes[this.data.parent];}
  get children(){return __nativeNodes.filter(n=>n.data.parent===this.index);}
  get classList(){const n=this;return{contains:c=>n.className.split(/\s+/).includes(c),add:(...cs)=>n.className=[...new Set([...n.className.split(/\s+/),...cs])].filter(Boolean).join(' '),remove:(...cs)=>n.className=n.className.split(/\s+/).filter(c=>!cs.includes(c)).join(' '),toggle(c,force){const enable=force===undefined?!this.contains(c):!!force;if(enable)this.add(c);else this.remove(c);return enable;}};}
  getAttribute(k){return this.data.attrs[k]===undefined?null:this.data.attrs[k];} setAttribute(k,v){this.data.attrs[k]=String(v);} removeAttribute(k){delete this.data.attrs[k];}
  addEventListener(type,fn){(this.listeners[type]||(this.listeners[type]=[])).push(fn);if(type==='click'||type==='change')this.data.attrs['data-native-action']='true';}
  removeEventListener(type,fn){this.listeners[type]=(this.listeners[type]||[]).filter(f=>f!==fn);}
  dispatchEvent(event){if(!event.target)event.target=this;event.currentTarget=this;event.preventDefault=()=>{};event.stopPropagation=()=>{event.stopped=true;};for(const fn of this.listeners[event.type]||[])fn.call(this,event);const inline=this.data.attrs['on'+event.type];if(inline)Function('event',inline).call(this,event);if(!event.stopped&&this.parentNode)this.parentNode.dispatchEvent(event);return true;}
  click(){this.dispatchEvent({type:'click'});}
  querySelectorAll(s){return __nativeSelect(s).filter(n=>{let p=n.parentNode;while(p){if(p===this)return true;p=p.parentNode;}return false;});} querySelector(s){return this.querySelectorAll(s)[0]||null;}
  snapshot(){return this.data;}
}
const __nativeNodes=__nativeData.map((d,i)=>new NativeNode(d,i));
function __nativeSelect(s){return __nativeNodes.filter(n=>s.split(',').some(q=>{q=q.trim();if(q[0]==='#')return n.id===q.slice(1);if(q[0]==='.')return n.classList.contains(q.slice(1));const match=q.match(/^([a-z]+)(?:\[([\w-]+)(?:=["']?([^"'\]]+)["']?)?\])?(:checked)?$/i);return !!match&&n.data.tag===match[1].toLowerCase()&&(!match[2]||(n.getAttribute(match[2])!==null&&(!match[3]||n.getAttribute(match[2])===match[3])))&&(!match[4]||n.checked);}));}
const document={readyState:'loading',listeners:{},getElementById:id=>__nativeNodes.find(n=>n.id===id)||null,querySelector:s=>__nativeSelect(s)[0]||null,querySelectorAll:__nativeSelect,addEventListener(type,fn){(this.listeners[type]||(this.listeners[type]=[])).push(fn);},dispatchEvent(e){for(const fn of this.listeners[e.type]||[])fn(e);},body:__nativeNodes.find(n=>n.data.tag==='body'),documentElement:__nativeNodes.find(n=>n.data.tag==='html')};
const window=globalThis; window.document=document; window.listeners={};window.addEventListener=document.addEventListener;window.dispatchEvent=document.dispatchEvent;
const console={log(){},warn(){},error(){}};
