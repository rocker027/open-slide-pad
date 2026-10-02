// 使用最小 DOM 替身驗證設定表單的 reset 契約，不操作原生視窗。
const assert = require('node:assert/strict');
const path = require('node:path');
const fs = require('node:fs');
const vm = require('node:vm');
const ids = new Map();
class Element {
  constructor(tag) { this.tagName=tag; this.children=[]; this.classList={toggle(){}}; this.textContent=''; this.attributes={}; this.dataset={}; this.style={}; }
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
  blur() { if(ctx.document.activeElement===this) ctx.document.activeElement=null; }
}
for(const id of ['pads','home','home-button','site-count','saved-sites','empty-state','suggestions','pin','back','forward','address','status','status-text','undo-remove','status-shortcut','home-shortcut','hero-add','add','settings','address-form','quick','overlay','toast','toast-text','toast-close','resize-grip','app-version','reload','progress','progress-bar','load-error','load-error-title','load-error-detail','load-error-address','load-dismiss','empty-import']) {
 const e=new Element('div'); e.id=id; e.append(new Element('#text'));
}
const sent=[];
const timers=[];
const ctx={URL,window:{ipc:{postMessage(m){sent.push(JSON.parse(m));}}},document:{documentElement:{dataset:{}},createElement:t=>new Element(t),createTextNode:t=>Object.assign(new Element('#text'),{textContent:t}),getElementById:id=>ids.get(id),querySelectorAll:()=>[],addEventListener(){}},setTimeout(callback){timers.push(callback);return timers.length;},clearTimeout(id){if(id)timers[id-1]=null;}};
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

// 右键菜单不切换 active；关闭目标由点击的分页 ID 决定。
const tabState={...state,home:false,settings:{...state.settings,active:7,pads:[
  {id:7,title:'First',url:'https://example.com/'},
  {id:42,title:'Second',url:'https://example.org/'},
]}};
ctx.window.render(tabState);
let menuPrevented=false;
const beforeMenu=sent.length;
ids.get('pads').children[1].oncontextmenu({preventDefault(){menuPrevented=true;}});
assert.ok(menuPrevented,'分頁右鍵應阻止預設網頁選單');
assert.equal(sent.length,beforeMenu+1,'右鍵只開啟選單，不切換或移除分頁');
assert.deepEqual(sent.at(-1),{action:'pad_menu',id:42});
ids.get('pads').children[0].onclick();
assert.deepEqual(sent.at(-1),{action:'select',id:7},'左鍵仍切換分頁');
ctx.window.render(state);
console.log('PASS sidebar context menu targets the clicked tab and preserves left click');

// 设置只在保存回覆后显示新值；休眠状态变化必须独立刷新分页标记。
ctx.window.showSettings();
assert.equal(ids.get('sleep-select').value,'15');
ids.get('sleep-select').value='30';
ids.get('sleep-select').onchange();
assert.deepEqual(sent.at(-1),{action:'set_sleep',minutes:30});
assert.equal(ids.get('sleep-select').value,'15','保存回覆前維持原設定');
ctx.window.render({...state,settings:{...state.settings,sleep_after_minutes:30}});
assert.equal(ids.get('sleep-select').value,'30');
ids.get('sleep-select').value='0';
ids.get('sleep-select').onchange();
assert.deepEqual(sent.at(-1),{action:'set_sleep',minutes:0});
ctx.window.render(tabState);
ctx.window.render({...tabState,sleeping:[42]});
assert.match(ids.get('pads').children[1].className,/sleeping/);
assert.match(ids.get('pads').children[1].title,/Sleeping/);
ctx.window.render({...tabState,sleeping:[]});
assert.doesNotMatch(ids.get('pads').children[1].className,/sleeping/);
ctx.window.render(state);
console.log('PASS sleep setting awaits persistence and tab sleep markers refresh independently');
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

// 錄製快捷鍵只填表單草稿，不直接送出；Esc 只取消錄製，不關閉設定。
const recorder=ids.get('shortcut-recorder');
const keydown=fields=>{const event={key:'',code:'',metaKey:false,altKey:false,ctrlKey:false,shiftKey:false,prevented:false,stopped:false,preventDefault(){this.prevented=true;},stopPropagation(){this.stopped=true;},...fields};recorder.onkeydown(event);return event;};
const sentBeforeRecording=sent.length;
keydown({key:'k',code:'KeyK',metaKey:true,shiftKey:true});
assert.deepEqual(['control','option','shift','command'].map(name=>ids.get(`shortcut-${name}`).checked),[false,false,true,true]);
assert.equal(ids.get('shortcut-key').value,'KeyK');
assert.equal(recorder.value,'⇧⌘K');
keydown({key:'j',code:'KeyJ'});
assert.equal(ids.get('shortcut-key').value,'KeyK','沒有 ⌘、⌥、⌃ 的按鍵不可成為草稿');
keydown({key:'Enter',code:'Enter',metaKey:true});
assert.equal(ids.get('shortcut-key').value,'KeyK','下拉選單沒有的按鍵不可成為草稿');
keydown({key:'Meta',code:'MetaLeft',metaKey:true});
assert.equal(ids.get('shortcut-key').value,'KeyK','只按修飾鍵不算組合');
recorder.focus();
const escape=keydown({key:'Escape',code:'Escape'});
assert.equal(escape.stopped,true,'錄製中的 Esc 不可傳到關閉設定的處理器');
assert.notEqual(ctx.document.activeElement,recorder,'Esc 取消錄製');
const tab=keydown({key:'Tab',code:'Tab'});
assert.equal(tab.prevented,false,'Tab 保留給鍵盤導覽');
assert.equal(sent.length,sentBeforeRecording,'錄製不可直接送出命令');
find(ids.get('shortcut-form'),'Reset to default').onclick();
assert.equal(recorder.value,'','恢復預設也要清掉錄製欄，否則會與表單草稿不符');
console.log('PASS shortcut recorder fills the draft from a key press and keeps Escape local');

// 左右位置：只有點另一側才切換。
assert.equal(ids.get('side-right').attributes['aria-pressed'],'true');
assert.equal(ids.get('side-left').attributes['aria-pressed'],'false');
const sentBeforeSide=sent.length;
ids.get('side-right').onclick();
assert.equal(sent.length,sentBeforeSide,'已在右側時點右側不送命令');
ids.get('side-left').onclick();
assert.deepEqual(sent.at(-1),{action:'side'});
console.log('PASS side control only toggles when the other side is chosen');

// 提示可手動關閉，滑鼠停留時暫停倒數。
ctx.window.showToast('disk full');
assert.equal(ids.get('toast-text').textContent,'disk full');
assert.equal(ids.get('toast').hidden,false);
const armed=timers.filter(Boolean).length;
ids.get('toast').onmouseenter();
assert.equal(timers.filter(Boolean).length,armed-1,'滑鼠停留時取消倒數');
ids.get('toast').onmouseleave();
assert.equal(timers.filter(Boolean).length,armed,'滑鼠離開後重新倒數');
timers.filter(Boolean).at(-1)();
assert.equal(ids.get('toast').hidden,true,'倒數結束後收起');
ctx.window.showToast('again');
ids.get('toast-close').onclick();
assert.equal(ids.get('toast').hidden,true,'關閉鈕立即收起');
console.log('PASS toast can be dismissed and pauses while hovered');

// 首頁使用相同的網站順序與 select IPC，最後一個網站移除後回到空狀態。
const populated=JSON.parse(JSON.stringify(state));
populated.settings.pads=[{id:7,title:'工作筆記',url:'https://www.notion.so/team'},
  {id:12,title:'<img src=x onerror=alert(1)>',url:'https://example.com'}];
ctx.window.render(populated);
assert.equal(ids.get('site-count').textContent,'2');
assert.equal(ids.get('status-text').children.at(-1).textContent,'Home','首頁狀態列不重複網站數');
assert.equal(ids.get('status-shortcut').hidden,true,'首頁說明區已有快捷鍵，狀態列不重複');
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

// 載入中：進度條與「停止」；失敗：錯誤畫面與網址；首頁一律不顯示載入狀態。
const page={...populated,home:false,title:'Docs',address:'https://example.com/docs'};
ctx.window.render({...page,loading:true,progress:0.42});
assert.equal(ids.get('progress').hidden,false);
assert.equal(ids.get('progress-bar').style.width,'42%');
assert.equal(ids.get('progress').attributes['aria-valuenow'],'42');
assert.equal(ids.get('reload').dataset.action,'stop','載入中按鈕改為停止');
assert.equal(ids.get('reload').attributes.title,'Stop loading');
assert.equal(ids.get('reload').attributes['data-i18n-title'],'停止載入','切換語言時仍要翻成停止');
ctx.window.render({...page,loading:true,progress:0});
assert.equal(ids.get('progress-bar').style.width,'8%','尚無進度時仍露出一小段');
ctx.window.render({...page,loading:false,progress:0});
assert.equal(ids.get('progress').hidden,true);
assert.equal(ids.get('reload').dataset.action,'reload');
assert.equal(ids.get('reload').attributes.title,'Reload');
assert.equal(ids.get('load-error').hidden,true);
ctx.window.render({...page,failure:'unreachable',address:'http://127.0.0.1:1/'});
assert.equal(ids.get('load-error').hidden,false);
assert.equal(ids.get('load-error-title').textContent,'This page could not be loaded');
assert.equal(ids.get('load-error-address').textContent,'http://127.0.0.1:1/');
assert.equal(ids.get('load-dismiss').hidden,true,'沒有原頁面可回時不顯示退路');
ctx.window.render({...page,failure:'unreachable',dismissible:true});
assert.equal(ids.get('load-dismiss').hidden,false,'原頁面還在時可以回去');
assert.equal(ids.get('status-text').children.at(-1).textContent,'Could not load');
ctx.window.render({...page,failure:'crashed'});
assert.equal(ids.get('load-error-title').textContent,'This page stopped unexpectedly');
ctx.window.render({...page,dismissible:true});
assert.equal(ids.get('load-dismiss').hidden,true,'沒有失敗就不顯示退路');
ctx.window.render({...page,home:true,loading:true,failure:'unreachable'});
assert.equal(ids.get('load-error').hidden,true,'首頁不顯示網站的失敗畫面');
assert.equal(ids.get('progress').hidden,true,'首頁不顯示網站的載入進度');
assert.equal(ids.get('reload').dataset.action,'reload');
console.log('PASS load state drives the progress bar, stop control and failure screen');

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

// 匯入書籤：入口送 show_import；挑選畫面只送索引、尊重上限、已加入者停用、篩選只藏不刪。
ids.get('empty-import').onclick();
assert.deepEqual(sent.at(-1),{action:'show_import'});
ids.get('import-bookmarks').onclick();
assert.deepEqual(sent.at(-1),{action:'show_import'});
ctx.window.showImport({entries:[],remaining:18});
assert.ok(ids.get('import-empty'),'沒有書籤時顯示空狀態');
assert.equal(ids.get('overlay-title').textContent,'Import Chrome bookmarks');
const entries=[{title:'Docs',url:'https://docs.example.com/a',added:false},{title:'工作筆記',url:'https://www.notion.so/team',added:true},{title:'<b>x</b>',url:'https://x.example.com/',added:false},{title:'Mail',url:'https://mail.example.com/',added:false}];
ctx.window.showImport({entries,remaining:2});
const boxes=ids.get('import-list').children.map(row=>row.children[0]);
assert.equal(boxes.length,4);
assert.equal(boxes[1].disabled,true,'已加入的書籤不可再勾');
assert.equal(ids.get('import-list').children[1].children[2].textContent,'Added');
assert.equal(ids.get('import-list').children[2].children[1].children[0].textContent,'<b>x</b>','名稱以文字節點呈現');
assert.equal(ids.get('import-list').children[0].children[1].children[1].textContent,'docs.example.com');
assert.equal(ids.get('import-submit').disabled,true,'未勾選時不可送出');
assert.equal(ids.get('import-count').textContent,'0 selected · 2 more can be added');
boxes[3].checked=true; boxes[3].onchange();
boxes[0].checked=true; boxes[0].onchange();
assert.equal(ids.get('import-count').textContent,'2 selected · 0 more can be added');
assert.equal(boxes[2].disabled,true,'到達上限後其餘項目停用');
assert.equal(boxes[0].disabled,false,'已勾選的項目仍可取消');
ids.get('import-filter').value='MAIL';
ids.get('import-filter').oninput();
assert.deepEqual(ids.get('import-list').children.map(row=>row.hidden),[true,true,true,false],'篩選不分大小寫、比對名稱與網址');
ids.get('import-filter').value='notion.so';
ids.get('import-filter').oninput();
assert.deepEqual(ids.get('import-list').children.map(row=>row.hidden),[true,false,true,true]);
ids.get('import-form').onsubmit({preventDefault(){}});
assert.deepEqual(sent.at(-1),{action:'import',indices:[0,3]},'只送索引，且藏起來的勾選仍算數');
boxes[0].checked=false; boxes[0].onchange();
assert.equal(boxes[2].disabled,false,'取消勾選後釋出名額');
ctx.window.showImport({entries,remaining:0});
assert.equal(ids.get('import-count').textContent,'The sidebar is full. Remove a site before importing.');
assert.ok(ids.get('import-list').children.every(row=>row.children[0].disabled),'側欄已滿時全部停用');
assert.equal(ids.get('import-submit').disabled,true);
console.log('PASS bookmark import picker sends indices, enforces the limit and filters by name or URL');
