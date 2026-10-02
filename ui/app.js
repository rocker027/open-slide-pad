'use strict';
// 版本以 Cargo.toml 為唯一來源，由 Rust 在組合文件時代入。
const APP_VERSION = '/* SLIDERUST_VERSION */';
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
const TOAST_DURATION_MS = 8000;
function hideToast() {
  clearTimeout(toastTimer);
  $('toast').hidden = true;
}
function armToastTimer() {
  clearTimeout(toastTimer);
  toastTimer = setTimeout(hideToast, TOAST_DURATION_MS);
}
window.showToast = message => {
  $('toast-text').textContent = message;
  $('toast').hidden = false;
  armToastTimer();
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
  close.id = 'overlay-close';
  close.setAttribute('aria-label', t('關閉'));
  const heading = text('h2', title);
  heading.id = 'overlay-title';
  header.append(heading, close);
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
  const element = overlay(t('新增網站'), 'add');
  element.append(text('p', t('加入後，可從左側或首頁直接開啟。'), 'hint'));
  const form = document.createElement('form');
  const {fieldLabel, input} = inputField('new-address', t('網址或搜尋內容'));
  input.placeholder = t('例如 notion.so');
  form.append(fieldLabel, input,
    text('p', t('也可以輸入關鍵字，用 DuckDuckGo 搜尋。'), 'hint'), submitRow(t('加入側欄')));
  form.onsubmit = event => {
    event.preventDefault();
    send('add', {address:input.value});
  };
  element.append(form);
  input.focus();
};
function editPad(pad) {
  const element = overlay(t('重新命名網站'), 'edit');
  const form = document.createElement('form');
  const {fieldLabel, input} = inputField('pad-name', t('在側欄顯示的名稱'), pad.title);
  input.maxLength = 160;
  const count = text('p', '', 'hint');
  input.oninput = () => { count.textContent = t('{count} / 80 字', {count:Array.from(input.value.trim()).length}); };
  input.oninput();
  form.append(fieldLabel, input, count, text('p', pad.url, 'hint address-hint'), submitRow(t('儲存名稱')));
  form.onsubmit = event => {
    event.preventDefault();
    send('rename', {id:pad.id, title:input.value});
  };
  element.append(form, button(t('← 返回設定'), window.showSettings, 'text-button'));
  input.focus();
  input.select();
}
window.showSettings = () => {
  const element = overlay(t('設定'), 'settings');
  renderSettings(element);
};
function importRow(entry, index) {
  const row = text('label', '', 'site-row import-row');
  const box = document.createElement('input');
  Object.assign(box, {type:'checkbox', value:String(index), disabled:entry.added});
  const copy = text('div', '', 'site-info');
  copy.append(text('strong', entry.title), text('small', siteDomain(entry.url)));
  row.append(box, copy);
  if (entry.added) row.append(text('span', t('已加入'), 'site-key'));
  return {row, box};
}
function selectedIndices(rows) {
  return rows.filter(({box}) => box.checked && !box.disabled).map(({box}) => Number(box.value));
}
// 到達上限後，其餘未勾選的項目不可再勾；已加入的項目一直停用。
function bindImportLimit(rows, entries, remaining, counter, submit) {
  const refresh = () => {
    const count = selectedIndices(rows).length;
    counter.textContent = remaining
      ? t('已選 {count} · 還可加入 {remaining}', {count, remaining:remaining - count})
      : t('側欄已滿，請先移除網站再匯入。');
    rows.forEach(({box}, index) => { if (!entries[index].added) box.disabled = !box.checked && count >= remaining; });
    submit.disabled = count === 0;
  };
  rows.forEach(({box}) => { box.onchange = refresh; });
  refresh();
}
// 篩選只隱藏列，藏起來的勾選仍算數。
function bindImportFilter(input, rows, entries) {
  input.oninput = () => {
    const needle = input.value.trim().toLowerCase();
    rows.forEach(({row}, index) => {
      row.hidden = Boolean(needle) && !entries[index].title.toLowerCase().includes(needle) && !entries[index].url.toLowerCase().includes(needle);
    });
  };
}
// 候選清單由 Rust 一次推送，不進 render 的狀態；送回的只有索引，避免超過 IPC 訊息上限。
window.showImport = ({entries, remaining}) => {
  const element = overlay(t('從 Chrome 匯入書籤'), 'import');
  const back = button(t('← 返回設定'), window.showSettings, 'text-button');
  if (!entries.length) {
    const empty = text('p', t('找不到 Chrome 書籤。只會讀取 Google Chrome 各個設定檔的書籤，不含其他瀏覽器。'), 'hint');
    empty.id = 'import-empty';
    element.append(empty, back);
    return;
  }
  const form = document.createElement('form');
  form.id = 'import-form';
  const {fieldLabel, input} = inputField('import-filter', t('篩選書籤'));
  Object.assign(input, {required:false, type:'search', placeholder:t('輸入名稱或網址')});
  const counter = text('p', '', 'hint');
  counter.id = 'import-count';
  counter.setAttribute('aria-live', 'polite');
  const list = text('div', '', 'import-list');
  list.id = 'import-list';
  const rows = entries.map(importRow);
  rows.forEach(({row}) => list.append(row));
  const actions = submitRow(t('加入所選'));
  actions.lastChild.id = 'import-submit';
  bindImportLimit(rows, entries, remaining, counter, actions.lastChild);
  bindImportFilter(input, rows, entries);
  form.onsubmit = event => {
    event.preventDefault();
    send('import', {indices:selectedIndices(rows)});
  };
  form.append(fieldLabel, input, counter, list, actions);
  element.append(form, back);
  input.focus();
};
function row(title, description, control) {
  const element = text('div', '', 'setting-row');
  const copy = text('div', '');
  copy.append(text('strong', title), text('p', description));
  element.append(copy, control);
  return element;
}
function toggle(label, checked, action) {
  const control = button('', () => send(action), 'switch-button');
  control.setAttribute('role', 'switch');
  control.setAttribute('aria-label', label);
  control.setAttribute('aria-checked', String(checked));
  return control;
}
function sideControl(side) {
  const group = text('div', '', 'segmented');
  group.setAttribute('role', 'group');
  group.setAttribute('aria-label', t('側欄位置'));
  [['left', t('← 左側')], ['right', t('右側 →')]].forEach(([value, label]) => {
    const option = button(label, () => { if (snapshot.settings.side !== value) send('side'); }, 'segment');
    option.id = `side-${value}`;
    option.setAttribute('aria-pressed', String(side === value));
    group.append(option);
  });
  return group;
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
  up.setAttribute('aria-label', t('將 {name} 向上移', {name:pad.title}));
  down.setAttribute('aria-label', t('將 {name} 向下移', {name:pad.title}));
  const rename = button(t('編輯'), () => editPad(pad), 'mini-button');
  rename.setAttribute('aria-label', t('重新命名 {name}', {name:pad.title}));
  const remove = button('✕', () => send('remove', {id:pad.id}), 'mini-button danger');
  remove.setAttribute('aria-label', t('移除 {name}', {name:pad.title}));
  actions.append(up, down, rename, remove);
  entry.append(info, actions);
  return entry;
}
const SHORTCUT_MODIFIERS = [['control','⌃'],['option','⌥'],['shift','⇧'],['command','⌘']];
function shortcutKeys() {
  return [['Space', t('空白鍵 Space')],
    ...Array.from('ABCDEFGHIJKLMNOPQRSTUVWXYZ', letter => [`Key${letter}`, letter]),
    ...Array.from('0123456789', digit => [`Digit${digit}`, digit]),
    ...Array.from({length:20}, (_, index) => [`F${index + 1}`, `F${index + 1}`])];
}
function fillShortcutDraft(shortcut) {
  SHORTCUT_MODIFIERS.forEach(([name]) => { $(`shortcut-${name}`).checked = shortcut[name]; });
  $('shortcut-key').value = shortcut.key;
}
function shortcutRecorder(keys) {
  const recorder = document.createElement('input');
  Object.assign(recorder, {id:'shortcut-recorder', className:'recorder', readOnly:true, placeholder:t('點這裡，再按下新的組合鍵')});
  recorder.setAttribute('aria-label', t('錄製快捷鍵'));
  recorder.onkeydown = event => {
    if (event.key === 'Tab') return;
    event.preventDefault();
    // Esc 只取消錄製，不往上傳到關閉設定的處理器。
    event.stopPropagation();
    if (event.key === 'Escape') { recorder.blur(); return; }
    const key = keys.find(([value]) => value === event.code);
    if (!key || !(event.metaKey || event.altKey || event.ctrlKey)) return;
    const shortcut = {control:event.ctrlKey, option:event.altKey, shift:event.shiftKey, command:event.metaKey, key:event.code};
    fillShortcutDraft(shortcut);
    recorder.value = SHORTCUT_MODIFIERS.filter(([name]) => shortcut[name]).map(([, symbol]) => symbol).join('') + key[1];
  };
  return recorder;
}
function shortcutEditor() {
  const form = document.createElement('form');
  form.id = 'shortcut-form';
  form.className = 'shortcut-editor';
  form.append(text('strong', t('顯示／收合快捷鍵')));
  const status = snapshot.shortcut_active
    ? t('目前使用 {shortcut}', {shortcut:snapshot.shortcut_label}) : t('目前未啟用，請選擇組合後重新套用');
  form.append(text('p', status, 'hint'));
  const controls = text('div', '', 'shortcut-controls');
  [['control','⌃ Control'],['option','⌥ Option'],['shift','⇧ Shift'],['command','⌘ Command']].forEach(([name, title]) => {
    const label = text('label', '', 'modifier');
    const input = document.createElement('input');
    Object.assign(input, {type:'checkbox', id:`shortcut-${name}`, checked:snapshot.settings.toggle_shortcut[name]});
    label.append(input, document.createTextNode(title));
    controls.append(label);
  });
  const keyLabel = text('label', t('搭配按鍵'), 'field');
  keyLabel.htmlFor = 'shortcut-key';
  const select = document.createElement('select');
  select.id = 'shortcut-key';
  const keys = shortcutKeys();
  keys.forEach(([value, label]) => {
    const option = text('option', label);
    option.value = value;
    select.append(option);
  });
  select.value = snapshot.settings.toggle_shortcut.key;
  const actions = submitRow(t('套用快捷鍵'));
  const reset = button(t('恢復預設'), () => {
    const shortcut = {control:false, option:false, shift:true, command:true, key:'Space'};
    // 已保存預設值時後端狀態不變，仍須清除表單尚未套用的草稿。
    fillShortcutDraft(shortcut);
    $('shortcut-recorder').value = '';
    send('set_shortcut', {shortcut});
  }, 'pill');
  reset.id = 'shortcut-reset';
  actions.prepend(reset);
  form.append(shortcutRecorder(keys), controls, keyLabel, select,
    text('p', t('至少選一個 ⌘、⌥ 或 ⌃。套用後立即生效，下次開啟也會保留。'), 'hint'), actions);
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
  $('overlay-title').textContent = t('設定');
  $('overlay-close').setAttribute('aria-label', t('關閉'));
  while (element.children.length > 1) element.lastChild.remove();
  const languageSelect = document.createElement('select');
  languageSelect.id = 'language-select';
  languageSelect.setAttribute('aria-label', t('介面語言'));
  [['en', 'English'], ['zh-TW', '繁體中文']].forEach(([value, label]) => {
    const option = text('option', label);
    option.value = value;
    languageSelect.append(option);
  });
  languageSelect.value = language;
  languageSelect.onchange = () => {
    const selected = languageSelect.value;
    // 等待後端保存成功的 snapshot，避免失敗時介面顯示未保存的語言。
    languageSelect.value = language;
    send('set_language', {language:selected});
  };
  element.append(row(t('介面語言'), t('切換後立即生效'), languageSelect));
  element.append(text('h3', t('視窗與顯示'), 'section-title'));
  const appearance = text('div', '', 'settings-group');
  appearance.append(row(t('側欄位置'), t('從螢幕的哪一側開啟'), sideControl(settings.side)));
  appearance.append(row(t('觸碰邊緣開啟'), t('游標停留片刻即可滑出'),
    toggle(t('觸碰邊緣開啟'), settings.hot_edge, 'hot_edge')));
  appearance.append(row(t('固定顯示'), t('游標移開時仍保留側欄'),
    toggle(t('固定顯示'), settings.pinned, 'pin')));
  element.append(appearance);
  const sleepSelect = document.createElement('select');
  sleepSelect.id = 'sleep-select';
  sleepSelect.setAttribute('aria-label', t('背景分頁休眠'));
  [0,5,15,30,60,120].forEach(minutes => {
    const option = text('option', minutes ? t('{minutes} 分鐘', {minutes}) : t('停用'));
    option.value = String(minutes);
    sleepSelect.append(option);
  });
  sleepSelect.value = String(settings.sleep_after_minutes ?? 15);
  sleepSelect.onchange = () => {
    const minutes = Number(sleepSelect.value);
    sleepSelect.value = String(snapshot.settings.sleep_after_minutes ?? 15);
    send('set_sleep', {minutes});
  };
  element.append(row(t('背景分頁休眠'), t('閒置後釋放記憶體；目前分頁保持運作。恢復時重新載入，未提交的輸入內容或即時狀態可能無法保留。'), sleepSelect));
  const widthLabel = text('label', t('側欄寬度'), 'field width-label');
  widthLabel.htmlFor = 'sidebar-width';
  const widthValue = text('output', `${settings.width} pt`);
  widthValue.htmlFor = 'sidebar-width';
  widthLabel.append(widthValue);
  element.append(widthLabel);
  const range = document.createElement('input');
  Object.assign(range, {id:'sidebar-width', type:'range', min:'360', max:'960', step:'20', value:settings.width});
  range.setAttribute('aria-label', t('側欄寬度'));
  range.oninput = () => { widthValue.textContent = `${range.value} pt`; };
  range.onchange = () => send('width', {width:Number(range.value)});
  element.append(range, row(t('視窗高度'), settings.height === null ? t('跟隨螢幕可用高度') : t('目前 {height} pt · 拖曳邊緣或下角調整', {height:Math.round(settings.height)}),
    button(t('恢復全高'), () => send('full_height'))));
  element.append(text('h3', t('快捷鍵'), 'section-title'), shortcutEditor());
  element.append(text('h3', t('管理網站 · {count} / 20', {count:settings.pads.length}), 'section-title'));
  settings.pads.forEach((pad, index) => element.append(padRow(pad, index, settings.pads.length)));
  if (!settings.pads.length) element.append(text('p', t('尚未加入網站，按左側 ＋ 開始。'), 'hint'));
  element.append(text('p', t('用箭頭調整順序，按編輯重新命名。移除後可在下方復原最近一個捷徑；登入資料不受影響。'), 'hint'));
  const importButton = button(t('從 Chrome 匯入書籤'), () => send('show_import'), 'pill');
  importButton.id = 'import-bookmarks';
  element.append(importButton);
  element.append(text('h3', t('其他快捷鍵'), 'section-title'));
  const shortcuts = text('div', '', 'shortcut-list');
  [[t('選取網址'),'⌘ L'],[t('新增網站'),'⌘ T'],[t('重新整理'),'⌘ R'],[t('上一頁／下一頁'),'⌘ [ / ⌘ ]'],[t('切換網站'),'⌘ 1–9'],[t('收合側欄'),'⌘ W'],[t('開啟設定'),'⌘ ,']].forEach(([name, keys]) => {
    const item = text('div', '', 'shortcut-row');
    item.append(text('span', name), text('kbd', keys));
    shortcuts.append(item);
  });
  element.append(shortcuts, button(t('結束 Open Slide Pad'), () => send('quit'), 'pill'));
  element.append(text('p', `Open Slide Pad ${APP_VERSION}`, 'version'));
}
function padInitial(pad) {
  return Array.from(pad.title.replace(/^www\./, '')).slice(0,2).join('').toUpperCase();
}
function siteDomain(address) {
  try { return new URL(address).hostname.replace(/^www\./, ''); }
  catch { return address; }
}
function renderHomePads(pads) {
  $('site-count').textContent = String(pads.length);
  $('empty-state').hidden = pads.length > 0;
  $('saved-sites').replaceChildren();
  pads.forEach((pad, index) => {
    const entry = button('', () => send('select', {id:pad.id}), 'saved-site');
    entry.setAttribute('aria-label', t('開啟 {name}', {name:pad.title}));
    const copy = text('div', '', 'site-info');
    copy.append(text('strong', pad.title), text('small', siteDomain(pad.url)));
    entry.append(text('span', padInitial(pad), 'site-monogram'), copy);
    if (index < 9) entry.append(text('span', `⌘ ${index + 1}`, 'site-key'));
    const arrow = text('span', '›', 'site-arrow');
    arrow.setAttribute('aria-hidden', 'true');
    entry.append(arrow);
    $('saved-sites').append(entry);
  });
}
function renderPads(state) {
  $('pads').replaceChildren();
  state.settings.pads.forEach((pad, index) => {
    const initial = padInitial(pad);
    const selected = !state.home && state.settings.active === pad.id;
    const sleeping = state.sleeping?.includes(pad.id);
    const element = button(initial, () => send('select', {id:pad.id}), `pad${selected ? ' active' : ''}${sleeping ? ' sleeping' : ''}`);
    element.title = pad.title + (sleeping ? ` · ${t('已休眠')}` : '') + (index < 9 ? ` · ⌘${index + 1}` : '');
    element.setAttribute('aria-label', pad.title);
    element.setAttribute('aria-pressed', String(selected));
    element.oncontextmenu = event => {
      event.preventDefault();
      send('pad_menu', {id:pad.id});
    };
    $('pads').append(element);
  });
}
function setReloadMode(stoppable) {
  const control = $('reload');
  const label = stoppable ? '停止載入' : '重新整理';
  control.dataset.action = stoppable ? 'stop' : 'reload';
  control.classList.toggle('stoppable', stoppable);
  // 同步 data-i18n 屬性，之後切換語言時 localizeDocument 才會翻成正確的標籤。
  ['title', 'aria-label'].forEach(attribute => {
    control.setAttribute(`data-i18n-${attribute}`, label);
    control.setAttribute(attribute, t(label));
  });
}
function renderLoadState(state) {
  const loading = !state.home && Boolean(state.loading);
  const failed = !state.home && Boolean(state.failure);
  setReloadMode(loading);
  $('progress').hidden = !loading;
  // 剛開始還沒有進度時也露出一小段，讓人知道已經在載入。
  const percent = Math.round(Math.max(state.progress || 0, 0.08) * 100);
  $('progress-bar').style.width = `${percent}%`;
  $('progress').setAttribute('aria-valuenow', String(percent));
  $('load-error').hidden = !failed;
  // 原頁面還在時才提供退路；首次載入失敗或程序終止沒有頁面可回。
  $('load-dismiss').hidden = !(failed && state.dismissible);
  if (!failed) return;
  const crashed = state.failure === 'crashed';
  $('load-error-title').textContent = crashed ? t('網頁意外停止') : t('無法載入這個頁面');
  $('load-error-detail').textContent = crashed
    ? t('這個網頁的處理程序已結束，重新載入即可繼續。') : t('請檢查網路連線或網址，然後再試一次。');
  $('load-error-address').textContent = state.address || '';
}
window.render = state => {
  const nextLanguage = state.settings.language || 'en';
  const languageChanged = language !== nextLanguage;
  if (languageChanged) {
    language = nextLanguage;
    localizeDocument();
    renderQuickSites();
  }
  const settingsChanged = JSON.stringify(snapshot?.settings) !== JSON.stringify(state.settings);
  const homeChanged = snapshot?.home !== state.home;
  const sleepingChanged = JSON.stringify(snapshot?.sleeping) !== JSON.stringify(state.sleeping);
  const shortcutChanged = snapshot?.shortcut_error !== state.shortcut_error || snapshot?.shortcut_active !== state.shortcut_active;
  if (!snapshot || Boolean(snapshot.settings.pads.length) !== Boolean(state.settings.pads.length)) {
    $('suggestions').open = state.settings.pads.length === 0;
  }
  snapshot = state;
  const settings = state.settings;
  document.documentElement.dataset.side = settings.side;
  if (settingsChanged || homeChanged || sleepingChanged) renderPads(state);
  if (settingsChanged) renderHomePads(settings.pads);
  $('home').hidden = !state.home;
  $('home-button').setAttribute('aria-pressed', String(state.home));
  $('pin').classList.toggle('active', settings.pinned);
  $('pin').setAttribute('aria-pressed', String(settings.pinned));
  $('back').disabled = state.home || !state.back;
  $('forward').disabled = state.home || !state.forward;
  if (document.activeElement !== $('address')) $('address').value = state.home ? '' : (state.address || '');
  $('status').classList.toggle('loading', !state.home && state.loading);
  renderLoadState(state);
  const label = state.home ? t('首頁') : state.failure ? t('載入失敗') : state.loading ? t('正在載入…') : state.title || t('準備就緒');
  $('status-text').replaceChildren(text('i', '', 'dot'), document.createTextNode(label));
  $('undo-remove').hidden = !state.undo_title;
  $('undo-remove').title = state.undo_title ? t('復原「{name}」', {name:state.undo_title}) : '';
  $('status-shortcut').hidden = state.home || Boolean(state.undo_title);
  $('status-shortcut').textContent = state.shortcut_active ? state.shortcut_label : t('快捷鍵未啟用');
  $('home-shortcut').textContent = state.shortcut_active ? state.shortcut_label : t('選單列圖示');
  if ((settingsChanged || shortcutChanged) && overlayMode === 'settings') {
    const languageFocused = document.activeElement?.id === 'language-select';
    renderSettings($('overlay'));
    if (languageFocused) $('language-select').focus();
  }
};
document.querySelectorAll('[data-action]').forEach(element => {
  element.onclick = () => send(element.dataset.action);
});
$('add').onclick = $('hero-add').onclick = () => send('new_pad');
$('empty-import').onclick = () => send('show_import');
$('settings').onclick = () => send('show_settings');
$('toast-close').onclick = hideToast;
// 滑鼠停在提示上時不倒數，避免訊息還沒讀完就消失。
$('toast').onmouseenter = () => clearTimeout(toastTimer);
$('toast').onmouseleave = armToastTimer;
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
function renderQuickSites() {
  $('quick').replaceChildren();
  [['Gmail',t('電子郵件'),'https://mail.google.com','M'],['Notion',t('筆記與文件'),'https://www.notion.so','N'],['ChatGPT',t('對話與搜尋'),'https://chatgpt.com','C'],['YouTube',t('影片與音樂'),'https://www.youtube.com','▶']].forEach(([name, description, address, mark]) => {
    const element = button('', () => send('add', {address}), 'quick');
    element.setAttribute('aria-label', t('加入 {name}', {name}));
    const copy = text('div', '', 'quick-copy');
    copy.append(text('strong', name), text('small', description));
    element.append(text('span', mark, 'quick-logo'), copy, text('span', '+', 'quick-add'));
    $('quick').append(element);
  });
}
localizeDocument();
renderQuickSites();
$('app-version').textContent = APP_VERSION;
// Cmd 快捷鍵由原生選單統一處理，避免同一個按鍵重複送出命令。
document.addEventListener('keydown', event => {
  if (event.key === 'Escape') overlayMode ? dismiss() : send('hide');
});
send('ready');
