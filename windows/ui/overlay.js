// Recording overlay. Polls `overlay_state` (codes and numbers only; no
// transcript or model output ever reaches this window).
(function () {
  'use strict';
  var invoke = window.__TAURI__.core.invoke;
  var $ = function (id) { return document.getElementById(id); };
  var MODE = { raw: '原文', light: '標點', polish: '潤稿', formal: '正式' };
  var NOTICE = {
    translation_needs_api: '翻譯需要 API，已輸出原文',
    no_key: '未設定金鑰', key_invalid: '金鑰無效，請重新輸入', hard_limit: '已達本月預算硬上限',
    budget_exhausted: '帳號額度已用盡', network: '無法連線', timeout: '連線逾時',
    rate_limited: '請求太多，請稍後再試', api_error: 'API 錯誤', guard_rejected: '潤稿結果未通過檢查',
    mic_none: '找不到麥克風', mic_unavailable: '無法使用麥克風（請檢查隱私權設定）', mic_format: '不支援的麥克風格式',
    no_model: '尚未下載語音模型（設定 → 模型）', stt: '語音辨識失敗', stt_unavailable: '此版本未內建語音辨識',
    audio_format: '錄音格式錯誤', empty: '沒有聽到聲音', insert_failed: '無法插入文字',
    insert_unavailable: '無法插入文字'
  };
  var BARS = 12;
  var bars = $('bars');
  for (var i = 0; i < BARS; i++) bars.appendChild(document.createElement('i'));
  var history = [];

  function show(el, on) { el.hidden = !on; }
  function fmt(ms) { var s = Math.floor(ms / 1000); return Math.floor(s / 60) + ':' + ('0' + s % 60).slice(-2); }

  function render(v) {
    var hud = $('hud');
    if (v.phase === 'hidden') { hud.hidden = true; history = []; return; }
    hud.hidden = false;
    var rec = v.phase === 'recording', busy = v.phase === 'transcribing' || v.phase === 'polishing';
    show($('dot'), rec); show(bars, rec); show($('time'), rec);
    show($('spin'), busy);
    show($('okc'), v.phase === 'inserted');
    show($('warn'), v.phase === 'fallback' || v.phase === 'error');
    var label = '', hint = '';
    if (rec) {
      history.push(Math.min(1, v.level * 4));
      if (history.length > BARS) history.shift();
      var items = bars.children;
      for (var j = 0; j < BARS; j++) {
        var lv = history[history.length - BARS + j] || 0;
        items[j].style.height = (6 + Math.round(lv * 16)) + 'px';
      }
      $('time').textContent = fmt(v.elapsed_ms);
      $('time').classList.toggle('cap', v.at_cap);
      label = v.translate ? '潤飾並翻譯' : '';
      hint = v.at_cap ? '已達錄音上限，請停止' : (v.stop_hint + ' 停止・Esc 取消');
    } else if (v.phase === 'transcribing') {
      label = '轉寫中…';
    } else if (v.phase === 'polishing') {
      label = v.translate ? '潤稿並翻譯中…' : '潤稿中…';
    } else if (v.phase === 'inserted') {
      label = '已插入・' + (v.translate ? '已翻譯' : MODE[v.mode] || '');
    } else if (v.phase === 'fallback') {
      if (v.notice === 'translation_needs_api') { label = NOTICE.translation_needs_api; }
      else {
        label = v.translate ? '翻譯失敗，已插入原文' : '潤稿失敗，已插入原文';
        hint = NOTICE[v.notice] || '';
      }
    } else if (v.phase === 'error') {
      label = NOTICE[v.notice] || '發生錯誤';
    }
    $('label').textContent = label;
    show($('label'), label !== '');
    $('hint').textContent = hint;
    show($('hint'), hint !== '');
  }

  function tick() {
    invoke('overlay_state').then(function (v) {
      render(v);
      setTimeout(tick, v.phase === 'hidden' ? 400 : 100);
    }, function () { setTimeout(tick, 1000); });
  }
  tick();
})();
