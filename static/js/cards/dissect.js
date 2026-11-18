import { apiPost } from '../core/api-client.js';
import { el, clearChildren, setText, tr } from '../core/dom-safe.js';
import { showToast } from '../core/toast.js';
import '../widgets/if-citation-chip.js';
import '../widgets/if-multimodal-box.js';
import '../widgets/if-vault-picker.js';

const state = {
  doc: null,
  chunks: [],
  figureNumerals: [],
  sections: [],
  activeSubType: '',
  verifiedCitations: [],
  featureAtoms: [],
  selectedNumeral: '',
  selectedAnchor: '',
  selectedExcerpt: '',
  qaHistory: [],
};

function readFileAsBase64(file) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = typeof reader.result === 'string' ? reader.result : '';
      const idx = result.indexOf('base64,');
      resolve(idx >= 0 ? result.slice(idx + 7) : result);
    };
    reader.onerror = () => reject(new Error('Failed to read file'));
    reader.readAsDataURL(file);
  });
}

function readFileAsDataUrl(file) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(typeof reader.result === 'string' ? reader.result : '');
    reader.onerror = () => reject(new Error('Failed to read image'));
    reader.readAsDataURL(file);
  });
}

function isPrivacyOffline() {
  const cb = document.getElementById('dissect-privacy-lock');
  return Boolean(cb && cb.checked);
}

function renderDocHeader() {
  const badge = document.getElementById('dissect-doc-badge');
  if (!badge) return;
  if (!state.doc) {
    setText(badge, tr('dissect.noDoc', null, '尚未加载文献'));
    return;
  }
  const title = state.doc.title_zh || state.doc.title_en || state.doc.doc_id;
  setText(badge, `${state.doc.doc_kind} · ${title} (${state.chunks.length} 切片)`);
}

function renderNumerals() {
  const container = document.getElementById('dissect-numerals-container');
  if (!container) return;
  clearChildren(container);

  if (!state.figureNumerals.length) {
    container.appendChild(el('span', {
      className: 'muted',
      text: tr('dissect.noNumerals', null, '暂无识别到的附图部件标号'),
    }));
    return;
  }

  for (const num of state.figureNumerals) {
    const isActive = state.selectedNumeral === num.numeral_code;
    const pill = el('button', {
      type: 'button',
      className: isActive ? 'dissect-numeral-pill is-active' : 'dissect-numeral-pill',
      text: `${num.numeral_code} · ${num.part_name} (${num.figure_label})`,
      onClick: () => {
        state.selectedNumeral = isActive ? '' : num.numeral_code;
        state.selectedAnchor = num.anchor_tag || '';
        renderNumerals();
        renderChunks();
        updateSelectionHint();
      },
    });
    container.appendChild(pill);
  }
}

function updateSelectionHint() {
  const hint = document.getElementById('dissect-selection-hint');
  if (!hint) return;
  const parts = [];
  if (state.selectedAnchor) parts.push(`锚点: ${state.selectedAnchor}`);
  if (state.selectedNumeral) parts.push(`标号: ${state.selectedNumeral}`);
  if (state.selectedExcerpt) parts.push(`摘录: ${state.selectedExcerpt.slice(0, 48)}...`);
  if (!parts.length) {
    setText(hint, '提示：在左栏点击任意切片的「框选追问」或点击附图部件标号，即可锁定局部上下文；下方支持粘贴截图追问。');
  } else {
    setText(hint, `已锁定局部上下文 — ${parts.join(' | ')}`);
  }
}

function renderChunks(highlightTag = '') {
  const container = document.getElementById('dissect-chunks-container');
  if (!container) return;
  clearChildren(container);

  if (!state.chunks.length) {
    container.appendChild(el('div', {
      className: 'muted',
      text: tr('dissect.emptyChunks', null, '请先在顶部上传文献或从金库点选打开。'),
    }));
    return;
  }

  let scrollTarget = null;
  for (const ch of state.chunks) {
    const matchesNumeral = !state.selectedNumeral
      || ch.raw_content.includes(state.selectedNumeral);
    if (!matchesNumeral) continue;

    const isHighlighted = Boolean(highlightTag) && ch.section_tag === highlightTag;
    const card = el('article', {
      className: isHighlighted ? 'dissect-chunk-card is-highlighted' : 'dissect-chunk-card',
      dataset: { anchor: ch.section_tag },
    }, [
      el('div', { className: 'dissect-chunk-head' }, [
        el('strong', { text: `[${ch.section_tag}] (${ch.section_kind})` }),
        el('button', {
          type: 'button',
          className: 'btn-secondary',
          style: 'padding:2px 8px;font-size:11px;',
          text: '框选追问',
          onClick: () => {
            state.selectedAnchor = ch.section_tag;
            state.selectedExcerpt = ch.raw_content.slice(0, 300);
            updateSelectionHint();
            switchMainTab('ask');
            showToast(`已锁定切片 ${ch.section_tag} 进入追问`, 'info');
          },
        }),
      ]),
      el('div', { className: 'dissect-chunk-body', text: ch.raw_content }),
    ]);

    if (isHighlighted && !scrollTarget) {
      scrollTarget = card;
    }
    container.appendChild(card);
  }

  if (scrollTarget && typeof scrollTarget.scrollIntoView === 'function') {
    scrollTarget.scrollIntoView({ behavior: 'smooth', block: 'center' });
  }
}

function renderCitationChips(container, citations) {
  if (!container) return;
  clearChildren(container);
  if (!citations || !citations.length) {
    container.appendChild(el('span', {
      className: 'muted',
      style: 'font-size:12px;',
      text: '暂无核验锚点芯片',
    }));
    return;
  }

  for (const cit of citations) {
    const chip = document.createElement('if-citation-chip');
    chip.setAttribute('doc-id', cit.doc_id || '');
    chip.setAttribute('anchor-tag', cit.anchor_tag || cit.raw_mention || '');
    chip.setAttribute('verdict', cit.verdict || 'unverified');
    chip.setAttribute('summary', cit.matched_excerpt || cit.reason || '');
    container.appendChild(chip);
  }
}

function renderReportView() {
  const subtabs = document.getElementById('dissect-subtabs');
  const box = document.getElementById('dissect-report-box');
  const chipBar = document.getElementById('dissect-citations-bar');
  if (!subtabs || !box) return;

  clearChildren(subtabs);
  renderCitationChips(chipBar, state.verifiedCitations);

  if (!state.sections.length) {
    setText(box, '请先拖入文献或从金库点选一篇文献，再点击「开始双模式深析」。');
    return;
  }

  if (!state.activeSubType && state.sections[0]) {
    state.activeSubType = state.sections[0].sub_type;
  }

  for (const sec of state.sections) {
    const isActive = sec.sub_type === state.activeSubType;
    subtabs.appendChild(el('button', {
      type: 'button',
      className: isActive ? 'dissect-tab-btn is-active' : 'dissect-tab-btn',
      text: sec.title_zh || sec.sub_type,
      onClick: () => {
        state.activeSubType = sec.sub_type;
        renderReportView();
      },
    }));
  }

  const current = state.sections.find((s) => s.sub_type === state.activeSubType) || state.sections[0];
  const providerTag = current.provider_id ? `[AI: ${current.provider_id}]` : '[离线确定性结构化提取]';
  setText(box, `${providerTag}\n\n${current.markdown}`);
}

function renderAtoms() {
  const list = document.getElementById('dissect-atoms-list');
  if (!list) return;
  clearChildren(list);

  if (!state.featureAtoms.length) {
    list.appendChild(el('div', {
      className: 'muted',
      text: '尚未沉淀特征原子。运行「开始双模式深析」后将自动抽取技术问题-手段-效果三元组写入本地金库。',
    }));
    return;
  }

  for (const atom of state.featureAtoms) {
    const chip = document.createElement('if-citation-chip');
    chip.setAttribute('doc-id', atom.doc_id || '');
    chip.setAttribute('anchor-tag', atom.anchor_tag || '');
    chip.setAttribute('verdict', 'verified');
    chip.setAttribute('summary', atom.tech_means || '');

    list.appendChild(el('div', { className: 'dissect-atom-card' }, [
      el('div', { style: 'display:flex;justify-content:space-between;align-items:center;margin-bottom:4px;' }, [
        el('strong', { text: atom.atom_id }),
        chip,
      ]),
      el('div', { text: `技术问题：${atom.tech_problem}` }),
      el('div', { text: `技术手段：${atom.tech_means}` }),
      el('div', { className: 'muted', text: `技术效果：${atom.tech_effect}` }),
    ]));
  }
}

function renderQaThread() {
  const thread = document.getElementById('dissect-qa-thread');
  if (!thread) return;
  clearChildren(thread);

  for (const item of state.qaHistory) {
    const chipRow = el('div', { className: 'dissect-chip-bar', style: 'margin-top:6px;' });
    if (item.citations && item.citations.length) {
      renderCitationChips(chipRow, item.citations);
    }
    thread.appendChild(el('div', { className: 'dissect-qa-msg' }, [
      el('strong', { text: item.role === 'user' ? '追问：' : '深析伴读：' }),
      el('div', { text: item.text }),
      chipRow,
    ]));
  }
  thread.scrollTop = thread.scrollHeight;
}

function switchMainTab(tabKey) {
  const panels = {
    report: document.getElementById('dissect-panel-report'),
    atoms: document.getElementById('dissect-panel-atoms'),
    ask: document.getElementById('dissect-panel-ask'),
  };
  for (const [key, node] of Object.entries(panels)) {
    if (node) node.hidden = key !== tabKey;
  }
  const tabBtns = document.querySelectorAll('#dissect-tabs .dissect-tab-btn');
  for (const btn of tabBtns) {
    const isTarget = btn.getAttribute('data-tab') === tabKey;
    btn.classList.toggle('is-active', isTarget);
  }
}

function applyIngestResult(res) {
  state.doc = res.doc || null;
  state.chunks = Array.isArray(res.chunks) ? res.chunks : [];
  state.figureNumerals = Array.isArray(res.figure_numerals) ? res.figure_numerals : [];
  state.featureAtoms = Array.isArray(res.existing_feature_atoms) ? res.existing_feature_atoms : [];
  state.sections = [];
  state.activeSubType = '';
  state.verifiedCitations = [];
  state.selectedNumeral = '';
  state.selectedAnchor = '';
  state.selectedExcerpt = '';
  state.qaHistory = [];

  renderDocHeader();
  renderNumerals();
  renderChunks();
  renderReportView();
  renderAtoms();
  renderQaThread();
  updateSelectionHint();
}

async function handleIngestSubmit(detail) {
  const titleEl = document.getElementById('dissect-title-input');
  const extEl = document.getElementById('dissect-external-id-input');
  const payload = {
    title: titleEl ? titleEl.value.trim() || null : null,
    external_id: extEl ? extEl.value.trim() || null : null,
    fulltext_raw: detail.text || null,
    privacy_offline_only: isPrivacyOffline(),
  };

  const firstFile = (detail.attachments && detail.attachments[0] && detail.attachments[0].file)
    || (detail.images && detail.images[0] && detail.images[0].file)
    || null;

  if (firstFile) {
    payload.filename = firstFile.name || 'upload.bin';
    payload.mime_type = firstFile.type || 'application/octet-stream';
    payload.file_base64 = await readFileAsBase64(firstFile);
  }

  if (!payload.fulltext_raw && !payload.file_base64) {
    showToast('请先输入文献全文或拖入 PDF/DOCX/图像文件', 'warn');
    return;
  }

  try {
    const res = await apiPost('/api/deep-dissect/ingest', payload);
    applyIngestResult(res);
    showToast(`文献入库切分完成（${state.chunks.length} 个切片）`, 'success');
  } catch (err) {
    showToast(err.message || '文献入库失败', 'error');
  }
}

async function handleVaultDocSelect(docId) {
  if (!docId) return;
  try {
    const res = await apiPost('/api/deep-dissect/ingest', {
      doc_id: docId,
      privacy_offline_only: isPrivacyOffline(),
    });
    applyIngestResult(res);
    showToast(`已从金库加载文献：${state.doc ? state.doc.title_zh : docId}`, 'success');
  } catch (err) {
    showToast(err.message || '从金库加载文献失败', 'error');
  }
}

async function handleAnalyze() {
  if (!state.doc || !state.doc.doc_id) {
    showToast('请先入库或从金库点选一篇文献', 'warn');
    return;
  }
  const modeSel = document.getElementById('dissect-mode-select');
  const mode = modeSel ? modeSel.value : 'auto';

  try {
    const res = await apiPost('/api/deep-dissect/analyze', {
      doc_id: state.doc.doc_id,
      mode,
      privacy_offline_only: isPrivacyOffline(),
    });
    state.sections = Array.isArray(res.sections) ? res.sections : [];
    state.activeSubType = state.sections[0] ? state.sections[0].sub_type : '';
    state.verifiedCitations = Array.isArray(res.verified_citations) ? res.verified_citations : [];
    state.featureAtoms = Array.isArray(res.saved_feature_atoms) ? res.saved_feature_atoms : [];
    if (Array.isArray(res.figure_numerals)) {
      state.figureNumerals = res.figure_numerals;
    }

    renderNumerals();
    renderReportView();
    renderAtoms();
    switchMainTab('report');
    showToast(`深析完成：核验 ${state.verifiedCitations.length} 个锚点，沉淀 ${state.featureAtoms.length} 个特征原子`, 'success');
  } catch (err) {
    showToast(err.message || '深析失败', 'error');
  }
}

async function handleAskSubmit(detail) {
  if (!state.doc || !state.doc.doc_id) {
    showToast('请先加载一篇文献后再发起追问', 'warn');
    return;
  }
  const question = (detail.text || '').trim();
  if (!question) {
    showToast('请输入追问问题', 'warn');
    return;
  }

  const imageDataUrls = [];
  if (Array.isArray(detail.images)) {
    for (const img of detail.images) {
      if (img && img.file) {
        const dataUrl = await readFileAsDataUrl(img.file);
        if (dataUrl) imageDataUrls.push(dataUrl);
      }
    }
  }

  state.qaHistory.push({ role: 'user', text: question, citations: [] });
  renderQaThread();

  try {
    const res = await apiPost('/api/deep-dissect/ask', {
      doc_id: state.doc.doc_id,
      question,
      selected_excerpt: state.selectedExcerpt || null,
      selected_anchor: state.selectedAnchor || null,
      figure_numeral: state.selectedNumeral || null,
      image_data_urls: imageDataUrls,
      privacy_offline_only: isPrivacyOffline(),
    });
    state.qaHistory.push({
      role: 'assistant',
      text: res.answer_markdown || '',
      citations: Array.isArray(res.verified_citations) ? res.verified_citations : [],
    });
    renderQaThread();
  } catch (err) {
    showToast(err.message || '追问失败', 'error');
  }
}

function initDissectPage() {
  if (typeof window.renderSidebar === 'function') {
    window.renderSidebar('dissect');
  }

  const ingestBox = document.getElementById('dissect-ingest-box');
  if (ingestBox) {
    ingestBox.addEventListener('if-submit', (ev) => {
      handleIngestSubmit(ev.detail || {});
    });
  }

  const askBox = document.getElementById('dissect-ask-box');
  if (askBox) {
    askBox.addEventListener('if-submit', (ev) => {
      handleAskSubmit(ev.detail || {});
    });
  }

  const vaultPicker = document.getElementById('dissect-vault-picker');
  const openVaultBtn = document.getElementById('btn-open-vault-picker');
  if (openVaultBtn && vaultPicker) {
    openVaultBtn.addEventListener('click', () => {
      vaultPicker.show();
      if (typeof vaultPicker.loadFromApi === 'function') {
        vaultPicker.loadFromApi('/api/vault/docs').catch((err) => {
          showToast(err.message || '加载金库列表失败', 'error');
        });
      }
    });
    vaultPicker.addEventListener('if-select-change', (ev) => {
      const ids = ev.detail && Array.isArray(ev.detail.docIds) ? ev.detail.docIds : [];
      if (ids.length > 0) {
        handleVaultDocSelect(ids[0]);
      }
    });
  }

  const analyzeBtn = document.getElementById('btn-run-analyze');
  if (analyzeBtn) {
    analyzeBtn.addEventListener('click', handleAnalyze);
  }

  const tabBtns = document.querySelectorAll('#dissect-tabs .dissect-tab-btn');
  for (const btn of tabBtns) {
    btn.addEventListener('click', () => {
      const target = btn.getAttribute('data-tab') || 'report';
      switchMainTab(target);
    });
  }

  document.addEventListener('if-open-source', (ev) => {
    const detail = ev.detail || {};
    if (detail.anchorTag) {
      state.selectedNumeral = '';
      renderNumerals();
      renderChunks(detail.anchorTag);
      showToast(`已定位至原文锚点：${detail.anchorTag}`, 'info');
    }
  });

  const params = new URLSearchParams(window.location.search);
  const initialDocId = params.get('doc_id');
  if (initialDocId) {
    handleVaultDocSelect(initialDocId);
  }
}

document.addEventListener('DOMContentLoaded', initDissectPage);
