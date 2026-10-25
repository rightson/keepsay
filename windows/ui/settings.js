// keepsay settings page (docs/mockups/desktop.html, Windows card layout).
//
// The key never passes through this page: "輸入金鑰…" calls `enter_key`,
// which opens a native Win32 dialog from Rust and returns only the
// fingerprint. All DOM text from settings (glossary) is set with
// textContent, never innerHTML.
(function () {
  'use strict';
  var invoke = window.__TAURI__.core.invoke;

  var PANES = [['general', '一般'], ['key', '金鑰'], ['glossary', '詞彙表'], ['budget', '預算'], ['model', '模型'], ['privacy', '隱私']];
  var MODES = [['raw', '原文'], ['light', '標點'], ['polish', '潤稿'], ['formal', '正式']];
  var MODE_DESC = { raw: '只轉寫，不連網', light: '補標點、分段，不改字', polish: '修順語句，保留原意', formal: '改成正式書面語' };
  var LANGS = [['en', 'English'], ['zh-TW', '繁體中文（台灣）'], ['ja', '日本語']];
  var ERR = {
    cancelled: '', busy: '已有一個視窗正在進行中', dialog_failed: '無法開啟金鑰對話框',
    key_format: '金鑰格式不正確（應以 sk-ant- 開頭）', key_invalid: '金鑰無效（驗證失敗），未儲存',
    network: '無法連線到 api.anthropic.com，未儲存', timeout: '連線逾時，未儲存', rate_limited: '請求太多，請稍後再試',
    vault_unavailable: '無法使用 Windows 認證管理員', budget_exhausted: '帳號額度已用盡',
    hotkey_syntax: '快捷鍵格式不正確', hotkey_needs_alt_or_ctrl: '快捷鍵需要 Alt 或 Ctrl',
    hotkey_ctrl_alt: '不能用 Ctrl + Alt（等同 AltGr）', hotkey_win: '不能用 Win 鍵', hotkey_reserved: 'Esc 保留給取消',
    hotkey_duplicate: '兩個快捷鍵不能相同', glossary_entry_invalid: '詞彙太長或含控制字元', glossary_too_long: '詞彙表最多 200 筆',
    config: '設定值不正確', io: '無法寫入設定檔', model_hash: 'SHA-256 不符，已刪除', model_too_large: '檔案大小不符，已中止',
    model_unknown: '未知的模型', disallowed_host: '下載來源不在允許清單內'
  };
  var GUIDE = [
    ['開專用 workspace', '，只給 keepsay 用。'],
    ['不綁付款方式', '、關閉自動儲值。'],
    ['設定花費上限', '（Console → Billing → Spend limits）。'],
    ['撤銷', '：Console → API Keys → 停用或刪除。']
  ];

  var S = null, key = { has_key: false, fingerprint: null }, budget = null, models = [], hk = null, dl = null;
  var pane = 'general', gSel = -1, msg = {}, dlTimer = null;

  // ---------- tiny DOM helpers (textContent only) ----------
  function el(tag, cls, text) {
    var e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text != null) e.textContent = text;
    return e;
  }
  function add(parent) { for (var i = 1; i < arguments.length; i++) if (arguments[i]) parent.appendChild(arguments[i]); return parent; }
  function btn(text, cls, onclick) { var b = el('button', 'btn' + (cls ? ' ' + cls : ''), text); b.type = 'button'; b.onclick = onclick; return b; }
  function card(title, sub, right) {
    var c = el('div', 'card'), g = el('div', 'grow', title);
    if (sub) add(g, el('div', 'sub', sub));
    var r = el('div', 'right');
    (right || []).forEach(function (x) { add(r, x); });
    return add(c, g, r);
  }
  function cardCol(title) {
    var c = el('div', 'card col');
    if (title) add(c, el('div', null, title));
    for (var i = 1; i < arguments.length; i++) add(c, arguments[i]);
    return c;
  }
  function message(id) { var m = el('div', 'msg'); m.id = 'msg-' + id; var v = msg[id]; if (v) { m.textContent = v.text; m.classList.add(v.err ? 'err' : 'ok'); } return m; }
  function setMsg(id, text, err) { msg[id] = text ? { text: text, err: !!err } : null; var m = document.getElementById('msg-' + id); if (m) { m.textContent = text || ''; m.className = 'msg' + (text ? (err ? ' err' : ' ok') : ''); } }
  function errText(e) { var c = e && e.code; return (c && c in ERR) ? ERR[c] : '發生錯誤（' + (c || '未知') + '）'; }
  function usd(n) { return 'US$' + (n || 0).toFixed(2); }
  function mb(bytes) { return bytes >= 1e9 ? (bytes / 1e9).toFixed(1) + ' GB' : Math.round(bytes / 1e6) + ' MB'; }

  // ---------- data ----------
  function loadAll() {
    return Promise.all([invoke('get_settings'), invoke('key_status'), invoke('budget_status'), invoke('list_models'), invoke('hotkey_status')])
      .then(function (r) { S = r[0]; key = r[1]; budget = r[2]; models = r[3]; hk = r[4]; render(); });
  }

  function input() {
    return {
      mode: S.mode, translate_to: S.translate_to, glossary: S.glossary.slice(),
      hotkey_polish: S.hotkey_polish, hotkey_translate: S.hotkey_translate,
      stt_model: S.stt_model, prices: S.prices, budget: S.budget
    };
  }

  function save(where) {
    return invoke('save_settings', { input: input() }).then(function (status) {
      hk = status;
      setMsg(where, '已儲存');
      return invoke('get_settings').then(function (s) { S = s; return invoke('budget_status'); }).then(function (b) { budget = b; render(); });
    }, function (e) {
      setMsg(where, errText(e), true);
      return invoke('get_settings').then(function (s) { S = s; render(); setMsg(where, errText(e), true); });
    });
  }

  // ---------- banner ----------
  function banner() {
    var b = document.getElementById('banner'), lines = [];
    if (hk && !(hk.polish_registered && hk.translate_registered)) lines.push('快捷鍵註冊失敗：可能已被其他程式佔用。請到「一般」改用其他組合。');
    if (budget && budget.ledger_unreadable) lines.push('預算紀錄無法讀取，已暫停潤稿（只輸出原文）。');
    else if (budget && budget.hard_limited) lines.push('已達本月預算硬上限，已暫停潤稿（只輸出原文）。');
    else if (budget && budget.soft_limited) lines.push('已超過本月預算軟上限。');
    b.hidden = lines.length === 0;
    b.textContent = lines.join('\n');
  }

  // ---------- panes ----------
  function seg(items, current, onpick, disabled) {
    var s = el('div', 'seg');
    items.forEach(function (m) {
      var b = el('button', m[0] === current ? 'on' : '', m[1]);
      b.type = 'button';
      b.disabled = !!(disabled && disabled(m[0]));
      b.onclick = function () { onpick(m[0]); };
      add(s, b);
    });
    return s;
  }

  function hotkeyInput(field) {
    var i = el('input', 'text hotkey');
    i.readOnly = true;
    i.value = S[field].replace(/\+/g, ' + ');
    i.title = '點一下後按下新的組合（Esc 取消）';
    i.onfocus = function () { i.classList.add('recording'); i.value = '請按下組合…'; };
    i.onblur = function () { i.classList.remove('recording'); i.value = S[field].replace(/\+/g, ' + '); };
    i.onkeydown = function (e) {
      e.preventDefault();
      if (e.key === 'Escape') { i.blur(); return; }
      if (/^(Shift|Control|Alt|Meta)(Left|Right)$/.test(e.code)) return;
      var k = e.code.replace(/^Key/, '').replace(/^Digit/, '');
      var combo = (e.ctrlKey ? 'Ctrl+' : '') + (e.altKey ? 'Alt+' : '') + (e.shiftKey ? 'Shift+' : '') + k;
      S[field] = combo;
      i.blur();
      save('hotkey');
    };
    return i;
  }

  function general(p) {
    var hasApi = key.has_key;
    add(p, card('預設模式', MODE_DESC[S.mode] + (hasApi ? '' : '・未設定金鑰時只能用原文'), [
      seg(MODES, S.mode, function (m) { S.mode = m; save('general'); }, function (m) { return !hasApi && m !== 'raw'; })
    ]));
    var sel = el('select');
    LANGS.forEach(function (l) { var o = el('option', null, l[1]); o.value = l[0]; o.selected = S.translate_to === l[0]; add(sel, o); });
    sel.onchange = function () { S.translate_to = sel.value; save('general'); };
    add(p, card('翻譯目標語言', '「潤飾並翻譯」快捷鍵的輸出語言', [sel]));
    add(p, message('general'));
    var reg = hk ? (hk.polish_registered && hk.translate_registered ? '已註冊' : '註冊失敗，請改用其他組合') : '';
    var pol = el('div', 'row'), tr = el('div', 'row');
    add(pol, el('div', 'lbl', '潤飾'), hotkeyInput('hotkey_polish'));
    add(tr, el('div', 'lbl', '潤飾並翻譯'), hotkeyInput('hotkey_translate'));
    var reset = btn('還原預設', null, function () { S.hotkey_polish = 'Alt+Shift+Z'; S.hotkey_translate = 'Alt+Shift+X'; save('hotkey'); });
    add(p, cardCol('快捷鍵', el('div', 'sub', '按一下開始錄音，再按一下停止並處理；錄音中按 Esc 取消。' + (reg ? '狀態：' + reg : '')), pol, tr, reset, message('hotkey')));
  }

  function keyPane(p) {
    var right;
    if (key.has_key) {
      var del = btn('刪除', 'danger', function () {
        if (!del.dataset.armed) { del.dataset.armed = '1'; del.textContent = '確定刪除？'; return; }
        invoke('delete_key').then(function (k) { key = k; setMsg('key', '已刪除'); return refreshSettings(); }, function (e) { setMsg('key', errText(e), true); });
      });
      right = [el('span', 'fp', key.fingerprint || '…'), btn('更換…', null, enterKey), del];
    } else {
      right = [btn('輸入金鑰…', 'primary', enterKey)];
    }
    add(p, card('Claude API 金鑰', key.has_key ? '已存於 Windows 認證管理員・完整金鑰不會再顯示' : '未設定・只能使用原文模式', right));
    add(p, message('key'));
    var guide = el('details', 'guide');
    add(guide, el('summary', null, '安全建議'));
    var ol = el('ol');
    GUIDE.forEach(function (g) { var li = el('li'); add(li, el('b', null, g[0]), document.createTextNode(g[1])); add(ol, li); });
    add(guide, ol);
    add(p, cardCol('', el('div', 'sub', '按「' + (key.has_key ? '更換…' : '輸入金鑰…') + '」會跳出 Windows 原生對話框。金鑰只經過原生程式碼，不會進入這個設定頁（WebView）。儲存前會先向 api.anthropic.com 驗證。'), guide));
  }

  function enterKey() {
    setMsg('key', '請在對話框輸入金鑰…');
    invoke('enter_key').then(function (k) {
      key = k; setMsg('key', '已驗證並儲存');
      return refreshSettings();
    }, function (e) { setMsg('key', e && e.code === 'cancelled' ? '' : errText(e), true); });
  }

  function refreshSettings() {
    return Promise.all([invoke('get_settings'), invoke('budget_status')]).then(function (r) { S = r[0]; budget = r[1]; render(); });
  }

  function glossary(p) {
    var list = el('div', 'list');
    if (!S.glossary.length) add(list, el('div', 'empty', '（沒有詞彙）'));
    S.glossary.forEach(function (t, i) {
      var r = el('div', 'r' + (gSel === i ? ' sel' : ''));
      add(r, el('span', 'grow', t));
      r.onclick = function () { gSel = i; render(); };
      add(list, r);
    });
    var tools = el('div', 'tools'), inp = el('input');
    inp.placeholder = '輸入詞彙後按 ＋ 或 Enter';
    inp.maxLength = 64;
    var addBtn = el('button', null, '＋'), delBtn = el('button', null, '−');
    function doAdd() { var v = inp.value.trim(); if (!v) return; S.glossary.push(v); gSel = -1; save('glossary').then(function () { var n = document.querySelector('.tools input'); if (n) n.focus(); }); }
    addBtn.onclick = doAdd;
    inp.onkeydown = function (e) { if (e.key === 'Enter') doAdd(); };
    delBtn.onclick = function () { if (gSel >= 0) { S.glossary.splice(gSel, 1); gSel = -1; save('glossary'); } };
    add(tools, addBtn, delBtn, inp);
    add(p, cardCol('詞彙表', el('div', 'sub', '人名、產品名、專有名詞。轉寫與潤稿時會照這裡的寫法。只存在這台電腦。'), list, tools, message('glossary')));
  }

  function num(value, onchange) {
    var i = el('input', 'num');
    i.inputMode = 'decimal';
    i.value = value == null ? '' : String(value);
    i.onchange = function () { var t = i.value.trim(); onchange(t === '' ? null : Number(t)); };
    return i;
  }

  function budgetPane(p) {
    var b = budget, c = el('div', 'card col');
    var used = el('div', null, usd(b.estimated_usd));
    if (b.hard_limit_usd != null) add(used, el('span', 'sub', ' ／ ' + b.hard_limit_usd.toFixed(2)));
    var bar = el('div', 'ubar'), fill = el('i');
    var pct = b.hard_limit_usd ? Math.min(100, 100 * b.estimated_usd / b.hard_limit_usd) : 0;
    fill.style.width = pct + '%';
    add(bar, fill);
    var r1 = el('div', 'row'); add(r1, el('div', 'lbl', '本月用量'), add(el('div'), used, bar, el('div', 'sub', '每月 1 日歸零・只在本機計算・' + b.input_tokens + ' 輸入 / ' + b.output_tokens + ' 輸出 token')));
    var r2 = el('div', 'row'); add(r2, el('div', 'lbl', '軟上限'), add(el('div'), document.createTextNode('US$ '), num(S.budget.soft_usd, function (v) { S.budget.soft_usd = v; save('budget'); }), el('span', 'sub', '　到了提醒')));
    var r3 = el('div', 'row'); add(r3, el('div', 'lbl', '硬上限'), add(el('div'), document.createTextNode('US$ '), num(S.budget.hard_usd, function (v) { S.budget.hard_usd = v; save('budget'); }), el('span', 'sub', '　到了停用潤稿，只出原文')));
    var r4 = el('div', 'row');
    add(r4, el('div', 'lbl', '價格表'), add(el('div'),
      add(el('div', 'sub', '每百萬 token'), el('span', 'tag', '請核對官方定價')),
      document.createTextNode('輸入 US$ '), num(S.prices.input_usd_per_mtok, function (v) { S.prices.input_usd_per_mtok = v == null ? 0 : v; save('budget'); }),
      document.createTextNode('　輸出 US$ '), num(S.prices.output_usd_per_mtok, function (v) { S.prices.output_usd_per_mtok = v == null ? 0 : v; save('budget'); })));
    add(c, r1, r2, r3, r4, message('budget'));
    add(p, c);
    add(p, cardCol('', el('div', 'sub', '這只是本機估算，用來擋住程式錯誤造成的失控請求。真正的損失上限請在 Anthropic Console 設定（專用 workspace、不綁付款方式、spend limit）。')));
  }

  function modelPane(p) {
    var list = el('div', 'list models');
    models.forEach(function (m) {
      var r = el('div', 'r'), radio = el('span', 'radio' + (m.selected ? ' on' : ''));
      var g = el('div', 'grow', m.name);
      add(g, el('div', 'sub', mb(m.size_bytes) + (m.verified ? '・已驗證' : m.present ? '・檔案存在，尚未驗證' : '')));
      add(r, radio, g);
      var active = dl && dl.id === m.id && (dl.phase === 'downloading' || dl.phase === 'verifying');
      if (active) {
        add(r, el('span', 'sub', dl.phase === 'verifying' ? 'SHA-256 驗證中…' : '下載中 ' + Math.floor(100 * dl.received / Math.max(1, dl.total)) + '%'));
      } else if (m.verified) {
        add(r, el('span', 'ok-text', '✓ 已驗證'));
      } else if (m.present) {
        add(r, btn('驗證', null, function () {
          setMsg('model', 'SHA-256 驗證中…');
          invoke('verify_model', { id: m.id }).then(function () { setMsg('model', '驗證通過'); return reloadModels(); }, function (e) { setMsg('model', errText(e), true); reloadModels(); });
        }));
      } else {
        add(r, btn('下載 ' + mb(m.size_bytes), null, function () { download(m.id); }));
      }
      r.onclick = function (e) {
        if (e.target.closest('button')) return;
        if (!m.verified) { setMsg('model', '請先下載或驗證這個模型', true); return; }
        S.stt_model = m.id; save('model').then(reloadModels);
      };
      add(list, r);
    });
    add(p, cardCol('語音模型', list, message('model'),
      el('div', 'sub', '只在你按下時下載，完成後比對寫死在程式裡的 SHA-256，不符就刪除。也可以手動把檔案放到 %LOCALAPPDATA%\\keepsay\\models\\<id>.bin 再按「驗證」。')));
  }

  function reloadModels() { return invoke('list_models').then(function (m) { models = m; render(); }); }

  function download(id) {
    setMsg('model', '');
    invoke('download_model', { id: id }).then(function () { setMsg('model', '下載完成，SHA-256 相符'); }, function (e) { setMsg('model', errText(e), true); })
      .then(function () { stopPoll(); return invoke('model_progress'); }).then(function (d) { dl = d; return reloadModels(); });
    startPoll();
  }
  function startPoll() {
    stopPoll();
    dlTimer = setInterval(function () { invoke('model_progress').then(function (d) { dl = d; if (pane === 'model') render(); }); }, 300);
  }
  function stopPoll() { if (dlTimer) clearInterval(dlTimer); dlTimer = null; }

  function privacy(p) {
    add(p, card('麥克風', '設定 → 隱私權與安全性 → 麥克風（需允許桌面應用程式存取）', [btn('開啟系統設定…', null, function () { invoke('open_mic_settings'); })]));
    var pc = el('div', 'privacy');
    add(pc, el('div', 'p1', '無遙測・不保存歷史・音訊不落地'),
      el('div', 'p2', '唯一的連線：api.anthropic.com（潤稿與驗證金鑰時），以及你手動按下的模型下載。沒有帳號、同步或自動更新。'));
    add(p, cardCol('', pc));
  }

  // ---------- render ----------
  function render() {
    if (!S) return;
    var nav = document.getElementById('nav');
    nav.textContent = '';
    PANES.forEach(function (x) {
      var b = el('button', pane === x[0] ? 'on' : '', x[1]);
      b.type = 'button';
      b.onclick = function () { pane = x[0]; gSel = -1; render(); };
      add(nav, b);
    });
    var p = document.getElementById('pane');
    p.textContent = '';
    add(p, el('h3', null, PANES.filter(function (x) { return x[0] === pane; })[0][1]));
    ({ general: general, key: keyPane, glossary: glossary, budget: budgetPane, model: modelPane, privacy: privacy })[pane](p);
    banner();
  }

  loadAll().catch(function (e) {
    var p = document.getElementById('pane');
    p.textContent = '';
    add(p, el('div', 'banner', '無法載入設定：' + errText(e)));
  });
  // Budget and hotkey state can change while the page is open.
  setInterval(function () {
    if (!S || document.hidden) return;
    Promise.all([invoke('budget_status'), invoke('hotkey_status'), invoke('key_status')]).then(function (r) {
      budget = r[0]; hk = r[1]; key = r[2]; banner();
    });
  }, 5000);
})();
