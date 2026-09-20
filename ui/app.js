'use strict';
const $ = id => document.getElementById(id);
let snapshot = null;
let overlayMode = null;
let toastTimer;

function send(action, fields = {}) {
  window.ipc.postMessage(JSON.stringify({action, ...fields}));
}
function button(label, action, className = 'pill') {
  const element = document.createElement('button');
  element.type = 'button';
  element.textContent = label;
  element.className = className;
  element.onclick = action;
  return element;
}
function text(tag, content, className = '') {
  const element = document.createElement(tag);
  element.textContent = content;
  element.className = className;
  return element;
}
window.showToast = message => {
  $('toast').textContent = message;
  $('toast').hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => $('toast').hidden = true, 8000);
};
window.closeOverlay = () => {
  overlayMode = null;
  $('overlay').hidden = true;
};
function dismiss() {
  window.closeOverlay();
  send('overlay', {open:false});
}
window.focusAddress = () => {
  window.closeOverlay();
  $('address').focus();
  $('address').select();
};
function overlay(title, mode) {
  overlayMode = mode;
  send('overlay', {open:true});
  const element = $('overlay');
  element.replaceChildren();
  element.hidden = false;
  const header = text('div', '', 'overlay-header');
  const close = button('✕', dismiss, 'icon');
  close.setAttribute('aria-label', '關閉');
  header.append(text('h2', title), close);
  element.append(header);
  return element;
}
function inputField(id, label, value = '') {
  const fieldLabel = text('label', label, 'field');
  fieldLabel.htmlFor = id;
  const input = document.createElement('input');
  input.id = id;
  input.className = 'big-input';
  input.value = value;
  input.required = true;
  input.autocomplete = 'off';
  return {fieldLabel, input};
}
function submitRow(label) {
  const actions = text('div', '', 'form-actions');
  const submit = button(label, () => {}, 'primary');
  submit.type = 'submit';
  actions.append(submit);
  return actions;
}
window.showAddForm = () => {
  const element = overlay('新增網站', 'add');
  element.append(text('p', '讓常用的工具，在你的工作旁邊待命。', 'hint'));
  const form = document.createElement('form');
  const {fieldLabel, input} = inputField('new-address', '網址或搜尋內容');
  input.placeholder = '例如 notion.so';
  form.append(fieldLabel, input,
    text('p', '也可以輸入關鍵字，用 DuckDuckGo 搜尋。', 'hint'), submitRow('加入側欄 ↗'));
  form.onsubmit = event => {
    event.preventDefault();
    send('add', {address:input.value});
  };
  element.append(form);
  input.focus();
};
function editPad(pad) {
  const element = overlay('重新命名網站', 'edit');
  const form = document.createElement('form');
  const {fieldLabel, input} = inputField('pad-name', '在側欄顯示的名稱', pad.title);
  input.maxLength = 160;
  const count = text('p', '', 'hint');
  input.oninput = () => { count.textContent = `${Array.from(input.value.trim()).length} / 80 字`; };
  input.oninput();
  form.append(fieldLabel, input, count, text('p', pad.url, 'hint address-hint'), submitRow('儲存名稱'));
  form.onsubmit = event => {
    event.preventDefault();
    send('rename', {id:pad.id, title:input.value});
  };
  element.append(form, button('← 返回設定', window.showSettings, 'text-button'));
  input.focus();
  input.select();
}
window.showSettings = () => {
  const element = overlay('留一個舒服的位置', 'settings');
  renderSettings(element);
};
function row(title, description, control) {
  const element = text('div', '', 'setting-row');
  const copy = text('div', '');
  copy.append(text('strong', title), text('p', description));
  element.append(copy, control);
  return element;
}
function padRow(pad, index, total) {
  const entry = text('div', '', 'site-row');
  const info = text('div', '', 'site-info');
  info.append(text('strong', pad.title), text('small', pad.url));
  const actions = text('div', '', 'site-actions');
  const up = button('↑', () => send('move', {id:pad.id, position:index - 1}), 'mini-button');
  const down = button('↓', () => send('move', {id:pad.id, position:index + 1}), 'mini-button');
  up.disabled = index === 0;
  down.disabled = index === total - 1;
  up.setAttribute('aria-label', `將 ${pad.title} 向上移`);
  down.setAttribute('aria-label', `將 ${pad.title} 向下移`);
  const rename = button('編輯', () => editPad(pad), 'mini-button');
  rename.setAttribute('aria-label', `重新命名 ${pad.title}`);
  const remove = button('✕', () => send('remove', {id:pad.id}), 'mini-button danger');
  remove.setAttribute('aria-label', `移除 ${pad.title}`);
  actions.append(up, down, rename, remove);
  entry.append(info, actions);
  return entry;
}
function shortcutEditor() {
  const form = document.createElement('form');
  form.id = 'shortcut-form';
  form.className = 'shortcut-editor';
  form.append(text('strong', '顯示／收合快捷鍵'));
  const status = snapshot.shortcut_active
    ? `目前使用 ${snapshot.shortcut_label}` : '目前未啟用，請選擇組合後重新套用';
  form.append(text('p', status, 'hint'));
  const controls = text('div', '', 'shortcut-controls');
  [['control','⌃ Control'],['option','⌥ Option'],['shift','⇧ Shift'],['command','⌘ Command']].forEach(([name, title]) => {
    const label = text('label', '', 'modifier');
    const input = document.createElement('input');
    Object.assign(input, {type:'checkbox', id:`shortcut-${name}`, checked:snapshot.settings.toggle_shortcut[name]});
    label.append(input, document.createTextNode(title));
    controls.append(label);
  });
  const keyLabel = text('label', '搭配按鍵', 'field');
  keyLabel.htmlFor = 'shortcut-key';
  const select = document.createElement('select');
  select.id = 'shortcut-key';
  const keys = [['Space','空白鍵 Space'],
    ...Array.from('ABCDEFGHIJKLMNOPQRSTUVWXYZ', letter => [`Key${letter}`, letter]),
    ...Array.from('0123456789', digit => [`Digit${digit}`, digit]),
    ...Array.from({length:20}, (_, index) => [`F${index + 1}`, `F${index + 1}`])];
  keys.forEach(([value, label]) => {
    const option = text('option', label);
    option.value = value;
    select.append(option);
  });
  select.value = snapshot.settings.toggle_shortcut.key;
  const actions = submitRow('套用快捷鍵');
  const reset = button('恢復預設', () => {
    const shortcut = {control:false, option:false, shift:true, command:true, key:'Space'};
    // 已保存預設值時後端狀態不變，仍須清除表單尚未套用的草稿。
    ['control','option','shift','command'].forEach(name => {
      $(`shortcut-${name}`).checked = shortcut[name];
    });
    select.value = shortcut.key;
    send('set_shortcut', {shortcut});
  }, 'pill');
  reset.id = 'shortcut-reset';
  actions.prepend(reset);
  form.append(controls, keyLabel, select,
    text('p', '至少選一個 ⌘、⌥ 或 ⌃。套用後立即生效，下次開啟也會保留。', 'hint'), actions);
  if (snapshot.shortcut_error) {
    const error = text('p', snapshot.shortcut_error, 'hint danger');
    error.setAttribute('role', 'alert');
    form.append(error);
  }
  form.onsubmit = event => {
    event.preventDefault();
    const shortcut = {key:select.value};
    ['control','option','shift','command'].forEach(name => {
      shortcut[name] = $(`shortcut-${name}`).checked;
    });
    send('set_shortcut', {shortcut});
  };
  return form;
}
function renderSettings(element) {
  if (!snapshot) return;
  const settings = snapshot.settings;
  while (element.children.length > 1) element.lastChild.remove();
  element.append(shortcutEditor());
  element.append(row('側欄位置', '在你順手的一側打開',
    button(settings.side === 'right' ? '右側 →' : '← 左側', () => send('side'))));
  element.append(row('觸碰邊緣開啟', '游標停留片刻即可滑出',
    button(settings.hot_edge ? '已開啟' : '已關閉', () => send('hot_edge'))));
  element.append(row('固定顯示', '游標移開時仍保留側欄',
    button(settings.pinned ? '已釘選' : '自動收合', () => send('pin'))));
  element.append(text('label', `側欄寬度 · ${settings.width} pt`, 'field'));
  const range = document.createElement('input');
  Object.assign(range, {type:'range', min:'360', max:'960', step:'20', value:settings.width});
  range.setAttribute('aria-label', '側欄寬度');
  range.onchange = () => send('width', {width:Number(range.value)});
  element.append(range, row('視窗高度', settings.height === null ? '跟隨螢幕可用高度' : `目前 ${Math.round(settings.height)} pt · 拖曳邊緣或下角調整`,
    button('恢復全高', () => send('full_height'))), text('div', `網站 · ${settings.pads.length} / 20`, 'label'));
  settings.pads.forEach((pad, index) => element.append(padRow(pad, index, settings.pads.length)));
  if (!settings.pads.length) element.append(text('p', '尚未加入網站，按左側 ＋ 開始。', 'hint'));
  element.append(text('p', '用箭頭調整順序，按編輯重新命名。移除後可在下方復原最近一個捷徑；登入資料不受影響。', 'hint'));
  element.append(text('div', '鍵盤也很順手', 'label'));
  const shortcuts = text('div', '', 'shortcut-list');
  [['選取網址','⌘ L'],['新增網站','⌘ T'],['重新整理','⌘ R'],['上一頁／下一頁','⌘ [ / ⌘ ]'],['切換網站','⌘ 1–9'],['收合側欄','⌘ W'],['開啟設定','⌘ ,']].forEach(([name, keys]) => {
    const item = text('div', '', 'shortcut-row');
    item.append(text('span', name), text('kbd', keys));
    shortcuts.append(item);
  });
  element.append(shortcuts, button('結束 Open Slide Pad', () => send('quit'), 'pill'));
  element.append(text('p', 'Open Slide Pad 0.4.1 · 獨立 Rust 實作', 'version'));
}
function renderPads(state) {
  $('pads').replaceChildren();
  state.settings.pads.forEach((pad, index) => {
    const initial = Array.from(pad.title.replace(/^www\./, '')).slice(0,2).join('').toUpperCase();
    const selected = !state.home && state.settings.active === pad.id;
    const element = button(initial, () => send('select', {id:pad.id}), `pad${selected ? ' active' : ''}`);
    element.title = pad.title + (index < 9 ? ` · ⌘${index + 1}` : '');
    element.setAttribute('aria-label', pad.title);
    element.setAttribute('aria-pressed', String(selected));
    $('pads').append(element);
  });
}
window.render = state => {
  const settingsChanged = JSON.stringify(snapshot?.settings) !== JSON.stringify(state.settings);
  const homeChanged = snapshot?.home !== state.home;
  const shortcutChanged = snapshot?.shortcut_error !== state.shortcut_error || snapshot?.shortcut_active !== state.shortcut_active;
  snapshot = state;
  const settings = state.settings;
  document.documentElement.dataset.side = settings.side;
  if (settingsChanged || homeChanged) renderPads(state);
  $('home').hidden = !state.home;
  $('pin').classList.toggle('active', settings.pinned);
  $('pin').setAttribute('aria-pressed', String(settings.pinned));
  $('back').disabled = state.home || !state.back;
  $('forward').disabled = state.home || !state.forward;
  if (document.activeElement !== $('address')) $('address').value = state.home ? '' : (state.address || '');
  $('status').classList.toggle('loading', !state.home && state.loading);
  const label = state.home ? '你的網路小角落' : state.loading ? '正在載入…' : state.title || '準備就緒';
  $('status-text').replaceChildren(text('i', '', 'dot'), document.createTextNode(label));
  $('undo-remove').hidden = !state.undo_title;
  $('undo-remove').title = state.undo_title ? `復原「${state.undo_title}」` : '';
  $('status-shortcut').hidden = Boolean(state.undo_title);
  $('status-shortcut').textContent = state.shortcut_active ? state.shortcut_label : '快捷鍵未啟用';
  $('home-shortcut').textContent = state.shortcut_active ? state.shortcut_label : '選單列 ◧';
  $('hero-add').firstChild.textContent = settings.pads.length ? '＋ 再加一個常用網站 ' : '＋ 新增你的第一個網站 ';
  if ((settingsChanged || shortcutChanged) && overlayMode === 'settings') renderSettings($('overlay'));
};
document.querySelectorAll('[data-action]').forEach(element => {
  element.onclick = () => send(element.dataset.action);
});
$('add').onclick = $('hero-add').onclick = () => send('new_pad');
$('settings').onclick = () => send('show_settings');
$('resize-grip').onpointerdown = event => {
  if (event.button !== 0) return;
  event.preventDefault();
  send('begin_resize');
};
$('address-form').onsubmit = event => {
  event.preventDefault();
  send('navigate', {address:$('address').value});
  $('address').blur();
};
$('address').onfocus = () => $('address').select();
[['ChatGPT','AI 隨手幫忙','https://chatgpt.com','✳'],['Gmail','收件匣在旁邊','https://mail.google.com','M'],['Notion','筆記與靈感','https://www.notion.so','N'],['YouTube','一點聲音與靈感','https://www.youtube.com','▶']].forEach(([name, description, address, mark]) => {
  const element = button('', () => send('add', {address}), 'quick');
  const copy = text('div', '');
  copy.append(text('strong', name), text('small', description));
  element.append(text('span', mark, 'quick-logo'), copy);
  $('quick').append(element);
});
// Cmd 快捷鍵由原生選單統一處理，避免同一個按鍵重複送出命令。
document.addEventListener('keydown', event => {
  if (event.key === 'Escape') overlayMode ? dismiss() : send('hide');
});
send('ready');
