'use strict';
// 英文譯文與 Rust 共用；繁體中文來源字串即為 zh-TW 譯文。
const englishMessages = /* SLIDERUST_ENGLISH */;
let language = 'en';
function t(source, parameters = {}) {
  const template = language === 'zh-TW' ? source : (englishMessages[source] ?? source);
  return template.replace(/\{(\w+)\}/g, (match, key) => String(parameters[key] ?? match));
}
function localizeDocument() {
  document.documentElement.lang = language;
  document.querySelectorAll('[data-i18n]').forEach(element => {
    element.textContent = t(element.dataset.i18n);
  });
  ['title', 'aria-label', 'placeholder'].forEach(attribute => {
    document.querySelectorAll(`[data-i18n-${attribute}]`).forEach(element => {
      element.setAttribute(attribute, t(element.getAttribute(`data-i18n-${attribute}`)));
    });
  });
}
