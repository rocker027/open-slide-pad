// 使用最小 DOM 替身驗證設定表單的 reset 契約，不操作原生視窗。
const assert = require('node:assert/strict');
const path = require('node:path');
const fs = require('node:fs');
const vm = require('node:vm');
const ids = new Map();
class Element {
  constructor(tag) { this.tagName=tag; this.children=[]; this.classList={toggle(){}}; this.textContent=''; this.attributes={}; }
  set id(id) { this._id=id; ids.set(id,this); }
  get id() { return this._id; }
  append(...items) { for(const item of items) { item.parent=this; this.children.push(item); } }
  prepend(...items) { for(const item of items.reverse()) { item.parent=this; this.children.unshift(item); } }
  replaceChildren(...items) { this.children=[]; this.append(...items); }
  get lastChild() { return this.children.at(-1); }
  get firstChild() { return this.children[0]; }
  setAttribute(name, value) { this.attributes[name]=value; }
  remove() { this.parent.children.splice(this.parent.children.indexOf(this),1); }
  focus() {} select() {}
}
for(const id of ['pads','home','home-button','site-count','saved-sites','empty-state','suggestions','pin','back','forward','address','status','status-text','undo-remove','status-shortcut','home-shortcut','hero-add','add','settings','address-form','quick','overlay','toast','resize-grip']) {
 const e=new Element('div'); e.id=id; e.append(new Element('#text'));
}
const sent=[];
const ctx={URL,window:{ipc:{postMessage(m){sent.push(JSON.parse(m));}}},document:{documentElement:{dataset:{}},createElement:t=>new Element(t),createTextNode:t=>Object.assign(new Element('#text'),{textContent:t}),getElementById:id=>ids.get(id),querySelectorAll:()=>[],addEventListener(){}},setTimeout(){},clearTimeout(){}};
vm.createContext(ctx);
const sourcePath = process.argv[2] || path.join(__dirname, '../ui/app.js');
vm.runInContext(fs.readFileSync(sourcePath, 'utf8'), ctx);
const state={settings:{pads:[],toggle_shortcut:{control:false,option:false,shift:true,command:true,key:'Space'},width:520,height:null,top_offset:8,side:'right',hot_edge:true,pinned:false},home:true,shortcut_active:true,shortcut_label:'⇧⌘Space',shortcut_error:null};
ctx.window.render(state); ctx.window.showSettings();
ids.get('shortcut-control').checked=true;
ids.get('shortcut-key').value='F19';
function find(e, label) { if(e.textContent===label)return e; for(const c of e.children){const result=find(c,label);if(result)return result;} }
find(ids.get('shortcut-form'),'恢復預設').onclick();
ctx.window.render(JSON.parse(JSON.stringify(state)));
assert.deepEqual(sent.at(-1), {action:'set_shortcut', shortcut:{control:false, option:false, shift:true, command:true, key:'Space'}});
assert.equal(ids.get('shortcut-control').checked, false, '恢復預設必須清除尚未套用的修飾鍵草稿');
assert.equal(ids.get('shortcut-key').value, 'Space', '恢復預設必須清除尚未套用的按鍵草稿');
console.log('PASS shortcut reset clears unsaved draft and submits the default IPC');
const before = sent.length;
ids.get('resize-grip').onpointerdown({button:2});
assert.equal(sent.length, before, '右鍵不可開始調整大小');
let prevented=false;
ids.get('resize-grip').onpointerdown({button:0,preventDefault(){prevented=true;}});
assert.equal(prevented,true);
assert.deepEqual(sent.at(-1),{action:'begin_resize'});
assert.equal(ctx.document.documentElement.dataset.side,'right');
find(ids.get('overlay'),'恢復全高').onclick();
assert.deepEqual(sent.at(-1),{action:'full_height'});
console.log('PASS resize grip routes primary pointer and full-height commands');

// 首頁使用相同的網站順序與 select IPC，最後一個網站移除後回到空狀態。
const populated=JSON.parse(JSON.stringify(state));
populated.settings.pads=[{id:7,title:'工作筆記',url:'https://www.notion.so/team'},
  {id:12,title:'<img src=x onerror=alert(1)>',url:'https://example.com'}];
ctx.window.render(populated);
assert.equal(ids.get('site-count').textContent,'2');
assert.equal(ids.get('empty-state').hidden,true);
assert.equal(ids.get('suggestions').open,false);
const sites=ids.get('saved-sites').children;
assert.equal(sites.length,2);
assert.equal(sites[0].children[1].children[1].textContent,'notion.so');
assert.equal(sites[1].children[1].children[0].textContent,populated.settings.pads[1].title);
sites[1].onclick();
assert.deepEqual(sent.at(-1),{action:'select',id:12});
ids.get('suggestions').open=true;
ctx.window.render({...populated,loading:true});
assert.equal(ids.get('suggestions').open,true,'載入狀態更新不應收起使用者開啟的建議');
ctx.window.render(state);
assert.equal(ids.get('empty-state').hidden,false);
assert.equal(ids.get('saved-sites').children.length,0);
assert.equal(ids.get('suggestions').open,true);
console.log('PASS home list selects saved sites, preserves text and restores empty state');
