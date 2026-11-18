import { apiPost } from '../core/api-client.js';
import { el, clearChildren, setText } from '../core/dom-safe.js';
import { showToast } from '../core/toast.js';

const state = {
  genre: 'product_prd_trd',
  selectedDocIds: [],
  selectedExperimentIds: [],
  selectedItems: [],
  compiledPreview: null,
};

function parseKnowhowMarkers() {
  const input = document.getElementById('knowhow-markers-input');
  if (!input) return [];
  return input.value
    .split(/[,，\n]+/)
    .map((s) => s.trim())
    .filter(Boolean);
}

function updateSelectedPills() {
  const summaryEl = document.getElementById('vault-selection-summary');
  const pillsEl = document.getElementById('vault-selected-pills');
  setText(
    summaryEl,
    `已选 ${state.selectedDocIds.length} 篇文档 · ${state.selectedExperimentIds.length} 项实验`,
  );
  if (!pillsEl) return;
  clearChildren(pillsEl);
  for (const item of state.selectedItems) {
    pillsEl.appendChild(
      el('span', {
        className: 'selected-pill',
        text: `${item.isExperiment ? 'Exp' : 'Doc'}: ${item.title || item.id}`,
      }),
    );
  }
}

function renderPriorityBanner(priorityCheck) {
  const banner = document.getElementById('priority-guard-banner');
  const titleEl = document.getElementById('priority-title');
  const descEl = document.getElementById('priority-desc');
  const ackWrap = document.getElementById('priority-ack-wrap');
  const ackCheckbox = document.getElementById('priority-ack-checkbox');

  if (!banner || !priorityCheck) return;
  const lamp = ['red', 'yellow', 'green'].includes(priorityCheck.lamp)
    ? priorityCheck.lamp
    : 'green';

  banner.classList.remove('priority-banner--green', 'priority-banner--yellow', 'priority-banner--red');
  banner.classList.add(`priority-banner--${lamp}`);

  const labelMap = {
    red: '公开时序冲突守卫：高危红灯阻断',
    yellow: '公开时序冲突守卫：内部保密黄灯提醒',
    green: '公开时序冲突守卫：绿灯合规通过',
  };
  setText(titleEl, labelMap[lamp] || labelMap.green);
  setText(descEl, priorityCheck.advisory_zh || priorityCheck.advisory_en || '');

  if (ackWrap) {
    if (priorityCheck.has_conflict) {
      ackWrap.classList.remove('hidden');
    } else {
      ackWrap.classList.add('hidden');
      if (ackCheckbox) ackCheckbox.checked = false;
    }
  }
}

async function runPriorityCheck() {
  const notesEl = document.getElementById('custom-notes-input');
  try {
    const res = await apiPost('/api/deliverable-forge/priority-check', {
      related_doc_ids: state.selectedDocIds,
      target_genre: state.genre,
      knowhow_markers: parseKnowhowMarkers(),
      draft_text: notesEl ? notesEl.value : '',
    });
    if (res && res.priority_check) {
      renderPriorityBanner(res.priority_check);
    }
  } catch (err) {
    showToast(`公开时序检查失败: ${err.message}`, 'error');
  }
}

function renderGateReport(gateReport) {
  const badge = document.getElementById('gate-badge');
  const overall = document.getElementById('gate-overall-score');
  const scores = document.getElementById('gate-scores');
  const warnBox = document.getElementById('gate-warnings-box');

  if (!gateReport) return;
  if (badge) {
    badge.classList.remove('gate-badge--pass', 'gate-badge--warn', 'gate-badge--block');
    if (gateReport.decision === 'pass_submittable') {
      badge.classList.add('gate-badge--pass');
      setText(badge, '门禁放行 Pass');
    } else if (gateReport.decision === 'hard_blocked') {
      badge.classList.add('gate-badge--block');
      setText(badge, '硬阻断 Blocked');
    } else {
      badge.classList.add('gate-badge--warn');
      setText(badge, '待补证降级');
    }
  }

  setText(overall, `五维门禁总分：${Number(gateReport.overall_score || 0).toFixed(1)}`);

  if (scores) {
    clearChildren(scores);
    const dims = [
      ['锚点', gateReport.anchor_accuracy_score],
      ['合规', gateReport.scope_compliance_score],
      ['逻辑', gateReport.logic_completeness_score],
      ['实施例', gateReport.embodiment_consistency_score],
      ['时序保密', gateReport.priority_privacy_score],
    ];
    for (const [name, val] of dims) {
      scores.appendChild(el('span', { text: `${name}: ${Number(val || 0).toFixed(0)}` }));
    }
  }

  if (warnBox) {
    const messages = [
      ...(Array.isArray(gateReport.blocking_reasons) ? gateReport.blocking_reasons : []),
      ...(Array.isArray(gateReport.downgrade_warnings) ? gateReport.downgrade_warnings : []),
    ];
    clearChildren(warnBox);
    if (messages.length > 0) {
      warnBox.classList.remove('hidden');
      warnBox.appendChild(el('strong', { text: '门禁提示与文献过滤记录：' }));
      for (const msg of messages) {
        warnBox.appendChild(el('div', { text: `• ${msg}` }));
      }
    } else {
      warnBox.classList.add('hidden');
    }
  }
}

function renderPreviewSections(preview) {
  const container = document.getElementById('preview-sections');
  if (!container) return;
  clearChildren(container);

  const sections = Array.isArray(preview.sections) ? preview.sections : [];
  if (sections.length === 0) {
    container.appendChild(
      el('div', { className: 'preview-section' }, [
        el('h3', { className: 'preview-section-title', text: '无可用章节' }),
      ]),
    );
    return;
  }

  for (const sec of sections) {
    const chipsRow = el('div', { className: 'preview-chips' });
    const citations = Array.isArray(sec.citations) ? sec.citations : [];
    for (const cit of citations) {
      const chip = document.createElement('if-citation-chip');
      chip.setAttribute('doc-id', cit.doc_id || '');
      chip.setAttribute('anchor-tag', cit.anchor_tag || '');
      chip.setAttribute('verdict', cit.verdict || 'unverified');
      chip.setAttribute('summary', cit.reason || cit.matched_snippet || '');
      chipsRow.appendChild(chip);
    }

    container.appendChild(
      el('article', { className: 'preview-section' }, [
        el('div', { className: 'preview-section-head' }, [
          el('h3', { className: 'preview-section-title', text: sec.heading || sec.key }),
        ]),
        el('div', { className: 'preview-section-body', text: sec.body || '' }),
        citations.length > 0 ? chipsRow : null,
      ]),
    );
  }
}

async function handleCompilePreview() {
  const titleInput = document.getElementById('forge-title-input');
  const notesInput = document.getElementById('custom-notes-input');
  const useAiBox = document.getElementById('use-ai-checkbox');
  const compileBtn = document.getElementById('btn-compile-preview');

  const title = titleInput ? titleInput.value.trim() : '';
  if (!title) {
    showToast('请先填写成果标题', 'warn');
    if (titleInput) titleInput.focus();
    return;
  }

  if (compileBtn) compileBtn.disabled = true;
  try {
    const res = await apiPost('/api/deliverable-forge/compile', {
      title,
      genre: state.genre,
      related_doc_ids: state.selectedDocIds,
      reference_doc_ids: state.selectedDocIds,
      experiment_ids: state.selectedExperimentIds,
      knowhow_markers: parseKnowhowMarkers(),
      custom_notes: notesInput ? notesInput.value : '',
      use_ai: Boolean(useAiBox && useAiBox.checked),
    });

    if (res && res.preview) {
      state.compiledPreview = res.preview;
      renderPriorityBanner(res.preview.priority_check);
      renderGateReport(res.preview.gate_report);
      renderPreviewSections(res.preview);
      showToast('分节可核验预览编译完成', 'success');
    }
  } catch (err) {
    showToast(`编译预览失败: ${err.message}`, 'error');
  } finally {
    if (compileBtn) compileBtn.disabled = false;
  }
}

function parseFilenameFromDisposition(disposition, fallbackName) {
  if (!disposition) return fallbackName;
  const match = disposition.match(/filename="([^"]+)"/i);
  return match && match[1] ? match[1] : fallbackName;
}

async function handleExport(format, bundleZip) {
  const titleInput = document.getElementById('forge-title-input');
  const ackBox = document.getElementById('priority-ack-checkbox');
  const title = titleInput ? titleInput.value.trim() : '';

  if (!title) {
    showToast('请先填写成果标题', 'warn');
    return;
  }
  if (!state.compiledPreview || !Array.isArray(state.compiledPreview.sections) || state.compiledPreview.sections.length === 0) {
    showToast('请先点击「编译分节可核验预览」生成草稿后再导出', 'warn');
    return;
  }

  const sections = state.compiledPreview.sections.map((s) => [s.heading || s.key, s.body || '']);
  try {
    const resp = await fetch('/api/deliverable-forge/export', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      credentials: 'same-origin',
      body: JSON.stringify({
        title,
        genre: state.genre,
        format,
        sections,
        related_doc_ids: state.selectedDocIds,
        reference_doc_ids: state.selectedDocIds,
        knowhow_markers: parseKnowhowMarkers(),
        acknowledge_priority_warning: Boolean(ackBox && ackBox.checked),
        bundle_zip: Boolean(bundleZip),
      }),
    });

    if (!resp.ok) {
      const errPayload = await resp.json().catch(() => null);
      const msg = (errPayload && errPayload.message) || `导出失败 (HTTP ${resp.status})`;
      showToast(msg, 'error', 5000);
      return;
    }

    const blob = await resp.blob();
    const ext = bundleZip ? 'zip' : format;
    const filename = parseFilenameFromDisposition(
      resp.headers.get('content-disposition'),
      `${title}.${ext}`,
    );
    const url = URL.createObjectURL(blob);
    const anchor = el('a', { href: url, download: filename });
    document.body.appendChild(anchor);
    anchor.click();
    document.body.removeChild(anchor);
    URL.revokeObjectURL(url);
    showToast(`已导出 ${filename}`, 'success');
  } catch (err) {
    showToast(`导出请求异常: ${err.message}`, 'error');
  }
}

function bindEvents() {
  const genreContainer = document.getElementById('genre-selector');
  if (genreContainer) {
    genreContainer.addEventListener('click', (ev) => {
      const btn = ev.target.closest('[data-genre]');
      if (!btn) return;
      state.genre = btn.getAttribute('data-genre') || 'product_prd_trd';
      for (const item of genreContainer.querySelectorAll('[data-genre]')) {
        item.classList.toggle('is-active', item === btn);
      }
      runPriorityCheck();
    });
  }

  const btnPriority = document.getElementById('btn-priority-check');
  if (btnPriority) {
    btnPriority.addEventListener('click', () => {
      runPriorityCheck();
    });
  }

  const btnOpenPicker = document.getElementById('btn-open-vault-picker');
  const picker = document.getElementById('forge-vault-picker');
  if (btnOpenPicker && picker) {
    btnOpenPicker.addEventListener('click', async () => {
      picker.show();
      if (typeof picker.loadFromApi === 'function') {
        try {
          await picker.loadFromApi('/api/vault/docs');
        } catch (err) {
          showToast(`加载知识金库列表失败: ${err.message}`, 'warn');
        }
      }
    });

    picker.addEventListener('if-vault-select', (ev) => {
      const detail = ev.detail || {};
      state.selectedDocIds = Array.isArray(detail.docIds) ? detail.docIds : [];
      state.selectedExperimentIds = Array.isArray(detail.experimentIds) ? detail.experimentIds : [];
      state.selectedItems = Array.isArray(detail.items) ? detail.items : [];
      updateSelectedPills();
      runPriorityCheck();
    });
  }

  const btnCompile = document.getElementById('btn-compile-preview');
  if (btnCompile) {
    btnCompile.addEventListener('click', handleCompilePreview);
  }

  for (const exportBtn of document.querySelectorAll('[data-export-format]')) {
    exportBtn.addEventListener('click', () => {
      const fmt = exportBtn.getAttribute('data-export-format') || 'md';
      const zipFlag = exportBtn.getAttribute('data-bundle-zip') === 'true';
      handleExport(fmt, zipFlag);
    });
  }

  document.addEventListener('if-open-source', (ev) => {
    const detail = ev.detail || {};
    const box = document.getElementById('citation-detail-box');
    if (!box) return;
    clearChildren(box);
    box.classList.remove('hidden');
    box.appendChild(
      el('strong', { text: `引证核验详情 · ${detail.anchorTag || ''}` }),
    );
    box.appendChild(
      el('div', {
        text: `文档ID: ${detail.docId || '无'} | 核验状态: ${detail.verdict || 'unverified'} | 说明: ${detail.summary || '暂无'}`,
      }),
    );
  });
}

document.addEventListener('DOMContentLoaded', () => {
  updateSelectedPills();
  bindEvents();
  runPriorityCheck();
});
