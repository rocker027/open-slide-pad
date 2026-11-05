'use strict';
// 版本以 Cargo.toml 為唯一來源，由 Rust 在組合文件時代入。
const APP_VERSION = '/* SLIDERUST_VERSION */';
const $ = id => document.getElementById(id);
let snapshot = null;
let overlayMode = null;
let toastTimer;
let returnFocus = null;
let settingsSection = 'general';
let settingsView = null;
const MAX_SITES = 20;

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
  if ($('toast').contains(document.activeElement) || $('toast').matches(':hover')) return;
  toastTimer = setTimeout(hideToast, TOAST_DURATION_MS);
}
window.showToast = message => {
  $('toast-text').textContent = message;
  $('toast').hidden = false;
  const feedback = $('form-feedback');
  if (feedback) {
    feedback.textContent = message;
    feedback.hidden = false;
  }
  armToastTimer();
};
function setBackgroundInert(inert) {
  document.querySelectorAll('.rail, .toolbar, .stage, .status, .resize-grip').forEach(element => { element.inert = inert; });
}
window.closeOverlay = () => {
  if ($('overlay').contains(document.activeElement)) document.activeElement.blur();
  overlayMode = null;
  $('overlay').hidden = true;
  setBackgroundInert(false);
  returnFocus = null;
  settingsView = null;
  settingsSection = 'general';
};
function dismiss() {
  const target = returnFocus;
  window.closeOverlay();
  send('overlay', {open:false});
  const control = target?.element?.isConnected ? target.element : $(target?.id);
  (control && !control.disabled && control.tabIndex >= 0 && !control.closest('[hidden]') ? control : $('home-button')).focus();
}
window.focusAddress = () => {
  window.closeOverlay();
  $('address').focus();
  $('address').select();
};
function overlay(title, mode) {
  if (overlayMode === 'settings') settingsView = captureSettingsView();
  if (!overlayMode) {
    const active = document.activeElement;
    returnFocus = {element:active, id:active?.id};
  }
  overlayMode = mode;
  send('overlay', {open:true});
  const element = $('overlay');
  element.replaceChildren();
  element.hidden = false;
  element.dataset.mode = mode;
  element.scrollTop = 0;
  setBackgroundInert(true);
  const header = text('div', '', 'overlay-header');
  const close = button('✕', dismiss, 'icon');
  close.id = 'overlay-close';
  close.setAttribute('aria-label', t('關閉'));
  const heading = text('h2', title);
  heading.id = 'overlay-title';
  header.append(heading, close);
  element.append(header);
  close.focus();
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
function formFeedback(input) {
  const feedback = text('p', '', 'inline-feedback');
  feedback.id = 'form-feedback';
  feedback.hidden = true;
  feedback.setAttribute('role', 'status');
  input.setAttribute('aria-describedby', feedback.id);
  input.addEventListener('input', () => { feedback.hidden = true; });
  return feedback;
}
window.showAddForm = () => {
  const fromSettings = overlayMode === 'settings';
  const element = overlay(t('新增網站'), 'add');
  if ((snapshot?.settings.pads.length || 0) >= MAX_SITES) {
    element.append(text('p', t('已達 20 個網站上限。請先到管理網站移除不需要的網站。'), 'hint'),
      button(t('管理網站'), () => window.showSettings('sites'), 'primary'));
    return;
  }
  element.append(text('p', t('貼上網站網址，加入後會立刻開啟。下次可從首頁找到。'), 'hint'));
  const form = document.createElement('form');
  const {fieldLabel, input} = inputField('new-address', t('網址或搜尋內容'));
  input.placeholder = t('例如 youtube.com');
  const actions = submitRow(t('加入並開啟'));
  const cancel = button(fromSettings ? t('返回管理網站') : t('取消'),
    () => fromSettings ? window.showSettings('sites') : dismiss(), 'pill form-cancel');
  cancel.id = 'add-cancel';
  actions.prepend(cancel);
  form.append(fieldLabel, input,
    text('p', t('沒有網址？輸入關鍵字，會加入並開啟 DuckDuckGo 搜尋結果。'), 'hint'), formFeedback(input), actions);
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
  form.append(fieldLabel, input, count, text('p', pad.url, 'hint address-hint'), formFeedback(input), submitRow(t('儲存名稱')));
  form.onsubmit = event => {
    event.preventDefault();
    send('rename', {id:pad.id, title:input.value});
  };
  element.append(form, button(t('← 返回設定'), () => window.showSettings('sites'), 'text-button'));
  input.focus();
  input.select();
}
window.showSettings = (section = settingsSection) => {
  const element = overlay(t('設定'), 'settings');
  settingsSection = section;
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
function bindImportLimit(rows, entries, remaining, counter, submit, hiddenCount) {
  const refresh = () => {
    const count = selectedIndices(rows).length;
    counter.textContent = remaining
      ? t('已選 {count} · 還可加入 {remaining}', {count, remaining:remaining - count})
      : t('側欄已滿，請先移除網站再匯入。');
    rows.forEach(({box}, index) => { if (!entries[index].added) box.disabled = !box.checked && count >= remaining; });
    submit.disabled = count === 0;
    const hidden = rows.filter(({row, box}) => row.hidden && box.checked && !entries[Number(box.value)].added).length;
    hiddenCount.hidden = hidden === 0;
    hiddenCount.textContent = hidden ? t(' · 隱藏 {count}', {count:hidden}) : '';
    hiddenCount.title = t('其中 {count} 個已選書籤不在目前篩選結果中。', {count:hidden});
    counter.append(hiddenCount);
  };
  rows.forEach(({box}) => { box.onchange = refresh; });
  refresh();
  return refresh;
}
// 篩選只隱藏列，藏起來的勾選仍算數。
function bindImportFilter(input, rows, entries, empty, refresh) {
  input.oninput = () => {
    const needle = input.value.trim().toLowerCase();
    rows.forEach(({row}, index) => {
      row.hidden = Boolean(needle) && !entries[index].title.toLowerCase().includes(needle) && !entries[index].url.toLowerCase().includes(needle);
    });
    empty.hidden = rows.some(({row}) => !row.hidden);
    refresh();
  };
}
// 候選清單由 Rust 一次推送，不進 render 的狀態；送回的只有索引，避免超過 IPC 訊息上限。
window.showImport = ({entries, remaining}) => {
  const fromSettings = overlayMode === 'settings';
  const element = overlay(t('從 Chrome 匯入書籤'), 'import');
  const back = button(fromSettings ? t('← 返回設定') : t('← 返回首頁'), () => {
    if (fromSettings) {
      send('overlay', {open:false});
      window.showSettings('sites');
    } else dismiss();
  }, 'text-button');
  back.id = 'import-back';
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
  const hiddenCount = text('span', '');
  hiddenCount.id = 'import-hidden-count';
  const empty = text('p', t('找不到符合的書籤，請試試其他名稱或網址。'), 'hint');
  empty.id = 'import-no-results';
  empty.hidden = true;
  empty.setAttribute('role', 'status');
  const list = text('div', '', 'import-list');
  list.id = 'import-list';
  const rows = entries.map(importRow);
  rows.forEach(({row}) => list.append(row));
  const results = text('div', '', 'import-results');
  results.id = 'import-results';
  results.append(empty, list);
  const actions = submitRow(t('加入所選'));
  actions.lastChild.id = 'import-submit';
  const refresh = bindImportLimit(rows, entries, remaining, counter, actions.lastChild, hiddenCount);
  actions.prepend(back);
  bindImportFilter(input, rows, entries, empty, refresh);
  form.onsubmit = event => {
    event.preventDefault();
    send('import', {indices:selectedIndices(rows)});
  };
  form.append(fieldLabel, input, counter, results, actions);
  element.append(form);
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
  control.id = `setting-${action}`;
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
  const entry = text('div', '', 'site-row managed-site');
  const info = text('div', '', 'site-info');
  info.append(text('strong', pad.title), text('small', pad.url));
  const actions = text('div', '', 'site-actions');
  const up = button('↑', () => send('move', {id:pad.id, position:index - 1}), 'mini-button');
  const down = button('↓', () => send('move', {id:pad.id, position:index + 1}), 'mini-button');
  up.disabled = index === 0;
  down.disabled = index === total - 1;
  up.setAttribute('aria-label', t('將 {name} 向上移', {name:pad.title}));
  down.setAttribute('aria-label', t('將 {name} 向下移', {name:pad.title}));
  const rename = button(t('重新命名'), () => editPad(pad), 'mini-button');
  rename.setAttribute('aria-label', t('重新命名 {name}', {name:pad.title}));
  const remove = button(t('移除'), () => send('remove', {id:pad.id}), 'mini-button danger');
  [[up, 'up'], [down, 'down'], [rename, 'rename'], [remove, 'remove']].forEach(([control, name]) => { control.id = `site-${pad.id}-${name}`; });
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
  actions.lastChild.id = 'shortcut-submit';
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
function captureSettingsView() {
  const form = $('shortcut-form');
  if (!form || !$('overlay').contains(form)) return settingsView;
  const draft = {key:$('shortcut-key').value};
  SHORTCUT_MODIFIERS.forEach(([name]) => { draft[name] = $(`shortcut-${name}`).checked; });
  const active = document.activeElement;
  return {
    draft, recorder:$('shortcut-recorder').value,
    sourceShortcut:JSON.stringify(snapshot.settings.toggle_shortcut),
    moreOpen:Boolean($('more-settings')?.open),
    focusId:$('overlay').contains(active) ? active.id : null,
    scrollTop:$('overlay').scrollTop,
  };
}
function selectSettingsSection(section, focus = false) {
  settingsSection = ['general', 'sites', 'shortcuts'].includes(section) ? section : 'general';
  ['general', 'sites', 'shortcuts'].forEach(name => {
    const selected = name === settingsSection;
    $(`settings-${name}`).hidden = !selected;
    const tab = $(`settings-tab-${name}`);
    tab.setAttribute('aria-selected', String(selected));
    tab.tabIndex = selected ? 0 : -1;
  });
  if (focus) {
    $('overlay').scrollTop = 0;
    $(`settings-tab-${settingsSection}`).focus();
  }
}
function settingsNavigation() {
  const nav = text('div', '', 'settings-nav');
  nav.setAttribute('role', 'tablist');
  nav.setAttribute('aria-label', t('設定分類'));
  const sections = [['general', t('一般')], ['sites', t('網站')], ['shortcuts', t('快捷鍵')]];
  sections.forEach(([name, label], index) => {
    const tab = button(label, () => selectSettingsSection(name, true), 'settings-tab');
    tab.id = `settings-tab-${name}`;
    tab.setAttribute('role', 'tab');
    tab.setAttribute('aria-controls', `settings-${name}`);
    tab.onkeydown = event => {
      let next;
      if (event.key === 'ArrowRight') next = (index + 1) % sections.length;
      if (event.key === 'ArrowLeft') next = (index + sections.length - 1) % sections.length;
      if (event.key === 'Home') next = 0;
      if (event.key === 'End') next = sections.length - 1;
      if (next === undefined) return;
      event.preventDefault();
      selectSettingsSection(sections[next][0], true);
    };
    nav.append(tab);
  });
  return nav;
}
function settingsPanel(name) {
  const panel = text('section', '', 'settings-panel');
  panel.id = `settings-${name}`;
  panel.setAttribute('role', 'tabpanel');
  panel.setAttribute('aria-labelledby', `settings-tab-${name}`);
  return panel;
}
function restoreSettingsView() {
  selectSettingsSection(settingsSection);
  if (!settingsView) {
    $(`settings-tab-${settingsSection}`).focus();
    return;
  }
  $('more-settings').open = settingsView.moreOpen;
  // 只有快捷鍵本身成功改變才採用新值；其他偏好更新不能沖掉未套用草稿。
  if (settingsView.sourceShortcut === JSON.stringify(snapshot.settings.toggle_shortcut)) {
    fillShortcutDraft(settingsView.draft);
    $('shortcut-recorder').value = settingsView.recorder;
  }
  const active = $(settingsView.focusId);
  if (active && !active.disabled && active.getClientRects().length) active.focus();
  else if (settingsView.focusId !== null) $(`settings-tab-${settingsSection}`).focus();
  $('overlay').scrollTop = settingsView.scrollTop;
}
function renderSettings(element) {
  if (!snapshot) return;
  const settings = snapshot.settings;
  $('overlay-title').textContent = t('設定');
  $('overlay-close').setAttribute('aria-label', t('關閉'));
  while (element.children.length > 1) element.lastChild.remove();
  const general = settingsPanel('general');
  const sites = settingsPanel('sites');
  const shortcutsPanel = settingsPanel('shortcuts');
  element.append(settingsNavigation(), general, sites, shortcutsPanel);
  general.append(text('p', t('變更會自動儲存。依照你的習慣調整即可。'), 'hint'));
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
  general.append(row(t('介面語言'), t('切換後立即生效'), languageSelect));
  general.append(text('h3', t('視窗與顯示'), 'section-title'));
  const appearance = text('div', '', 'settings-group');
  appearance.append(row(t('側欄位置'), t('從螢幕的哪一側開啟'), sideControl(settings.side)));
  appearance.append(row(t('觸碰邊緣開啟'), t('游標停留片刻即可滑出'),
    toggle(t('觸碰邊緣開啟'), settings.hot_edge, 'hot_edge')));
  appearance.append(row(t('固定顯示'), t('游標移開時仍保留側欄'),
    toggle(t('固定顯示'), settings.pinned, 'pin')));
  general.append(appearance);
  const more = text('details', '', 'more-settings');
  more.id = 'more-settings';
  const summary = text('summary', t('更多設定：尺寸與休眠'));
  summary.id = 'more-settings-toggle';
  more.append(summary);
  general.append(more);
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
  more.append(row(t('背景分頁休眠'), t('閒置後釋放記憶體；目前分頁保持運作。恢復時重新載入，未提交的輸入內容或即時狀態可能無法保留。'), sleepSelect));
  const widthLabel = text('label', t('側欄寬度'), 'field width-label');
  widthLabel.htmlFor = 'sidebar-width';
  const widthValue = text('output', `${settings.width} pt`);
  widthValue.htmlFor = 'sidebar-width';
  widthLabel.append(widthValue);
  more.append(widthLabel);
  const range = document.createElement('input');
  Object.assign(range, {id:'sidebar-width', type:'range', min:'360', max:'960', step:'20', value:settings.width});
  range.setAttribute('aria-label', t('側欄寬度'));
  range.oninput = () => { widthValue.textContent = `${range.value} pt`; };
  range.onchange = () => send('width', {width:Number(range.value)});
  const fullHeight = button(t('恢復全高'), () => send('full_height'));
  fullHeight.id = 'full-height';
  more.append(range, row(t('視窗高度'), settings.height === null ? t('跟隨螢幕可用高度') : t('目前 {height} pt · 拖曳邊緣或下角調整', {height:Math.round(settings.height)}), fullHeight));
  shortcutsPanel.append(shortcutEditor());
  sites.append(text('h3', t('管理網站 · {count} / 20', {count:settings.pads.length}), 'section-title'));
  const siteActions = text('div', '', 'site-management-actions');
  const add = button(t('新增網站'), () => send('new_pad'), 'primary');
  add.id = 'settings-add-site';
  add.disabled = settings.pads.length >= MAX_SITES;
  const importButton = button(t('從 Chrome 匯入書籤'), () => send('show_import'), 'pill');
  importButton.id = 'import-bookmarks';
  importButton.disabled = settings.pads.length >= MAX_SITES;
  siteActions.append(add, importButton);
  sites.append(siteActions);
  settings.pads.forEach((pad, index) => sites.append(padRow(pad, index, settings.pads.length)));
  if (!settings.pads.length) sites.append(text('p', t('尚未加入網站，按上方「新增網站」開始。'), 'hint'));
  sites.append(text('p', t('用箭頭調整順序。移除後可在這裡復原最近一個網站；登入資料不受影響。'), 'hint'));
  // 對話框開啟時背景是 inert，復原必須在管理區也有入口。
  if (snapshot.undo_title) {
    const undo = button(t('復原「{name}」', {name:snapshot.undo_title}), () => send('undo_remove'), 'pill');
    undo.id = 'settings-undo';
    sites.append(undo);
  }
  shortcutsPanel.append(text('h3', t('其他快捷鍵'), 'section-title'));
  const shortcuts = text('div', '', 'shortcut-list');
  [[t('選取網址'),'⌘ L'],[t('新增網站'),'⌘ T'],[t('重新整理'),'⌘ R'],[t('上一頁／下一頁'),'⌘ [ / ⌘ ]'],[t('切換網站'),'⌘ 1–9'],[t('收合側欄'),'⌘ W'],[t('開啟設定'),'⌘ ,']].forEach(([name, keys]) => {
    const item = text('div', '', 'shortcut-row');
    item.append(text('span', name), text('kbd', keys));
    shortcuts.append(item);
  });
  shortcutsPanel.append(shortcuts);
  const quit = button(t('結束 Open Slide Pad'), () => send('quit'), 'pill');
  quit.id = 'settings-quit';
  general.append(quit);
  general.append(text('p', `Open Slide Pad ${APP_VERSION}`, 'version'));
  restoreSettingsView();
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
  $('home-tools').hidden = pads.length === 0;
  if (!pads.length) $('site-filter').value = '';
  $('saved-sites').replaceChildren();
  pads.forEach((pad, index) => {
    const entry = button('', () => send('select', {id:pad.id}), 'saved-site');
    entry.id = `site-${pad.id}`;
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
  filterHomePads();
}
function filterHomePads() {
  const needle = $('site-filter').value.trim().toLowerCase();
  let visible = 0;
  (snapshot?.settings.pads || []).forEach(pad => {
    const hidden = !`${pad.title} ${pad.url}`.toLowerCase().includes(needle);
    $(`site-${pad.id}`).hidden = hidden;
    if (!hidden) visible += 1;
  });
  $('site-no-results').hidden = !snapshot?.settings.pads.length || visible > 0;
}
function renderHomeGuidance(state) {
  const settings = state.settings;
  const empty = settings.pads.length === 0;
  $('home-title').textContent = empty ? t('加入你的第一個網站') : t('我的網站');
  $('home-lead').textContent = empty ? t('把常用網站放在這裡，隨時打開。') : t('點一下網站就能開啟。');
  $('site-count').hidden = empty;
  $('site-limit').hidden = settings.pads.length < MAX_SITES;
  $('site-limit').textContent = t('已加入 20 個網站。要加入新的，請先到「管理網站」移除一個。');
  $('home-open-help').textContent = settings.hot_edge
    ? (settings.side === 'left' ? t('移到螢幕左邊緣，就能再次打開。') : t('移到螢幕右邊緣，就能再次打開。'))
    : t('邊緣開啟已關閉。點選單列的 Open Slide Pad 圖示即可打開。');
  $('home-hide-help').textContent = settings.pinned
    ? t('目前固定顯示，游標移開也不會收起。') : t('游標移開會自動收起。想讓側欄留在畫面上，可以固定顯示。');
  $('home-pin').textContent = settings.pinned ? t('取消固定顯示') : t('固定顯示側欄');
  $('home-pin').setAttribute('aria-pressed', String(settings.pinned));
}
function renderPads(state) {
  $('pads').replaceChildren();
  state.settings.pads.forEach((pad, index) => {
    const initial = padInitial(pad);
    const selected = !state.home && state.settings.active === pad.id;
    const sleeping = state.sleeping?.includes(pad.id);
    const element = button(initial, () => send('select', {id:pad.id}), `pad${selected ? ' active' : ''}${sleeping ? ' sleeping' : ''}`);
    element.id = `pad-${pad.id}`;
    element.title = pad.title + (sleeping ? ` · ${t('已休眠')}` : '') + (index < 9 ? ` · ⌘${index + 1}` : '');
    element.setAttribute('aria-label', pad.title + (sleeping ? ` · ${t('已休眠')}` : ''));
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
  }
  const settingsChanged = JSON.stringify(snapshot?.settings) !== JSON.stringify(state.settings);
  const homeChanged = snapshot?.home !== state.home;
  const sleepingChanged = JSON.stringify(snapshot?.sleeping) !== JSON.stringify(state.sleeping);
  const shortcutChanged = snapshot?.shortcut_error !== state.shortcut_error || snapshot?.shortcut_active !== state.shortcut_active;
  const undoChanged = snapshot?.undo_title !== state.undo_title;
  if (overlayMode === 'settings') settingsView = captureSettingsView();
  if (!snapshot || Boolean(snapshot.settings.pads.length) !== Boolean(state.settings.pads.length)) {
    $('suggestions').open = state.settings.pads.length === 0;
  }
  snapshot = state;
  const settings = state.settings;
  document.documentElement.dataset.side = settings.side;
  if (settingsChanged || homeChanged || sleepingChanged) renderPads(state);
  if (settingsChanged) renderHomePads(settings.pads);
  if (settingsChanged || languageChanged) renderHomeGuidance(state);
  if (settingsChanged || languageChanged) renderQuickSites();
  const full = settings.pads.length >= MAX_SITES;
  $('add').disabled = $('hero-add').disabled = full;
  const addTitle = full ? '已達 20 個網站上限' : '新增網站 ⌘T';
  $('add').setAttribute('data-i18n-title', addTitle);
  $('add').title = t(addTitle);
  $('hero-add').title = full ? t(addTitle) : t('新增網站');
  $('home').hidden = !state.home;
  $('home-button').setAttribute('aria-pressed', String(state.home));
  $('pin').classList.toggle('active', settings.pinned);
  $('pin').setAttribute('aria-pressed', String(settings.pinned));
  $('back').disabled = state.home || !state.back;
  $('forward').disabled = state.home || !state.forward;
  $('reload').disabled = state.home;
  $('external').disabled = state.home;
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
  if ((settingsChanged || shortcutChanged || undoChanged) && overlayMode === 'settings') {
    renderSettings($('overlay'));
  }
};
document.querySelectorAll('[data-action]').forEach(element => {
  element.onclick = () => send(element.dataset.action);
});
$('add').onclick = $('hero-add').onclick = () => send('new_pad');
$('empty-import').onclick = () => send('show_import');
$('settings').onclick = () => send('show_settings');
$('manage-sites').onclick = () => {
  settingsSection = 'sites';
  send('show_settings');
};
$('site-filter').oninput = filterHomePads;
$('clear-site-filter').onclick = () => {
  $('site-filter').value = '';
  filterHomePads();
  $('site-filter').focus();
};
$('home-pin').onclick = () => send('pin');
$('toast-close').onclick = hideToast;
// 滑鼠停在提示上時不倒數，避免訊息還沒讀完就消失。
$('toast').onmouseenter = () => clearTimeout(toastTimer);
$('toast').onmouseleave = armToastTimer;
$('toast').onfocusin = () => clearTimeout(toastTimer);
$('toast').onfocusout = event => {
  if (!$('toast').contains(event.relatedTarget)) {
    clearTimeout(toastTimer);
    toastTimer = setTimeout(armToastTimer, 0);
  }
};
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
function canonicalSite(address) {
  try { const url = new URL(address); return url.origin + url.pathname.replace(/\/$/, '') + url.search; }
  catch { return address; }
}
function renderQuickSites() {
  $('quick').replaceChildren();
  [['Gmail',t('電子郵件'),'https://mail.google.com','M'],['Notion',t('筆記與文件'),'https://www.notion.so','N'],['ChatGPT',t('對話與搜尋'),'https://chatgpt.com','C'],['YouTube',t('影片與音樂'),'https://www.youtube.com','▶']].forEach(([name, description, address, mark]) => {
    const existing = snapshot?.settings.pads.find(pad => canonicalSite(pad.url) === canonicalSite(address));
    const element = button('', () => existing ? send('select', {id:existing.id}) : send('add', {address}), 'quick');
    element.disabled = !existing && (snapshot?.settings.pads.length || 0) >= MAX_SITES;
    element.setAttribute('aria-label', existing ? t('開啟 {name}', {name}) : t('加入 {name}', {name}));
    element.title = existing ? t('已加入') : element.disabled ? t('已達 20 個網站上限') : t('加入 {name}', {name});
    const copy = text('div', '', 'quick-copy');
    copy.append(text('strong', name), text('small', existing ? t('已加入 · 點一下開啟') : description));
    element.append(text('span', mark, 'quick-logo'), copy, text('span', existing ? '✓' : '+', 'quick-add'));
    $('quick').append(element);
  });
}
localizeDocument();
renderQuickSites();
$('app-version').textContent = APP_VERSION;
// Cmd 快捷鍵由原生選單統一處理，避免同一個按鍵重複送出命令。
document.addEventListener('keydown', event => {
  if (event.key === 'Escape') {
    event.preventDefault();
    overlayMode ? dismiss() : send('hide');
  }
  if (event.key === 'Tab' && overlayMode) {
    const controls = Array.from($('overlay').querySelectorAll('button, input, select, summary, [tabindex]'))
      .filter(element => !element.disabled && element.tabIndex >= 0 && element.getClientRects().length);
    const first = controls[0];
    const last = controls.at(-1);
    const active = document.activeElement;
    if (!controls.length) { event.preventDefault(); $('overlay').focus(); }
    else if (event.shiftKey && (active === first || !controls.includes(active))) { event.preventDefault(); last.focus(); }
    else if (!event.shiftKey && (active === last || !controls.includes(active))) { event.preventDefault(); first.focus(); }
  }
});
send('ready');
