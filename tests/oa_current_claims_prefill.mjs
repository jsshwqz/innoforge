// tests/oa_current_claims_prefill.mjs
// #159：把「审查所针对的那版权项」接进核验门的产品侧来源。
//
// 做法：从 templates/office_action_response.html 抽出真实的 lookupMyPatent()，
// 用 mock DOM 执行，断言：
//   1. 现权项输入框为空、查到的专利带 claims ⇒ 回填，并派发 input（草稿保存接得住）；
//   2. 输入框已有内容（手填/草稿）⇒ 不覆盖、不派发；
//   3. claims 为空/只有空白 ⇒ 不回填；
//   4. 专利没有 claims 字段 ⇒ 不回填。
//
// 输出一行一个 CASE，Rust 侧（tests/oa_current_claims_prefill.rs）据此判定。

import { readFileSync } from 'node:fs';

const templatePath = 'templates/office_action_response.html';
const template = readFileSync(templatePath, 'utf8');

const startMarker = 'async function lookupMyPatent() {';
const start = template.indexOf(startMarker);
if (start < 0) {
  console.log(`CASE extract FAIL ${templatePath} 里找不到 lookupMyPatent()`);
  process.exit(1);
}

// 从函数体的 { 开始做括号配对（跳过字符串与行注释），取到配对的 }
function matchBrace(text, openIdx) {
  let depth = 0;
  for (let i = openIdx; i < text.length; i++) {
    const c = text[i];
    if (c === '"' || c === "'") {
      let j = i + 1;
      while (j < text.length) {
        if (text[j] === '\\') j += 2;
        else if (text[j] === c) break;
        else j++;
      }
      i = j;
      continue;
    }
    if (c === '/' && text[i + 1] === '/') {
      const nl = text.indexOf('\n', i);
      i = nl === -1 ? text.length : nl;
      continue;
    }
    if (c === '{') depth++;
    else if (c === '}') {
      depth--;
      if (depth === 0) return i;
    }
  }
  return -1;
}

const open = template.indexOf('{', start);
const end = matchBrace(template, open);
if (open < 0 || end < 0) {
  console.log('CASE extract FAIL 无法配对 lookupMyPatent 的函数体');
  process.exit(1);
}
const fnSource = template.slice(start, end + 1);

let failures = 0;
function caseResult(name, ok, detail) {
  if (ok) {
    console.log(`CASE ${name} PASS`);
  } else {
    failures++;
    console.log(`CASE ${name} FAIL ${detail}`);
  }
}

function makeHarness({ claims, textareaValue, hasClaimsField = true }) {
  const events = [];
  const elements = {
    'my-patent': { value: 'CN123456789', disabled: false },
    'my-file-tag': { innerHTML: '' },
    'current-claims': {
      value: textareaValue,
      dispatchEvent(e) {
        events.push(e.type);
        return true;
      },
    },
  };
  const document = { getElementById: (id) => elements[id] || null };
  const uploadedData = {};
  const t = (key) => key;
  const escapeHtml = (s) => String(s);
  class MockEvent {
    constructor(type) {
      this.type = type;
    }
  }
  const patent = { title: 'T', patent_number: 'CN123456789', abstract_text: '', description: '' };
  if (hasClaimsField) patent.claims = claims;
  const doPatentLookup = async (_number, onSuccess) => onSuccess(patent);
  const alert = (msg) => {
    throw new Error('unexpected alert: ' + msg);
  };

  const factory = new Function(
    'document',
    'uploadedData',
    't',
    'escapeHtml',
    'doPatentLookup',
    'alert',
    'Event',
    `${fnSource}\nreturn lookupMyPatent;`
  );
  const lookup = factory(document, uploadedData, t, escapeHtml, doPatentLookup, alert, MockEvent);
  return { lookup, elements, events, uploadedData };
}

async function main() {
  // 1. 空框 + 有 claims ⇒ 回填并触发 input
  {
    const h = makeHarness({ claims: '1.一种装置，包括A和B。', textareaValue: '' });
    await h.lookup();
    caseResult(
      'prefill_empty',
      h.elements['current-claims'].value === '1.一种装置，包括A和B。',
      `value=${JSON.stringify(h.elements['current-claims'].value)}`
    );
    caseResult(
      'prefill_dispatches_input',
      h.events.includes('input'),
      `events=${JSON.stringify(h.events)}`
    );
  }

  // 2. 已手填/含草稿 ⇒ 不覆盖、不派发
  {
    const h = makeHarness({ claims: '1.一种装置，包括A和B。', textareaValue: 'KEEP-ME' });
    await h.lookup();
    caseResult(
      'existing_value_kept',
      h.elements['current-claims'].value === 'KEEP-ME',
      `value=${JSON.stringify(h.elements['current-claims'].value)}`
    );
    caseResult('no_event_when_kept', !h.events.includes('input'), `events=${JSON.stringify(h.events)}`);
  }

  // 3. claims 只有空白 ⇒ 不回填
  {
    const h = makeHarness({ claims: '   \n  ', textareaValue: '' });
    await h.lookup();
    caseResult(
      'blank_claims_ignored',
      h.elements['current-claims'].value === '',
      `value=${JSON.stringify(h.elements['current-claims'].value)}`
    );
    caseResult(
      'no_event_for_blank',
      !h.events.includes('input'),
      `events=${JSON.stringify(h.events)}`
    );
  }

  // 4. 专利没有 claims 字段 ⇒ 不回填
  {
    const h = makeHarness({ claims: '', textareaValue: '', hasClaimsField: false });
    await h.lookup();
    caseResult(
      'missing_claims_ignored',
      h.elements['current-claims'].value === '',
      `value=${JSON.stringify(h.elements['current-claims'].value)}`
    );
  }

  // 附带：回填后 my_patent 仍按原逻辑装配（防止改坏主路径）
  {
    const h = makeHarness({ claims: '1.一种装置。', textareaValue: '' });
    await h.lookup();
    caseResult(
      'uploaded_my_still_set',
      h.uploadedData.my && h.uploadedData.my.content.includes('权利要求书：'),
      `uploaded=${JSON.stringify(h.uploadedData.my && h.uploadedData.my.content)}`
    );
  }

  if (failures > 0) {
    console.log(`SUMMARY failures=${failures}`);
    process.exit(1);
  }
  console.log('SUMMARY failures=0');
}

main().catch((e) => {
  console.log(`CASE runtime FAIL ${e && e.stack ? e.stack.split('\n')[0] : e}`);
  process.exit(1);
});
