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
  focus() { ctx.document.activeElement=this; } select() {}
}
for(const id of ['pads','home','home-button','site-count','saved-sites','empty-state','suggestions','pin','back','forward','address','status','status-text','undo-remove','status-shortcut','home-shortcut','hero-add','add','settings','address-form','quick','overlay','toast','resize-grip','app-version']) {
 const e=new Element('div'); e.id=id; e.append(new Element('#text'));
}
const sent=[];
const ctx={URL,window:{ipc:{postMessage(m){sent.push(JSON.parse(m));}}},document:{documentElement:{dataset:{}},createElement:t=>new Element(t),createTextNode:t=>Object.assign(new Element('#text'),{textContent:t}),getElementById:id=>ids.get(id),querySelectorAll:()=>[],addEventListener(){}},setTimeout(){},clearTimeout(){}};
vm.createContext(ctx);
const sourcePath = process.argv[2] || path.join(__dirname, '../ui/app.js');
const catalog=JSON.parse(fs.readFileSync(path.join(__dirname, '../ui/locales/en.json'), 'utf8'));
const i18nSource=fs.readFileSync(path.join(__dirname, '../ui/i18n.js'), 'utf8').replace('/* SLIDERUST_ENGLISH */', JSON.stringify(catalog));
vm.runInContext(i18nSource,ctx);
const appSource=fs.readFileSync(sourcePath, 'utf8').replace('/* SLIDERUST_VERSION */', '9.9.9');
vm.runInContext(appSource, ctx);
for (const [,key] of appSource.matchAll(/\bt\('([^']+)'/g)) assert.ok(Object.hasOwn(catalog,key), `Missing translation: ${key}`);
const html=fs.readFileSync(path.join(__dirname, '../ui/index.html'),'utf8');
for (const [,key] of html.matchAll(/data-i18n(?:-[a-z-]+)?="([^"]+)"/g)) assert.ok(Object.hasOwn(catalog,key), `Missing static translation: ${key}`);
const parameters=value=>[...value.matchAll(/\{(\w+)\}/g)].map(m=>m[1]).sort();
for (const [key,value] of Object.entries(catalog)) assert.deepEqual(parameters(value),parameters(key), `Template parameters differ: ${key}`);
// Rust 端訊息同樣以繁中為鍵；漏收錄時英文介面會直接顯示中文，這裡以字串形狀比對目錄。
// 不掃 smoke、測試段、註解，以及只寫到 stderr 或 expect 的開發者訊息。
const shape=value=>value.replace(/\{[^}]*\}/g,'\u0000');
const catalogShapes=new Set(Object.keys(catalog).map(shape));
const rustFiles=dir=>fs.readdirSync(dir,{withFileTypes:true}).flatMap(entry=>entry.isDirectory()?rustFiles(path.join(dir,entry.name)):[path.join(dir,entry.name)]);
for (const file of rustFiles(path.join(__dirname,'../src')).filter(name=>name.endsWith('.rs')&&!name.endsWith('smoke.rs'))) {
  const lines=fs.readFileSync(file,'utf8').split('\n');
  for (const [index,line] of lines.entries()) {
    if (line.includes('#[cfg(test)]')) {
      // 只認檔尾的測試模組；其他位置的 cfg(test) 會讓後面的程式碼漏掃，直接報錯。
      assert.match(lines[index+1]??'', /^\s*mod \w+/, `Unsupported #[cfg(test)] placement: ${file}:${index+1}`);
      break;
    }
    if (/^\s*\/\//.test(line)||line.includes('eprintln!')||line.includes('.expect(')) continue;
    for (const [,literal] of line.matchAll(/"((?:[^"\\]|\\.)*)"/g)) {
      if (/[\u3400-\u9fff]/.test(literal)) assert.ok(catalogShapes.has(shape(literal)), `Missing Rust translation: ${path.relative(path.join(__dirname,'..'),file)}:${index+1} ${literal}`);
    }
  }
}
const state={settings:{pads:[],toggle_shortcut:{control:false,option:false,shift:true,command:true,key:'Space'},width:520,height:null,top_offset:8,side:'right',hot_edge:true,pinned:false},home:true,shortcut_active:true,shortcut_label:'⇧⌘Space',shortcut_error:null};
ctx.window.render(state); ctx.window.showSettings();
assert.equal(ids.get('app-version').textContent,'9.9.9','首頁版本須來自代入的版本字串');
assert.ok(find(ids.get('overlay'),'Open Slide Pad 9.9.9'),'設定頁版本須來自代入的版本字串');
ids.get('shortcut-control').checked=true;
ids.get('shortcut-key').value='F19';
function find(e, label) { if(e.textContent===label)return e; for(const c of e.children){const result=find(c,label);if(result)return result;} }
find(ids.get('shortcut-form'),'Reset to default').onclick();
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
find(ids.get('overlay'),'Use full height').onclick();
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

// 未收到保存成功的狀態之前，select 與畫面都維持目前語言。
assert.equal(ctx.document.documentElement.lang,'en');
assert.equal(ids.get('overlay-title').textContent,'Settings');
assert.equal(ids.get('language-select').value,'en');
ids.get('language-select').value='zh-TW';
ids.get('language-select').onchange();
assert.deepEqual(sent.at(-1),{action:'set_language',language:'zh-TW'});
assert.equal(ids.get('language-select').value,'en');
ctx.window.render(state); // 儲存失敗或尚未回覆。
assert.equal(ctx.document.documentElement.lang,'en');
assert.equal(ids.get('language-select').value,'en');
const chinese=JSON.parse(JSON.stringify(populated));
chinese.settings.language='zh-TW';
ids.get('language-select').focus();
const sentBefore=sent.length;
ctx.window.render(chinese);
assert.equal(sent.length,sentBefore,'語言 render 不應發送其他設定或導覽命令');
assert.equal(ctx.document.documentElement.lang,'zh-TW');
assert.equal(ids.get('language-select').value,'zh-TW');
assert.equal(ctx.document.activeElement,ids.get('language-select'),'切換後保留鍵盤焦點');
assert.equal(ids.get('overlay-title').textContent,'設定');
assert.equal(ids.get('overlay-close').attributes['aria-label'],'關閉');
assert.ok(find(ids.get('overlay'),'恢復預設'));
assert.ok(find(ids.get('quick'),'電子郵件'));
assert.equal(ids.get('saved-sites').children[0].attributes['aria-label'],'開啟 工作筆記');
assert.equal(ids.get('saved-sites').children[1].children[1].children[0].textContent,populated.settings.pads[1].title);
ids.get('language-select').value='en';
ids.get('language-select').onchange();
assert.deepEqual(sent.at(-1),{action:'set_language',language:'en'});
assert.equal(ids.get('language-select').value,'zh-TW');
ctx.window.render(populated); // 英文使用與舊設定相容的省略欄位。
assert.equal(ctx.document.documentElement.lang,'en');
assert.equal(ids.get('overlay-title').textContent,'Settings');
assert.equal(ids.get('overlay-close').attributes['aria-label'],'Close');
assert.ok(find(ids.get('quick'),'Email'));
assert.equal(ids.get('saved-sites').children[0].attributes['aria-label'],'Open 工作筆記');
assert.equal(ids.get('saved-sites').children[0].children[1].children[0].textContent,'工作筆記');
console.log('PASS bilingual settings, IPC, save acknowledgement, focus, catalogs and user text');
