// Adds a link to the same page in the other language to the menu bar.
// English pages live under /docs/, Turkish pages under /tr/docs/, with the
// same file names.
(function () {
  'use strict';
  var bar = document.querySelector('.right-buttons');
  if (!bar) return;
  var path = window.location.pathname;
  var tr = /\/tr\/docs\//.test(path);
  var target = tr ? path.replace(/\/tr\/docs\//, '/docs/') : path.replace(/\/docs\//, '/tr/docs/');
  if (target === path) return;
  var a = document.createElement('a');
  a.className = 'hs-lang';
  a.href = target + window.location.hash;
  a.hreflang = tr ? 'en' : 'tr';
  a.lang = tr ? 'en' : 'tr';
  a.textContent = tr ? 'English' : 'Türkçe';
  a.title = tr ? 'Read this page in English' : 'Bu sayfayı Türkçe okuyun';
  bar.insertBefore(a, bar.firstChild);
})();
