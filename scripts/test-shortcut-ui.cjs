// 驗證前端互動與 IPC 契約，不操作原生視窗。
const assert = require('node:assert/strict');
const path = require('node:path');
const fs = require('node:fs');
const vm = require('node:vm');
// 僅模擬本頁使用的 DOM 契約；由真實 HTML 建樹，驗證事件、焦點與可見性，不模擬排版或原生 WebView。
const html=fs.readFileSync(path.join(__dirname, '../ui/index.html'),'utf8');
const ids = new Map();
const sent=[];
const timers=[];
const timerDelays=[];
function runTimers(delay) {
  timers.forEach((callback,index)=>{
    if(callback && timerDelays[index]===delay) { timers[index]=null; callback(); }
  });
}
const listeners=new Map();
let document;
function event(type, fields={}) {
  return {type,bubbles:true,defaultPrevented:false,prevented:false,stopped:false,
    preventDefault(){this.defaultPrevented=this.prevented=true;},stopPropagation(){this.stopped=true;},...fields};
}
function matches(element, selector) {
  selector=selector.trim();
  if(selector===':hover')return Boolean(element._hovered);
  if (selector.includes(',')) return selector.split(',').some(part=>matches(element,part));
  const excluded=[...selector.matchAll(/:not\(([^)]+)\)/g)].map(match=>match[1]);
  selector=selector.replace(/:not\([^)]+\)/g,'');
  if (excluded.some(part=>matches(element,part))) return false;
  if (selector.includes(':disabled')) {
    if (!element.disabled) return false;
    selector=selector.replace(':disabled','');
  }
  if (selector.includes(':hidden')) {
    if (element.getClientRects().length) return false;
    selector=selector.replace(':hidden','');
  }
  const attributes=[...selector.matchAll(/\[([^=\]]+)(?:=["']?([^"'\]]*)["']?)?\]/g)];
  selector=selector.replace(/\[[^\]]*\]/g,'');
  for (const [,key,value] of attributes) {
    if (!element.hasAttribute(key)) return false;
    if (value!==undefined && element.getAttribute(key)!==value) return false;
  }
  const id=selector.match(/#([\w-]+)/)?.[1];
  if (id && element.id!==id) return false;
  const classes=[...selector.matchAll(/\.([\w-]+)/g)].map(match=>match[1]);
  if (classes.some(name=>!element.classList.contains(name))) return false;
  const tag=selector.match(/^[\w-]+/)?.[0];
  return !tag || element.tagName===tag.toUpperCase();
}
class Element {
  constructor(tag) {
    this.tagName=tag.toUpperCase(); this.nodeType=tag==='#text'?3:1; this.childNodes=[];
    this.attributes={}; this.dataset={}; this.style={}; this._listeners=new Map();
    this.className=''; this.hidden=false; this.disabled=false; this.inert=false; this.checked=false; this.open=false;
    this.value=''; this.scrollTop=0; this.selectionStart=0; this.selectionEnd=0;
    this._tabIndex=undefined;
    this.classList={contains:name=>this.className.split(/\s+/).includes(name),toggle:(name,force)=>{
      const active=force??!this.classList.contains(name);
      this.className=[...new Set(this.className.split(/\s+/).filter(item=>item && item!==name).concat(active?[name]:[]))].join(' ');
      return active;
    },add:name=>this.classList.toggle(name,true),remove:name=>this.classList.toggle(name,false)};
  }
  set tabIndex(value) { this._tabIndex=Number(value); this.attributes.tabindex=String(value); }
  get tabIndex() { return this._tabIndex??(['BUTTON','INPUT','SELECT','TEXTAREA','SUMMARY'].includes(this.tagName)?0:-1); }
  set id(id) { this._id=id; ids.set(id,this); this.attributes.id=id; }
  get id() { return this._id||''; }
  get children() { return this.childNodes.filter(child=>child.nodeType===1); }
  get parentNode() { return this.parent||null; }
  get parentElement() { return this.parentNode; }
  get isConnected() { return this===document?.body || Boolean(this.parent?.isConnected); }
  set textContent(value) { this.replaceChildren(); this._text=String(value); }
  get textContent() { return (this._text||'')+this.childNodes.map(child=>child.textContent).join(''); }
  append(...items) { for(let item of items) { if(typeof item==='string') item=Object.assign(new Element('#text'),{textContent:item}); if(item.parent)item.remove(); item.parent=this; this.childNodes.push(item); } }
  prepend(...items) { for(const item of items.reverse()) { if(item.parent)item.remove(); item.parent=this; this.childNodes.unshift(item); } }
  replaceChildren(...items) { for(const child of [...this.childNodes])child.remove(); this._text=''; this.append(...items); }
  get lastChild() { return this.childNodes.at(-1)||null; }
  get firstChild() { return this.childNodes[0]||null; }
  setAttribute(name, value) {
    value=String(value); this.attributes[name]=value;
    if(name==='id')this.id=value;
    if(name==='class')this.className=value;
    if(['hidden','disabled','inert','open'].includes(name))this[name]=true;
    if(name==='tabindex')this.tabIndex=Number(value);
    if(name.startsWith('data-'))this.dataset[name.slice(5).replace(/-([a-z])/g,(_,letter)=>letter.toUpperCase())]=value;
  }
  getAttribute(name) {
    if(['hidden','disabled','inert','open'].includes(name))return this[name]?'':null;
    if(name==='tabindex' && this._tabIndex!==undefined)return String(this._tabIndex);
    return this.attributes[name]??null;
  }
  hasAttribute(name) { return this.getAttribute(name)!==null; }
  removeAttribute(name) { delete this.attributes[name]; if(['hidden','disabled','inert','open'].includes(name))this[name]=false; }
  contains(other) { return this===other || this.childNodes.some(child=>child.contains(other)); }
  matches(selector) { return matches(this,selector); }
  closest(selector) { return this.matches(selector)?this:this.parent?.closest(selector)||null; }
  querySelectorAll(selector) { return this.children.flatMap(child=>[...(child.matches(selector)?[child]:[]),...child.querySelectorAll(selector)]); }
  querySelector(selector) { return this.querySelectorAll(selector)[0]||null; }
  getClientRects() {
    if(!this.isConnected || this.closest('[hidden]'))return [];
    for(let ancestor=this.parent;ancestor;ancestor=ancestor.parent) {
      if(ancestor.tagName==='DETAILS'&&!ancestor.open) {
        const summary=ancestor.children.find(child=>child.tagName==='SUMMARY');
        if(!summary?.contains(this))return [];
      }
    }
    return [{}];
  }
  remove() {
    if(this.contains(document?.activeElement))document.activeElement=document.body;
    if(this.parent)this.parent.childNodes.splice(this.parent.childNodes.indexOf(this),1);
    this.parent=null;
  }
  addEventListener(type, handler) { if(!this._listeners.has(type))this._listeners.set(type,[]); this._listeners.get(type).push(handler); }
  dispatchEvent(value) {
    if(!value.target)value.target=this;
    if(value.type==='mouseenter')this._hovered=true;
    if(value.type==='mouseleave')this._hovered=false;
    value.currentTarget=this;
    this[`on${value.type}`]?.(value);
    for(const handler of this._listeners.get(value.type)||[])handler(value);
    if(value.bubbles&&!value.stopped) {
      if(this.parent)this.parent.dispatchEvent(value);
      else document.dispatchEvent(value);
    }
    return !value.defaultPrevented;
  }
  focus() {
    if(this.disabled || !this.getClientRects().length || this.closest('[inert]'))return;
    const previous=document.activeElement;
    if(previous===this)return;
    document.activeElement=this;
    previous?.dispatchEvent(event('blur',{bubbles:false,relatedTarget:this}));
    previous?.dispatchEvent(event('focusout',{relatedTarget:this}));
    this.dispatchEvent(event('focus',{bubbles:false,relatedTarget:previous}));
    this.dispatchEvent(event('focusin',{relatedTarget:previous}));
  }
  select() { this.selectionStart=0; this.selectionEnd=this.value.length; }
  setSelectionRange(start,end) { this.selectionStart=start; this.selectionEnd=end; }
  blur() { if(document.activeElement===this)document.body.focus(); }
  click() {
    if(this.disabled || !this.getClientRects().length || this.closest('[inert]'))return;
    const proceed=this.dispatchEvent(event('click'));
    if(proceed && this.tagName==='SUMMARY' && this.parent?.tagName==='DETAILS')this.parent.open=!this.parent.open;
  }
}
document={documentElement:{dataset:{}},createElement:tag=>new Element(tag),createTextNode:value=>Object.assign(new Element('#text'),{textContent:value}),
  getElementById:id=>{const element=ids.get(id);return element?.isConnected?element:null;},
  querySelectorAll:selector=>document.body.querySelectorAll(selector),querySelector:selector=>document.body.querySelector(selector),
  addEventListener(type,handler){if(!listeners.has(type))listeners.set(type,[]);listeners.get(type).push(handler);},
  dispatchEvent(value){for(const handler of listeners.get(value.type)||[])handler(value);return !value.defaultPrevented;}};
document.body=new Element('body'); document.activeElement=document.body;
const stack=[document.body];
const body=html.match(/<body[^>]*>([\s\S]*?)<\/body>/i)[1].replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi,'');
for(const token of body.match(/<!--[^]*?-->|<[^>]+>|[^<]+/g)||[]) {
  if(token.startsWith('<!--'))continue;
  if(token.startsWith('</')) { stack.pop(); continue; }
  if(token.startsWith('<')) {
    const tag=token.match(/^<([\w-]+)/)?.[1]; if(!tag)continue;
    const element=new Element(tag);
    for(const [,name,value] of token.slice(tag.length+1,-1).matchAll(/([\w:-]+)(?:="([^"]*)")?/g))element.setAttribute(name,value??'');
    stack.at(-1).append(element);
    if(!token.endsWith('/>')&&!['input','br','hr','img','meta','link'].includes(tag))stack.push(element);
  } else if(token.trim())stack.at(-1).append(document.createTextNode(token));
}
const ctx={URL,window:{ipc:{postMessage(message){sent.push(JSON.parse(message));}}},document,
  setTimeout(callback,delay){timers.push(callback);timerDelays.push(delay);return timers.length;},clearTimeout(id){if(id)timers[id-1]=null;}};
vm.createContext(ctx);
const sourcePath = process.argv[2] || path.join(__dirname, '../ui/app.js');
const catalog=JSON.parse(fs.readFileSync(path.join(__dirname, '../ui/locales/en.json'), 'utf8'));
const i18nSource=fs.readFileSync(path.join(__dirname, '../ui/i18n.js'), 'utf8').replace('/* SLIDERUST_ENGLISH */', JSON.stringify(catalog));
vm.runInContext(i18nSource,ctx);
const appSource=fs.readFileSync(sourcePath, 'utf8').replace('/* SLIDERUST_VERSION */', '9.9.9');
vm.runInContext(appSource, ctx);
for (const [,key] of appSource.matchAll(/\bt\('([^']+)'/g)) assert.ok(Object.hasOwn(catalog,key), `Missing translation: ${key}`);
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
function settingsTab(name) { document.getElementById(`settings-tab-${name}`).click(); }
settingsTab('shortcuts');
assert.equal(document.getElementById('app-version').textContent,'9.9.9','首頁版本須來自代入的版本字串');
assert.ok(find(document.getElementById('overlay'),'Open Slide Pad 9.9.9'),'設定頁版本須來自代入的版本字串');
document.getElementById('shortcut-control').checked=true;
document.getElementById('shortcut-key').value='F19';
function find(e, label) { if(e.textContent===label)return e; for(const c of e.children){const result=find(c,label);if(result)return result;} }
find(document.getElementById('shortcut-form'),'Reset to default').onclick();
ctx.window.render(JSON.parse(JSON.stringify(state)));
assert.deepEqual(sent.at(-1), {action:'set_shortcut', shortcut:{control:false, option:false, shift:true, command:true, key:'Space'}});
assert.equal(document.getElementById('shortcut-control').checked, false, '恢復預設必須清除尚未套用的修飾鍵草稿');
assert.equal(document.getElementById('shortcut-key').value, 'Space', '恢復預設必須清除尚未套用的按鍵草稿');
console.log('PASS shortcut reset clears unsaved draft and submits the default IPC');

// 右键菜单不切换 active；关闭目标由点击的分页 ID 决定。
const tabState={...state,home:false,settings:{...state.settings,active:7,pads:[
  {id:7,title:'First',url:'https://example.com/'},
  {id:42,title:'Second',url:'https://example.org/'},
]}};
ctx.window.render(tabState);
let menuPrevented=false;
const beforeMenu=sent.length;
document.getElementById('pads').children[1].oncontextmenu({preventDefault(){menuPrevented=true;}});
assert.ok(menuPrevented,'分頁右鍵應阻止預設網頁選單');
assert.equal(sent.length,beforeMenu+1,'右鍵只開啟選單，不切換或移除分頁');
assert.deepEqual(sent.at(-1),{action:'pad_menu',id:42});
document.getElementById('pads').children[0].onclick();
assert.deepEqual(sent.at(-1),{action:'select',id:7},'左鍵仍切換分頁');
ctx.window.render(state);
console.log('PASS sidebar context menu targets the clicked tab and preserves left click');

// 设置只在保存回覆后显示新值；休眠状态变化必须独立刷新分页标记。
ctx.window.showSettings();
settingsTab('general');
document.getElementById('more-settings-toggle').click();
assert.equal(document.getElementById('sleep-select').value,'15');
document.getElementById('sleep-select').value='30';
document.getElementById('sleep-select').onchange();
assert.deepEqual(sent.at(-1),{action:'set_sleep',minutes:30});
assert.equal(document.getElementById('sleep-select').value,'15','保存回覆前維持原設定');
ctx.window.render({...state,settings:{...state.settings,sleep_after_minutes:30}});
assert.equal(document.getElementById('sleep-select').value,'30');
document.getElementById('sleep-select').value='0';
document.getElementById('sleep-select').onchange();
assert.deepEqual(sent.at(-1),{action:'set_sleep',minutes:0});
ctx.window.render(tabState);
ctx.window.render({...tabState,sleeping:[42]});
assert.match(document.getElementById('pads').children[1].className,/sleeping/);
assert.match(document.getElementById('pads').children[1].title,/Sleeping/);
ctx.window.render({...tabState,sleeping:[]});
assert.doesNotMatch(document.getElementById('pads').children[1].className,/sleeping/);
ctx.window.render(state);
console.log('PASS sleep setting awaits persistence and tab sleep markers refresh independently');

// 收起休眠沿用已保存設定，開關與焦點都等保存回覆才更新。
for (const selectedLanguage of ['en','zh-TW']) {
  const label = selectedLanguage === 'en' ? 'Sleep when hidden' : '收起後也休眠';
  const prerequisite = selectedLanguage === 'en'
    ? 'Choose a sleep duration first to change this setting.' : '請先選擇休眠時間，才能調整此設定。';
  const description = selectedLanguage === 'en'
    ? 'When the sidebar is hidden, the current site also sleeps after the duration above. Reopening the sidebar reloads the page. Playback, live connections and unsaved input may not be preserved. Cookies are kept.'
    : '側欄收起後，目前網站也會依上方時間休眠。重新開啟側欄時會重新載入。播放、即時連線與未提交的輸入可能無法保留，cookies 會保留。';
  let hiddenSleepState = {...state,settings:{...state.settings,language:selectedLanguage}};
  ctx.window.render(hiddenSleepState);
  ctx.window.showSettings('general');
  if (!document.getElementById('more-settings').open) document.getElementById('more-settings-toggle').click();
  const sleepToggle = () => document.getElementById('setting-set_sleep_when_hidden');
  assert.equal(sleepToggle().getAttribute('role'),'switch');
  assert.equal(sleepToggle().getAttribute('aria-label'),label);
  assert.equal(sleepToggle().getAttribute('aria-checked'),'false','舊設定缺欄位時預設關閉');
  assert.equal(sleepToggle().disabled,false,'預設 15 分鐘可調整收起休眠');
  assert.ok(find(document.getElementById('more-settings'),description));
  sleepToggle().focus();
  sleepToggle().click();
  assert.deepEqual(sent.at(-1),{action:'set_sleep_when_hidden',enabled:true});
  assert.equal(sleepToggle().getAttribute('aria-checked'),'false','等待保存回覆時不可樂觀開啟');
  ctx.window.showToast(selectedLanguage === 'en' ? 'Could not save settings' : '無法保存設定');
  ctx.window.render(hiddenSleepState);
  assert.equal(sleepToggle().getAttribute('aria-checked'),'false','保存失敗保留原設定');
  assert.equal(document.activeElement,sleepToggle(),'保存失敗保留開關焦點');
  const beforeHiddenSleepAck = sent.length;
  hiddenSleepState = {...hiddenSleepState,settings:{...hiddenSleepState.settings,sleep_when_hidden:true}};
  ctx.window.render(hiddenSleepState);
  assert.equal(sent.length,beforeHiddenSleepAck,'保存回覆不應再送出設定命令');
  assert.equal(sleepToggle().getAttribute('aria-checked'),'true');
  assert.equal(document.activeElement,sleepToggle(),'保存成功後保留開關焦點');
  assert.equal(document.getElementById('more-settings').open,true,'保存回覆後保留展開狀態');
  sleepToggle().click();
  assert.deepEqual(sent.at(-1),{action:'set_sleep_when_hidden',enabled:false});
  assert.equal(sleepToggle().getAttribute('aria-checked'),'true','等待關閉回覆時仍顯示原設定');
  ctx.window.render(hiddenSleepState);
  assert.equal(sleepToggle().getAttribute('aria-checked'),'true','關閉保存失敗仍保留開啟狀態');
  assert.equal(document.activeElement,sleepToggle());
  hiddenSleepState = {...hiddenSleepState,settings:{...hiddenSleepState.settings,sleep_when_hidden:false}};
  ctx.window.render(hiddenSleepState);
  assert.equal(sleepToggle().getAttribute('aria-checked'),'false');
  assert.equal(document.activeElement,sleepToggle());

  for (const savedPreference of [false,true]) {
    hiddenSleepState = {...hiddenSleepState,settings:{...hiddenSleepState.settings,sleep_when_hidden:savedPreference,sleep_after_minutes:0}};
    ctx.window.render(hiddenSleepState);
    assert.equal(sleepToggle().getAttribute('aria-checked'),String(savedPreference),'停用時間仍顯示已保存偏好');
    assert.equal(sleepToggle().disabled,true);
    assert.ok(find(document.getElementById('more-settings'),`${prerequisite} ${description}`));
    const beforeDisabledClick = sent.length;
    sleepToggle().click();
    assert.equal(sent.length,beforeDisabledClick,'停用的開關不可送出設定命令');
  }
  assert.equal(document.activeElement.id,'settings-tab-general','原焦點控制停用時回到目前分區標籤');
  const duration = document.getElementById('sleep-select');
  duration.focus();
  duration.value='30';
  duration.onchange();
  assert.deepEqual(sent.at(-1),{action:'set_sleep',minutes:30},'仍使用原有休眠時間命令');
  assert.equal(duration.value,'0','休眠時間也等待保存回覆');
  assert.equal(sleepToggle().disabled,true,'選擇時間尚未保存前開關仍停用');
  hiddenSleepState = {...hiddenSleepState,settings:{...hiddenSleepState.settings,sleep_after_minutes:30}};
  ctx.window.render(hiddenSleepState);
  assert.equal(sleepToggle().disabled,false);
  assert.equal(sleepToggle().getAttribute('aria-checked'),'true','恢復時間後沿用已保存偏好');
  assert.equal(document.getElementById('sleep-select').value,'30');
  assert.equal(document.activeElement.id,'sleep-select','休眠時間保存後維持原控制焦點');
}
ctx.window.render(state);
console.log('PASS hidden sleep bilingual copy, explicit IPC, persistence acknowledgement, focus and disabled duration');
const before = sent.length;
document.getElementById('resize-grip').onpointerdown({button:2});
assert.equal(sent.length, before, '右鍵不可開始調整大小');
let prevented=false;
document.getElementById('resize-grip').onpointerdown({button:0,preventDefault(){prevented=true;}});
assert.equal(prevented,true);
assert.deepEqual(sent.at(-1),{action:'begin_resize'});
assert.equal(ctx.document.documentElement.dataset.side,'right');
find(document.getElementById('overlay'),'Use full height').onclick();
assert.deepEqual(sent.at(-1),{action:'full_height'});
console.log('PASS resize grip routes primary pointer and full-height commands');

// 錄製快捷鍵只填表單草稿，不直接送出；Esc 只取消錄製，不關閉設定。
settingsTab('shortcuts');
const recorder=document.getElementById('shortcut-recorder');
const keydown=fields=>{const event={key:'',code:'',metaKey:false,altKey:false,ctrlKey:false,shiftKey:false,prevented:false,stopped:false,preventDefault(){this.prevented=true;},stopPropagation(){this.stopped=true;},...fields};recorder.onkeydown(event);return event;};
const sentBeforeRecording=sent.length;
keydown({key:'k',code:'KeyK',metaKey:true,shiftKey:true});
assert.deepEqual(['control','option','shift','command'].map(name=>document.getElementById(`shortcut-${name}`).checked),[false,false,true,true]);
assert.equal(document.getElementById('shortcut-key').value,'KeyK');
assert.equal(recorder.value,'⇧⌘K');
keydown({key:'j',code:'KeyJ'});
assert.equal(document.getElementById('shortcut-key').value,'KeyK','沒有 ⌘、⌥、⌃ 的按鍵不可成為草稿');
keydown({key:'Enter',code:'Enter',metaKey:true});
assert.equal(document.getElementById('shortcut-key').value,'KeyK','下拉選單沒有的按鍵不可成為草稿');
keydown({key:'Meta',code:'MetaLeft',metaKey:true});
assert.equal(document.getElementById('shortcut-key').value,'KeyK','只按修飾鍵不算組合');
recorder.focus();
const escape=keydown({key:'Escape',code:'Escape'});
assert.equal(escape.stopped,true,'錄製中的 Esc 不可傳到關閉設定的處理器');
assert.notEqual(ctx.document.activeElement,recorder,'Esc 取消錄製');
const tab=keydown({key:'Tab',code:'Tab'});
assert.equal(tab.prevented,false,'Tab 保留給鍵盤導覽');
assert.equal(sent.length,sentBeforeRecording,'錄製不可直接送出命令');
find(document.getElementById('shortcut-form'),'Reset to default').onclick();
assert.equal(recorder.value,'','恢復預設也要清掉錄製欄，否則會與表單草稿不符');
console.log('PASS shortcut recorder fills the draft from a key press and keeps Escape local');

// 左右位置：只有點另一側才切換。
settingsTab('general');
assert.equal(document.getElementById('side-right').attributes['aria-pressed'],'true');
assert.equal(document.getElementById('side-left').attributes['aria-pressed'],'false');
const sentBeforeSide=sent.length;
document.getElementById('side-right').onclick();
assert.equal(sent.length,sentBeforeSide,'已在右側時點右側不送命令');
document.getElementById('side-left').onclick();
assert.deepEqual(sent.at(-1),{action:'side'});
console.log('PASS side control only toggles when the other side is chosen');

// 提示可手動關閉，滑鼠停留時暫停倒數。
ctx.window.showToast('disk full');
assert.equal(document.getElementById('toast-text').textContent,'disk full');
assert.equal(document.getElementById('toast').hidden,false);
const armed=timers.filter(Boolean).length;
document.getElementById('toast').dispatchEvent(event('mouseenter',{bubbles:false}));
assert.equal(timers.filter(Boolean).length,armed-1,'滑鼠停留時取消倒數');
document.getElementById('toast').dispatchEvent(event('mouseleave',{bubbles:false}));
assert.equal(timers.filter(Boolean).length,armed,'滑鼠離開後重新倒數');
timers.filter(Boolean).at(-1)();
assert.equal(document.getElementById('toast').hidden,true,'倒數結束後收起');
ctx.window.showToast('again');
document.getElementById('toast-close').onclick();
assert.equal(document.getElementById('toast').hidden,true,'關閉鈕立即收起');
console.log('PASS toast can be dismissed and pauses while hovered');

// 首頁使用相同的網站順序與 select IPC，最後一個網站移除後回到空狀態。
const populated=JSON.parse(JSON.stringify(state));
populated.settings.pads=[{id:7,title:'工作筆記',url:'https://www.notion.so/team'},
  {id:12,title:'<img src=x onerror=alert(1)>',url:'https://example.com'}];
ctx.window.render(populated);
assert.equal(document.getElementById('site-count').textContent,'2');
assert.equal(document.getElementById('status-text').textContent,'Home','首頁狀態列不重複網站數');
assert.equal(document.getElementById('status-shortcut').hidden,true,'首頁說明區已有快捷鍵，狀態列不重複');
assert.equal(document.getElementById('empty-state').hidden,true);
assert.equal(document.getElementById('suggestions').open,false);
const sites=document.getElementById('saved-sites').children;
assert.equal(sites.length,2);
assert.equal(sites[0].children[1].children[1].textContent,'notion.so');
assert.equal(sites[1].children[1].children[0].textContent,populated.settings.pads[1].title);
sites[1].onclick();
assert.deepEqual(sent.at(-1),{action:'select',id:12});
document.getElementById('suggestions').open=true;
ctx.window.render({...populated,loading:true});
assert.equal(document.getElementById('suggestions').open,true,'載入狀態更新不應收起使用者開啟的建議');
ctx.window.render(state);
assert.equal(document.getElementById('empty-state').hidden,false);
assert.equal(document.getElementById('saved-sites').children.length,0);
assert.equal(document.getElementById('suggestions').open,true);
console.log('PASS home list selects saved sites, preserves text and restores empty state');

// 載入中：進度條與「停止」；失敗：錯誤畫面與網址；首頁一律不顯示載入狀態。
const page={...populated,home:false,title:'Docs',address:'https://example.com/docs'};
ctx.window.render({...page,loading:true,progress:0.42});
assert.equal(document.getElementById('progress').hidden,false);
assert.equal(document.getElementById('progress-bar').style.width,'42%');
assert.equal(document.getElementById('progress').attributes['aria-valuenow'],'42');
assert.equal(document.getElementById('reload').dataset.action,'stop','載入中按鈕改為停止');
assert.equal(document.getElementById('reload').attributes.title,'Stop loading');
assert.equal(document.getElementById('reload').attributes['data-i18n-title'],'停止載入','切換語言時仍要翻成停止');
ctx.window.render({...page,loading:true,progress:0});
assert.equal(document.getElementById('progress-bar').style.width,'8%','尚無進度時仍露出一小段');
ctx.window.render({...page,loading:false,progress:0});
assert.equal(document.getElementById('progress').hidden,true);
assert.equal(document.getElementById('reload').dataset.action,'reload');
assert.equal(document.getElementById('reload').attributes.title,'Reload');
assert.equal(document.getElementById('load-error').hidden,true);
ctx.window.render({...page,failure:'unreachable',address:'http://127.0.0.1:1/'});
assert.equal(document.getElementById('load-error').hidden,false);
assert.equal(document.getElementById('load-error-title').textContent,'This page could not be loaded');
assert.equal(document.getElementById('load-error-address').textContent,'http://127.0.0.1:1/');
assert.equal(document.getElementById('load-dismiss').hidden,true,'沒有原頁面可回時不顯示退路');
ctx.window.render({...page,failure:'unreachable',dismissible:true});
assert.equal(document.getElementById('load-dismiss').hidden,false,'原頁面還在時可以回去');
assert.equal(document.getElementById('status-text').textContent,'Could not load');
ctx.window.render({...page,failure:'crashed'});
assert.equal(document.getElementById('load-error-title').textContent,'This page stopped unexpectedly');
ctx.window.render({...page,dismissible:true});
assert.equal(document.getElementById('load-dismiss').hidden,true,'沒有失敗就不顯示退路');
ctx.window.render({...page,home:true,loading:true,failure:'unreachable'});
assert.equal(document.getElementById('load-error').hidden,true,'首頁不顯示網站的失敗畫面');
assert.equal(document.getElementById('progress').hidden,true,'首頁不顯示網站的載入進度');
assert.equal(document.getElementById('reload').dataset.action,'reload');
console.log('PASS load state drives the progress bar, stop control and failure screen');

// 未收到保存成功的狀態之前，select 與畫面都維持目前語言。
assert.equal(ctx.document.documentElement.lang,'en');
assert.equal(document.getElementById('overlay-title').textContent,'Settings');
assert.equal(document.getElementById('language-select').value,'en');
document.getElementById('language-select').value='zh-TW';
document.getElementById('language-select').onchange();
assert.deepEqual(sent.at(-1),{action:'set_language',language:'zh-TW'});
assert.equal(document.getElementById('language-select').value,'en');
ctx.window.render(state); // 儲存失敗或尚未回覆。
assert.equal(ctx.document.documentElement.lang,'en');
assert.equal(document.getElementById('language-select').value,'en');
const chinese=JSON.parse(JSON.stringify(populated));
chinese.settings.language='zh-TW';
document.getElementById('language-select').focus();
const sentBefore=sent.length;
ctx.window.render(chinese);
assert.equal(sent.length,sentBefore,'語言 render 不應發送其他設定或導覽命令');
assert.equal(ctx.document.documentElement.lang,'zh-TW');
assert.equal(document.getElementById('language-select').value,'zh-TW');
assert.equal(ctx.document.activeElement,document.getElementById('language-select'),'切換後保留鍵盤焦點');
assert.equal(document.getElementById('overlay-title').textContent,'設定');
assert.equal(document.getElementById('overlay-close').attributes['aria-label'],'關閉');
assert.ok(find(document.getElementById('overlay'),'恢復預設'));
assert.ok(find(document.getElementById('quick'),'電子郵件'));
assert.equal(document.getElementById('saved-sites').children[0].attributes['aria-label'],'開啟 工作筆記');
assert.equal(document.getElementById('saved-sites').children[1].children[1].children[0].textContent,populated.settings.pads[1].title);
document.getElementById('language-select').value='en';
document.getElementById('language-select').onchange();
assert.deepEqual(sent.at(-1),{action:'set_language',language:'en'});
assert.equal(document.getElementById('language-select').value,'zh-TW');
ctx.window.render(populated); // 英文使用與舊設定相容的省略欄位。
assert.equal(ctx.document.documentElement.lang,'en');
assert.equal(document.getElementById('overlay-title').textContent,'Settings');
assert.equal(document.getElementById('overlay-close').attributes['aria-label'],'Close');
assert.ok(find(document.getElementById('quick'),'Email'));
assert.equal(document.getElementById('saved-sites').children[0].attributes['aria-label'],'Open 工作筆記');
assert.equal(document.getElementById('saved-sites').children[0].children[1].children[0].textContent,'工作筆記');
console.log('PASS bilingual settings, IPC, save acknowledgement, focus, catalogs and user text');

// 匯入書籤：入口送 show_import；挑選畫面只送索引、尊重上限、已加入者停用、篩選只藏不刪。
document.getElementById('empty-import').onclick();
assert.deepEqual(sent.at(-1),{action:'show_import'});
settingsTab('sites');
document.getElementById('import-bookmarks').click();
assert.deepEqual(sent.at(-1),{action:'show_import'});
ctx.window.showImport({entries:[],remaining:18});
assert.ok(document.getElementById('import-empty'),'沒有書籤時顯示空狀態');
assert.equal(document.getElementById('overlay-title').textContent,'Import Chrome bookmarks');
const entries=[{title:'Docs',url:'https://docs.example.com/a',added:false},{title:'工作筆記',url:'https://www.notion.so/team',added:true},{title:'<b>x</b>',url:'https://x.example.com/',added:false},{title:'Mail',url:'https://mail.example.com/',added:false}];
ctx.window.showImport({entries,remaining:2});
const boxes=document.getElementById('import-list').children.map(row=>row.children[0]);
assert.equal(boxes.length,4);
assert.equal(boxes[1].disabled,true,'已加入的書籤不可再勾');
assert.equal(document.getElementById('import-list').children[1].children[2].textContent,'Added');
assert.equal(document.getElementById('import-list').children[2].children[1].children[0].textContent,'<b>x</b>','名稱以文字節點呈現');
assert.equal(document.getElementById('import-list').children[0].children[1].children[1].textContent,'docs.example.com');
assert.equal(document.getElementById('import-submit').disabled,true,'未勾選時不可送出');
assert.equal(document.getElementById('import-count').textContent,'0 selected · 2 more can be added');
boxes[3].checked=true; boxes[3].onchange();
boxes[0].checked=true; boxes[0].onchange();
assert.equal(document.getElementById('import-count').textContent,'2 selected · 0 more can be added');
assert.equal(boxes[2].disabled,true,'到達上限後其餘項目停用');
assert.equal(boxes[0].disabled,false,'已勾選的項目仍可取消');
document.getElementById('import-filter').value='MAIL';
document.getElementById('import-filter').oninput();
assert.deepEqual(document.getElementById('import-list').children.map(row=>row.hidden),[true,true,true,false],'篩選不分大小寫、比對名稱與網址');
document.getElementById('import-filter').value='notion.so';
document.getElementById('import-filter').oninput();
assert.deepEqual(document.getElementById('import-list').children.map(row=>row.hidden),[true,false,true,true]);
document.getElementById('import-form').onsubmit({preventDefault(){}});
assert.deepEqual(sent.at(-1),{action:'import',indices:[0,3]},'只送索引，且藏起來的勾選仍算數');
boxes[0].checked=false; boxes[0].onchange();
assert.equal(boxes[2].disabled,false,'取消勾選後釋出名額');
ctx.window.showImport({entries,remaining:0});
assert.equal(document.getElementById('import-count').textContent,'The sidebar is full. Remove a site before importing.');
assert.ok(document.getElementById('import-list').children.every(row=>row.children[0].disabled),'側欄已滿時全部停用');
assert.equal(document.getElementById('import-submit').disabled,true);
console.log('PASS bookmark import picker sends indices, enforces the limit and filters by name or URL');


// 設定分區維持頁籤、捲動位置與鍵盤焦點；後端 ACK 才取代快捷鍵草稿。
ctx.window.closeOverlay();
ctx.window.render(state);
ctx.window.showSettings();
settingsTab('shortcuts');
for(const name of ['general','sites','shortcuts']) {
  const tab=document.getElementById(`settings-tab-${name}`);
  assert.equal(tab.getAttribute('role'),'tab');
  assert.equal(tab.getAttribute('aria-selected'),String(name==='shortcuts'));
  assert.equal(document.getElementById(`settings-${name}`).hidden,name!=='shortcuts');
}
assert.equal(document.activeElement,document.getElementById('settings-tab-shortcuts'),'切換分區後焦點留在頁籤');
document.activeElement.dispatchEvent(event('keydown',{key:'ArrowRight'}));
assert.equal(document.activeElement.id,'settings-tab-general','方向鍵循環切換設定分區');
document.activeElement.dispatchEvent(event('keydown',{key:'End'}));
assert.equal(document.activeElement.id,'settings-tab-shortcuts','End 移到最後一個設定分區');
document.getElementById('overlay').scrollTop=173;
document.getElementById('shortcut-key').value='F19';
document.getElementById('shortcut-control').checked=true;
document.getElementById('shortcut-key').focus();
ctx.window.render({...state,settings:{...state.settings,pinned:true}});
assert.equal(document.getElementById('settings-shortcuts').hidden,false,'無關設定更新保留目前分區');
assert.equal(document.getElementById('overlay').scrollTop,173,'無關設定更新保留閱讀位置');
assert.equal(document.activeElement,document.getElementById('shortcut-key'),'重建設定保留一般控制項焦點');
assert.equal(document.getElementById('shortcut-key').value,'F19','無關設定更新保留未提交草稿');
assert.equal(document.getElementById('shortcut-control').checked,true);
ctx.window.render({...state,shortcut_error:'That shortcut is unavailable.'});
assert.equal(document.getElementById('shortcut-key').value,'F19','儲存失敗保留草稿');
const acknowledged={...state,settings:{...state.settings,toggle_shortcut:{control:false,option:true,shift:false,command:true,key:'KeyQ'}}};
ctx.window.render(acknowledged);
assert.equal(document.getElementById('shortcut-key').value,'KeyQ','保存成功後採用已保存的快捷鍵');
assert.equal(document.getElementById('shortcut-control').checked,false);
assert.equal(document.getElementById('shortcut-option').checked,true);
settingsTab('sites');
assert.equal(document.activeElement,document.getElementById('settings-tab-sites'));
assert.equal(document.getElementById('settings-sites').hidden,false);
assert.equal(document.getElementById('settings-shortcuts').hidden,true);
ctx.window.render(tabState);
ctx.window.showSettings('sites');
document.getElementById('site-7-rename').click();
assert.equal(document.activeElement,document.getElementById('pad-name'));
find(document.getElementById('overlay'),'← Back to settings').click();
assert.equal(document.getElementById('settings-sites').hidden,false,'編輯返回保留網站分區，onclick 事件不可當作分區名稱');
console.log('PASS settings tabs preserve scroll, focus and shortcut drafts until persistence succeeds');

// Modal 開關只讓使用者關閉送 IPC；背景不可操作、Tab 不可跑到背後。
ctx.window.closeOverlay();
ctx.window.render(state);
document.getElementById('settings').focus();
document.getElementById('settings').click();
assert.deepEqual(sent.at(-1),{action:'show_settings'});
ctx.window.showSettings();
const dialog=document.getElementById('overlay');
assert.equal(dialog.getAttribute('role'),'dialog');
assert.equal(dialog.getAttribute('aria-modal'),'true');
assert.equal(document.activeElement.id,'settings-tab-general','設定開啟時聚焦第一個分區，讓鍵盤可立即選擇設定');
for(const selector of ['.rail','.toolbar','.stage','.status'])assert.equal(document.querySelector(selector).inert,true,`${selector} 在 dialog 開啟時應 inert`);
const focusable=dialog.querySelectorAll('button,input,select,textarea,summary,a[href],[tabindex]').filter(element=>!element.disabled&&element.tabIndex!==-1&&element.getClientRects().length);
assert.ok(focusable.length>1);
focusable.at(-1).focus();
const wrap=event('keydown',{key:'Tab',shiftKey:false});
document.activeElement.dispatchEvent(wrap);
assert.equal(wrap.defaultPrevented,true,'最後一個控制項 Tab 應由 dialog 接手');
assert.equal(document.activeElement,focusable[0]);
const reverse=event('keydown',{key:'Tab',shiftKey:true});
document.activeElement.dispatchEvent(reverse);
assert.equal(document.activeElement,focusable.at(-1),'Shift+Tab 回到最後一個可操作控制項');
const beforeDismiss=sent.length;
document.getElementById('overlay-close').click();
assert.equal(sent.length,beforeDismiss+1);
assert.deepEqual(sent.at(-1),{action:'overlay',open:false});
assert.equal(document.activeElement,document.getElementById('settings'),'關閉返回啟動設定的入口');
assert.equal(dialog.hidden,true);
for(const selector of ['.rail','.toolbar','.stage','.status'])assert.equal(document.querySelector(selector).inert,false);
ctx.window.showAddForm();
assert.equal(document.activeElement,document.getElementById('new-address'));
const nativeCloseCount=sent.length;
ctx.window.closeOverlay();
assert.equal(sent.length,nativeCloseCount,'原生 closeOverlay 不額外回送 overlay IPC');
assert.equal(dialog.hidden,true);
ctx.window.showAddForm();
document.activeElement.dispatchEvent(event('keydown',{key:'Escape'}));
assert.deepEqual(sent.at(-1),{action:'overlay',open:false});
assert.equal(dialog.hidden,true,'Esc 關閉目前 dialog');
document.body.dispatchEvent(event('keydown',{key:'Escape'}));
assert.deepEqual(sent.at(-1),{action:'hide'},'沒有 dialog 時 Esc 才收合側欄');
console.log('PASS modal initial focus, inert background, Tab containment, dismissal and native-close IPC');

// 首頁搜尋不改網站順序或刪除資料，空結果與真正空清單分開。
ctx.window.render(populated);
assert.equal(document.getElementById('home-tools').hidden,false);
const siteFilter=document.getElementById('site-filter');
siteFilter.value='NOTION.SO'; siteFilter.dispatchEvent(event('input'));
assert.deepEqual(document.getElementById('saved-sites').children.map(row=>row.hidden),[false,true]);
siteFilter.value='工作'; siteFilter.dispatchEvent(event('input'));
assert.deepEqual(document.getElementById('saved-sites').children.map(row=>row.hidden),[false,true]);
siteFilter.value='no-such-site'; siteFilter.dispatchEvent(event('input'));
assert.equal(document.getElementById('site-no-results').hidden,false);
assert.equal(document.getElementById('empty-state').hidden,true,'沒有搜尋結果不等同沒有網站');
assert.equal(document.getElementById('saved-sites').children.length,2,'篩選不可刪掉網站列');
document.getElementById('clear-site-filter').click();
assert.equal(siteFilter.value,'','清除搜尋按鈕清空篩選文字');
assert.equal(document.activeElement,siteFilter,'清除搜尋後可直接重新輸入');
assert.equal(document.getElementById('site-no-results').hidden,true);
assert.deepEqual(document.getElementById('saved-sites').children.map(row=>row.hidden),[false,false]);
ctx.window.render(state);
assert.equal(document.getElementById('home-tools').hidden,true);
assert.equal(document.getElementById('site-no-results').hidden,true);
console.log('PASS home search matches titles and URLs and distinguishes no matches from an empty library');

// 加入上限與快捷推薦使用相同規則；canonical URL 已存在時直接選取。
const popular={...state,settings:{...state.settings,pads:[{id:88,title:'Mail',url:'https://mail.google.com/'}]}};
ctx.window.render(popular);
document.getElementById('suggestions').open=true;
let mail=find(document.getElementById('quick'),'Gmail').closest('button');
mail.click();
assert.deepEqual(sent.at(-1),{action:'select',id:88},'已加入的推薦不重複 add');
const full={...state,settings:{...state.settings,pads:[...popular.settings.pads,...Array.from({length:19},(_,index)=>({id:index+100,title:`Site ${index}`,url:`https://site${index}.example.com/`}))]}};
ctx.window.render(full);
assert.equal(document.getElementById('add').disabled,true);
assert.equal(document.getElementById('hero-add').disabled,true);
assert.equal(document.getElementById('site-limit').hidden,false);
assert.match(document.getElementById('site-limit').textContent,/20 sites.*remove one in Manage sites/,'滿額說明須提供可用的處理方法');
mail=find(document.getElementById('quick'),'Gmail').closest('button');
assert.equal(mail.disabled,false,'滿額仍可開啟已加入網站');
assert.equal(find(document.getElementById('quick'),'Notion').closest('button').disabled,true,'滿額禁用新網站推薦');
ctx.window.showAddForm();
assert.ok(!document.getElementById('new-address')||document.getElementById('new-address').disabled,'快捷鍵入口滿額時不可繼續送出新增表單');
assert.ok(dialog.textContent.includes('20'),'快捷鍵入口說明網站數量上限');
ctx.window.closeOverlay();
ctx.window.render(populated);
assert.equal(document.getElementById('add').disabled,false);
assert.equal(document.getElementById('hero-add').disabled,false);
assert.equal(document.getElementById('site-limit').hidden,true,'移除網站後收起上限提示');
assert.equal(document.getElementById('reload').disabled,true,'首頁不可重新整理隱藏中的 active 網站');
assert.equal(document.getElementById('external').disabled,true,'首頁不可外開隱藏中的 active 網站');
ctx.window.render({...page,settings:{...page.settings,active:7}});
assert.equal(document.getElementById('reload').disabled,false);
assert.equal(document.getElementById('external').disabled,false);
console.log('PASS capacity, canonical quick-site selection and home navigation availability');

// 匯入保留入口脈絡，篩選零結果與隱藏的已選項目都有明確回饋。
ctx.window.render(populated);
ctx.window.showSettings(); settingsTab('sites');
document.getElementById('import-bookmarks').click();
ctx.window.showImport({entries,remaining:2});
document.getElementById('import-back').click();
assert.equal(document.getElementById('settings-sites').hidden,false,'從管理網站進入匯入後返回相同分區');
ctx.window.closeOverlay();
document.getElementById('empty-import').onclick();
ctx.window.showImport({entries,remaining:2});
const returnCount=sent.length;
document.getElementById('import-back').click();
assert.equal(dialog.hidden,true,'從首頁進入匯入後返回首頁');
assert.deepEqual(sent.at(-1),{action:'overlay',open:false});
assert.equal(sent.length,returnCount+1);
ctx.window.showImport({entries,remaining:2});
const importBoxes=document.getElementById('import-list').children.map(row=>row.children[0]);
importBoxes[0].checked=true; importBoxes[0].dispatchEvent(event('change'));
importBoxes[3].checked=true; importBoxes[3].dispatchEvent(event('change'));
const importFilter=document.getElementById('import-filter');
importFilter.value='MAIL'; importFilter.dispatchEvent(event('input'));
assert.equal(document.getElementById('import-hidden-count').hidden,false);
assert.ok(document.getElementById('import-hidden-count').textContent.includes('1'),'顯示被篩選藏起來的選取數量');
importFilter.value='no-matching-bookmark'; importFilter.dispatchEvent(event('input'));
assert.equal(document.getElementById('import-no-results').hidden,false);
assert.ok(document.getElementById('import-hidden-count').textContent.includes('2'));
document.getElementById('import-form').dispatchEvent(event('submit'));
assert.deepEqual(sent.at(-1),{action:'import',indices:[0,3]},'篩選不改索引或選取集合');
importFilter.value=''; importFilter.dispatchEvent(event('input'));
assert.equal(document.getElementById('import-no-results').hidden,true);
assert.equal(document.getElementById('import-hidden-count').hidden,true);
console.log('PASS import origin, no-match feedback and filtered selections');

// 鍵盤讀取提示時與滑鼠懸停一樣暫停倒數，離開提示才重新計時。
ctx.window.closeOverlay();
ctx.window.showToast('Keyboard-readable notice');
const pending=timers.filter(Boolean).length;
document.getElementById('toast-close').focus();
assert.equal(timers.filter(Boolean).length,pending-1,'toast focusin 暫停關閉倒數');
document.getElementById('address').focus();
runTimers(0); // focusout 等瀏覽器更新 activeElement 後，才排入真正的關閉倒數。
assert.equal(timers.filter(Boolean).length,pending,'toast focusout 恢復關閉倒數');
assert.equal(document.getElementById('toast').hidden,false,'focusout 不可立即隱藏提示');
runTimers(8000);
assert.equal(document.getElementById('toast').hidden,true,'離開提示後實際的關閉倒數仍會執行');
console.log('PASS toast dismissal timer pauses for keyboard focus');

// 保存回覆會重建設定內容，觸發操作的控制項應保留焦點；已消失的操作退回目前分區。
ctx.window.closeOverlay();
let focusAckState={...state,settings:{...state.settings,height:540}};
ctx.window.render(focusAckState);
ctx.window.showSettings('general');
document.getElementById('more-settings-toggle').click();
document.getElementById('full-height').focus();
document.getElementById('full-height').click();
assert.deepEqual(sent.at(-1),{action:'full_height'});
focusAckState={...focusAckState,settings:{...focusAckState.settings,height:null}};
ctx.window.render(focusAckState);
assert.equal(document.activeElement.id,'full-height','恢復全高保存成功後焦點仍留在操作按鈕');
const generalControls=document.getElementById('settings-general').querySelectorAll('button,input,select')
  .filter(control=>!control.disabled&&control.tabIndex>=0);
const generalIds=generalControls.map(control=>control.id);
assert.ok(generalIds.every(Boolean),'一般設定可操作控制項須可在內容重建後重新定位');
assert.equal(new Set(generalIds).size,generalIds.length,'一般設定控制項定位不可互相覆蓋');
for(const id of generalIds) {
  document.getElementById(id).focus();
  focusAckState={...focusAckState,settings:{...focusAckState.settings,pinned:!focusAckState.settings.pinned}};
  ctx.window.render(focusAckState);
  assert.equal(document.activeElement.id,id,`${id} 應在一般設定更新後保留鍵盤焦點`);
}
settingsTab('shortcuts');
document.getElementById('shortcut-key').value='KeyB';
document.getElementById('shortcut-submit').focus();
document.getElementById('shortcut-form').dispatchEvent(event('submit'));
assert.equal(sent.at(-1).action,'set_shortcut');
assert.equal(sent.at(-1).shortcut.key,'KeyB');
focusAckState={...focusAckState,settings:{...focusAckState.settings,toggle_shortcut:sent.at(-1).shortcut}};
ctx.window.render(focusAckState);
assert.equal(document.activeElement.id,'shortcut-submit','快捷鍵保存成功後焦點仍留在套用按鈕');
assert.equal(document.getElementById('shortcut-key').value,'KeyB');
focusAckState={...focusAckState,undo_title:'Removed site'};
ctx.window.render(focusAckState);
settingsTab('sites');
document.getElementById('settings-undo').focus();
document.getElementById('settings-undo').click();
assert.deepEqual(sent.at(-1),{action:'undo_remove'});
ctx.window.render({...focusAckState,undo_title:null,settings:{...focusAckState.settings,pads:[{id:71,title:'Removed site',url:'https://restored.example.com/'}]}});
assert.equal(document.getElementById('settings-undo'),null,'復原成功後移除已完成的操作');
assert.equal(document.activeElement.id,'settings-tab-sites','復原按鈕消失時焦點退回網站分區，不掉到 body');
console.log('PASS settings acknowledgement preserves action focus and falls back when undo disappears');

// 首頁提示描述目前的設定，不把預設的右側、自動收合當成固定行為。
ctx.window.closeOverlay();
for(const side of ['left','right']) {
  ctx.window.render({...state,settings:{...state.settings,side}});
  assert.equal(document.getElementById('home-open-help').textContent,`Move to the ${side} edge of your screen to open the sidebar again.`);
  assert.match(document.getElementById('home-hide-help').textContent,/hides when you move away/);
  assert.equal(document.getElementById('home-pin').getAttribute('aria-pressed'),'false');
  assert.equal(document.getElementById('home-pin').textContent,'Keep sidebar open');
}
document.getElementById('home-pin').click();
assert.deepEqual(sent.at(-1),{action:'pin'});
assert.equal(document.getElementById('home-pin').getAttribute('aria-pressed'),'false','保存回覆前固定顯示仍反映目前狀態');
ctx.window.render({...state,settings:{...state.settings,hot_edge:false,pinned:true}});
assert.equal(document.getElementById('home-open-help').textContent,'Edge opening is off. Click the Open Slide Pad menu bar icon to open the sidebar.');
assert.equal(document.getElementById('home-hide-help').textContent,'The sidebar stays open, even when you move away.');
assert.equal(document.getElementById('home-pin').getAttribute('aria-pressed'),'true');
assert.equal(document.getElementById('home-pin').textContent,'Allow auto-hide');
document.getElementById('home-pin').click();
assert.deepEqual(sent.at(-1),{action:'pin'});
ctx.window.render({...state,settings:{...state.settings,language:'zh-TW',side:'left',pinned:true}});
assert.equal(document.getElementById('home-open-help').textContent,'移到螢幕左邊緣，就能再次打開。');
assert.equal(document.getElementById('home-hide-help').textContent,'目前固定顯示，游標移開也不會收起。');
assert.equal(document.getElementById('home-pin').textContent,'取消固定顯示');
console.log('PASS home guidance follows edge, side and pin settings with acknowledged state and bilingual copy');

// 新增與取消都保留入口脈絡；表單旁的回饋不跟著短暫提示消失。
ctx.window.render(state);
document.getElementById('hero-add').focus();
document.getElementById('hero-add').click();
assert.deepEqual(sent.at(-1),{action:'new_pad'});
ctx.window.showAddForm();
assert.equal(document.getElementById('add-cancel').textContent,'Cancel');
const addForm=document.getElementById('new-address').closest('form');
assert.equal(addForm.querySelector('.primary').textContent,'Add and open');
document.getElementById('new-address').value='youtube.com';
addForm.dispatchEvent(event('submit'));
assert.deepEqual(sent.at(-1),{action:'add',address:'youtube.com'});
ctx.window.showToast('Could not save settings');
assert.equal(document.getElementById('form-feedback').hidden,false);
assert.equal(document.getElementById('form-feedback').textContent,'Could not save settings');
assert.equal(document.getElementById('new-address').getAttribute('aria-describedby'),'form-feedback');
runTimers(8000);
assert.equal(document.getElementById('toast').hidden,true);
assert.equal(document.getElementById('form-feedback').hidden,false,'表單回饋保留到使用者修正，不隨 toast 自動關閉');
assert.equal(document.getElementById('new-address').value,'youtube.com','失敗保留已輸入內容');
document.getElementById('new-address').dispatchEvent(event('input'));
assert.equal(document.getElementById('form-feedback').hidden,true,'重新輸入後清除舊回饋');
document.getElementById('add-cancel').click();
assert.equal(dialog.hidden,true);
assert.deepEqual(sent.at(-1),{action:'overlay',open:false});
assert.equal(document.activeElement,document.getElementById('hero-add'),'從首頁取消新增回到原本入口');
ctx.window.showSettings('sites');
document.getElementById('settings-add-site').focus();
document.getElementById('settings-add-site').click();
assert.deepEqual(sent.at(-1),{action:'new_pad'});
ctx.window.showAddForm();
assert.equal(document.getElementById('add-cancel').textContent,'Back to manage sites');
document.getElementById('add-cancel').click();
assert.equal(dialog.hidden,false);
assert.equal(document.getElementById('settings-sites').hidden,false,'從設定取消新增回到網站分區');
assert.equal(document.activeElement.id,'settings-add-site','返回管理網站恢復新增入口焦點');
ctx.window.render(tabState);
document.getElementById('site-7-rename').click();
ctx.window.showToast('Site names cannot contain control characters');
runTimers(8000);
assert.equal(document.getElementById('form-feedback').hidden,false,'重新命名錯誤同樣持續顯示');
document.getElementById('pad-name').value='New name';
document.getElementById('pad-name').dispatchEvent(event('input'));
assert.equal(document.getElementById('form-feedback').hidden,true);
assert.ok(find(dialog,'8 / 80 characters'),'清除回饋不能取代名稱長度提示的 input handler');
console.log('PASS add completion label, cancellation origin and persistent add/rename feedback');

// 進階設定預設收合；收合內容不可取得焦點，summary 本身仍屬 dialog 鍵盤順序。
ctx.window.closeOverlay();
ctx.window.render(state);
ctx.window.showSettings('general');
assert.equal(document.getElementById('more-settings').open,false);
const summary=document.getElementById('more-settings-toggle');
const hiddenSleep=document.getElementById('sleep-select');
assert.equal(summary.tabIndex,0);
assert.equal(summary.getClientRects().length,1);
assert.equal(hiddenSleep.getClientRects().length,0);
summary.focus();
hiddenSleep.focus();
assert.equal(document.activeElement,summary,'收合內容不可搶走焦點');
const summaryTab=event('keydown',{key:'Tab'});
summary.dispatchEvent(summaryTab);
assert.equal(summaryTab.defaultPrevented,false,'summary 在焦點序列內，不應被當成 dialog 外部焦點');
document.activeElement=hiddenSleep; // 模擬內容剛收合時仍殘留的焦點。
const hiddenTab=event('keydown',{key:'Tab'});
hiddenSleep.dispatchEvent(hiddenTab);
assert.equal(hiddenTab.defaultPrevented,true,'焦點陷阱不可把收合的控制項當作可操作項目');
assert.equal(document.activeElement.id,'overlay-close');
settingsTab('shortcuts');
document.getElementById('shortcut-key').value='F19';
document.getElementById('shortcut-option').checked=true;
settingsTab('general');
summary.click();
assert.equal(document.getElementById('more-settings').open,true);
hiddenSleep.focus();
dialog.scrollTop=137;
ctx.window.render({...state,settings:{...state.settings,pinned:true}});
assert.equal(document.getElementById('more-settings').open,true,'設定回覆不能收起使用者展開的內容');
assert.equal(document.activeElement.id,'sleep-select');
assert.equal(dialog.scrollTop,137);
assert.equal(document.getElementById('shortcut-key').value,'F19');
assert.equal(document.getElementById('shortcut-option').checked,true,'展開狀態更新不能沖掉另一分區的快捷鍵草稿');
document.getElementById('more-settings-toggle').click();
document.getElementById('more-settings-toggle').focus();
ctx.window.render(state);
assert.equal(document.getElementById('more-settings').open,false,'已收合狀態同樣跨重繪保存');
assert.equal(document.activeElement.id,'more-settings-toggle');
console.log('PASS advanced settings disclosure, hidden-control exclusion, summary focus and view/draft restoration');

// 失敗頁直接提供修改網址與返回首頁，沿用既有 IPC 而不新增導覽副作用。
ctx.window.closeOverlay();
ctx.window.render({...page,failure:'unreachable',dismissible:false});
document.getElementById('load-edit').click();
assert.deepEqual(sent.at(-1),{action:'focus_address'});
ctx.window.focusAddress();
assert.equal(document.activeElement.id,'address');
assert.equal(document.getElementById('address').selectionEnd,page.address.length,'修改網址入口選取完整舊網址供直接替換');
document.getElementById('load-home').click();
assert.deepEqual(sent.at(-1),{action:'home'});
console.log('PASS failed-page edit-address and home recovery actions route existing IPC');

// 狀態更新不能拆掉清單或目前聚焦的分頁；載入進度也不重建相同文字的狀態列。
ctx.window.closeOverlay();
let renderState={...tabState,loading:true,progress:0.2,sleeping:[],settings:{...tabState.settings,pads:[
  ...tabState.settings.pads,{id:99,title:'Mail',url:'https://mail.google.com/'},
]}};
ctx.window.render(renderState);
const stableTabs=[7,42,99].map(id=>document.getElementById(`pad-${id}`));
const stableHome=document.getElementById('site-7');
const stableQuick=document.getElementById('quick').firstChild;
const stableStatus=document.getElementById('status-text').firstChild;
stableTabs[1].focus();
renderState={...renderState,settings:{...renderState.settings,active:42}};
ctx.window.render(renderState);
assert.equal(document.activeElement,stableTabs[1],'切換目前分頁不能丟掉鍵盤焦點');
assert.equal(stableTabs[0].getAttribute('aria-pressed'),'false');
assert.equal(stableTabs[1].getAttribute('aria-pressed'),'true');
assert.ok(stableTabs[1].classList.contains('active'));
stableTabs[1].click();
assert.deepEqual(sent.at(-1),{action:'select',id:42},'沿用的分頁仍保留選取處理器');
renderState={...renderState,settings:{...renderState.settings,pinned:true},sleeping:[7],progress:0.6};
ctx.window.render(renderState);
ctx.window.render(JSON.parse(JSON.stringify(renderState)));
assert.deepEqual([7,42,99].map(id=>document.getElementById(`pad-${id}`)),stableTabs,'偏好與休眠更新沿用分頁節點');
assert.equal(document.getElementById('site-7'),stableHome,'切換與釘選不重建首頁網站');
assert.equal(document.getElementById('quick').firstChild,stableQuick,'切換與釘選不重建建議網站');
assert.equal(document.getElementById('status-text').firstChild,stableStatus,'進度或相同 snapshot 不重啟狀態圓點');
assert.equal(document.getElementById('progress-bar').style.width,'60%','沿用狀態列仍更新進度');
assert.match(stableTabs[0].title,/Sleeping/);
assert.match(stableTabs[0].getAttribute('aria-label'),/Sleeping/);
ctx.window.render({...renderState,home:true});
assert.ok(stableTabs.every(tab=>tab.getAttribute('aria-pressed')==='false'),'返回首頁清除目前分頁標示');
assert.equal(document.getElementById('status-text').textContent,'Home');
ctx.window.render(renderState);
assert.ok(stableTabs[1].classList.contains('active'),'離開首頁恢復目前分頁標示');
console.log('PASS state updates retain list and status nodes while updating selection, sleep and progress');

const englishStatus=document.getElementById('status-text').firstChild;
ctx.window.render({...renderState,settings:{...renderState.settings,language:'zh-TW'}});
assert.equal(document.getElementById('pad-7'),stableTabs[0],'翻譯狀態不拆掉分頁按鈕');
assert.match(stableTabs[0].title,/已休眠/);
assert.equal(stableTabs[0].getAttribute('aria-label'),'First · 已休眠');
assert.equal(document.getElementById('site-7').getAttribute('aria-label'),'開啟 First');
assert.ok(find(document.getElementById('quick'),'已加入 · 點一下開啟'));
assert.equal(document.getElementById('status-text').textContent,'正在載入…');
assert.notEqual(document.getElementById('status-text').firstChild,englishStatus,'語言改變時仍刷新狀態文字');
ctx.window.render(renderState);
assert.equal(document.getElementById('site-7').getAttribute('aria-label'),'Open First');
assert.equal(document.getElementById('status-text').textContent,'Loading…');
console.log('PASS language-only updates translate retained tabs, home, quick sites and status');

renderState={...renderState,home:true,settings:{...renderState.settings,pads:[...renderState.settings.pads].reverse()}};
ctx.window.render(renderState);
assert.deepEqual(document.getElementById('pads').children.map(tab=>tab.id),['pad-99','pad-42','pad-7']);
assert.deepEqual(document.getElementById('saved-sites').children.map(site=>site.id),['site-99','site-42','site-7']);
assert.match(document.getElementById('pad-7').title,/⌘3/,'排序變更同步快捷鍵位置');
const renamedPads=renderState.settings.pads.map(pad=>pad.id===7?{...pad,title:'Renamed',url:'https://changed.example.net/'}:pad);
renderState={...renderState,settings:{...renderState.settings,pads:renamedPads}};
ctx.window.render(renderState);
assert.equal(document.getElementById('pad-7').textContent,'RE');
assert.equal(document.getElementById('site-7').children[1].textContent,'Renamedchanged.example.net');
document.getElementById('site-7').click();
assert.deepEqual(sent.at(-1),{action:'select',id:7});
const removedState={...renderState,settings:{...renderState.settings,pads:renamedPads.filter(pad=>pad.id!==99)}};
ctx.window.render(removedState);
assert.equal(document.getElementById('pad-99'),null);
assert.equal(document.getElementById('site-99'),null);
document.getElementById('suggestions').open=true;
document.getElementById('quick').firstChild.click();
assert.deepEqual(sent.at(-1),{action:'add',address:'https://mail.google.com'},'移除後建議網站恢復加入操作');
ctx.window.render(renderState);
document.getElementById('quick').firstChild.click();
assert.deepEqual(sent.at(-1),{action:'select',id:99},'復原後建議網站改回開啟既有網站');
assert.equal(document.getElementById('pads').children.length,3);
assert.equal(document.getElementById('saved-sites').children.length,3);
console.log('PASS list mutations refresh order, rename, address, removal, undo and quick-site actions');
