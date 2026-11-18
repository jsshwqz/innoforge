import { apiPost } from '../core/api-client.js';
import { el, clearChildren, tr } from '../core/dom-safe.js';
import { showToast } from '../core/toast.js';
import '../widgets/if-matrix-table.js';
import '../widgets/if-vault-picker.js';

const MIN_SLOTS = 2;
const MAX_SLOTS = 6;

const state = {
  slots: [
    { label: '方案 1（基准）', docId: '', runId: '', role: 'own_asset', inlineText: '', metricsText: '' },
    { label: '方案 2（对比）', docId: '', runId: '', role: 'competitor_wall', inlineText: '', metricsText: '' },
  ],
  activeMode: 'engineering_spec',
  currentMatrix: null,
  busy: false,
};

function getEl(id) {
  return document.getElementById(id);
}

function renderSlots() {
  const container = getEl('scheme-slots');
  if (!container) return;
  clearChildren(container);

  state.slots.forEach((slot, idx) => {
    const removeBtn = state.slots.length > MIN_SLOTS
      ? el('button', {
          type: 'button',
          className: 'if-mm-btn if-mm-btn--ghost',
          text: '×',
          'aria-label': tr('matrix.removeSlot', null, 'Remove slot'),
          onClick: () => {
            state.slots.splice(idx, 1);
            renderSlots();
          },
        })
      : null;

    const labelInput = el('input', {
      type: 'text',
      className: 'matrix-slot-input',
      placeholder: `方案 ${idx + 1} 名称`,
      value: slot.label,
      onInput: (ev) => {
        slot.label = ev.target ? ev.target.value : '';
      },
    });
    labelInput.value = slot.label;

    const roleSelect = el('select', {
      className: 'matrix-slot-input',
      onChange: (ev) => {
        slot.role = ev.target ? ev.target.value : 'candidate';
      },
    }, [
      el('option', { value: 'own_asset', text: '我方前代/自有资产' }),
      el('option', { value: 'competitor_wall', text: '竞品专利/墙' }),
      el('option', { value: 'general_ref', text: '学术 SOTA Baseline' }),
      el('option', { value: 'candidate', text: '新候选方案' }),
    ]);
    roleSelect.value = slot.role || 'candidate';

    const docInput = el('input', {
      type: 'text',
      className: 'matrix-slot-input',
      placeholder: 'doc_id（可选）',
      value: slot.docId,
      onInput: (ev) => {
        slot.docId = ev.target ? ev.target.value : '';
      },
    });
    docInput.value = slot.docId;

    const runInput = el('input', {
      type: 'text',
      className: 'matrix-slot-input',
      placeholder: 'run_id（可选）',
      value: slot.runId,
      onInput: (ev) => {
        slot.runId = ev.target ? ev.target.value : '';
      },
    });
    runInput.value = slot.runId;

    const textArea = el('textarea', {
      className: 'matrix-slot-input matrix-slot-textarea',
      placeholder: '粘贴方案技术描述、权利要求或架构要点（若已填 doc_id/run_id 可作补充）',
      onInput: (ev) => {
        slot.inlineText = ev.target ? ev.target.value : '';
      },
    });
    textArea.value = slot.inlineText;

    const metricsInput = el('input', {
      type: 'text',
      className: 'matrix-slot-input',
      placeholder: '指标 JSON（可选，如 {"latency":"12ms","cost":"$5"}）',
      value: slot.metricsText,
      onInput: (ev) => {
        slot.metricsText = ev.target ? ev.target.value : '';
      },
    });
    metricsInput.value = slot.metricsText;

    const card = el('div', { className: 'matrix-slot-card' }, [
      el('div', { className: 'matrix-slot-head' }, [
        el('span', { className: 'matrix-slot-badge', text: `槽位 #${idx + 1}` }),
        removeBtn,
      ]),
      el('div', { className: 'matrix-slot-row' }, [labelInput, roleSelect]),
      el('div', { className: 'matrix-slot-row' }, [docInput, runInput]),
      textArea,
      metricsInput,
    ]);

    container.appendChild(card);
  });
}

function buildSourcesPayload() {
  return state.slots.map((slot) => {
    let parsedMetrics = null;
    const rawMetrics = (slot.metricsText || '').trim();
    if (rawMetrics) {
      try {
        parsedMetrics = JSON.parse(rawMetrics);
      } catch {
        throw new Error(`槽位「${slot.label || '未命名'}」的指标 JSON 格式无效`);
      }
    }
    return {
      label: (slot.label || '').trim(),
      doc_id: (slot.docId || '').trim() || null,
      run_id: (slot.runId || '').trim() || null,
      inline_text: (slot.inlineText || '').trim() || null,
      metrics: parsedMetrics,
      role: slot.role || null,
    };
  });
}

function renderMatrixView() {
  const matrix = state.currentMatrix;
  const table = getEl('matrix-table');
  const banner = getEl('stitching-banner');
  const diffPanel = getEl('diff-panel');
  const diffList = getEl('diff-list');
  const fusionPanel = getEl('fusion-panel');
  const recList = getEl('fusion-rec-list');
  const aiBox = getEl('ai-synthesis-box');

  if (!matrix) return;

  if (table) {
    table.matrixMode = state.activeMode;
    const persp = matrix.perspectives && matrix.perspectives[state.activeMode];
    if (persp) {
      table.columnsData = persp.columns || [];
      table.rowsData = persp.rows || [];
    } else {
      table.columnsData = [];
      table.rowsData = [];
    }
  }

  if (banner) {
    const alerts = Array.isArray(matrix.stitching_alerts) ? matrix.stitching_alerts : [];
    clearChildren(banner);
    if (alerts.length > 0) {
      banner.hidden = false;
      banner.appendChild(el('strong', { text: `跨实施例强行拼凑预警（${alerts.length} 项）：` }));
      for (const alert of alerts) {
        banner.appendChild(el('div', {
          text: `[${alert.doc_id}] ${alert.rebuttal_rationale}`,
          style: { marginTop: '4px' },
        }));
      }
    } else {
      banner.hidden = true;
    }
  }

  if (diffPanel && diffList) {
    const diffs = Array.isArray(matrix.diffs) ? matrix.diffs : [];
    clearChildren(diffList);
    if (diffs.length > 0) {
      diffPanel.hidden = false;
      for (const d of diffs) {
        const pct = ((d.similarity_ratio || 0) * 100).toFixed(1);
        const stream = el('div', { className: 'matrix-diff-stream' });
        for (const op of (d.ops || [])) {
          const cls = op.op === 'add'
            ? 'matrix-diff-add'
            : (op.op === 'remove' ? 'matrix-diff-rm' : 'matrix-diff-eq');
          stream.appendChild(el('span', { className: cls, text: op.text || '' }));
        }
        diffList.appendChild(el('div', { className: 'matrix-diff-block' }, [
          el('div', {
            className: 'matrix-diff-meta',
            text: `${d.base_label} vs ${d.target_label} · 相似度 ${pct}%（+${d.added_chars} / -${d.removed_chars} 字符）`,
          }),
          stream,
        ]));
      }
    } else {
      diffPanel.hidden = true;
    }
  }

  if (fusionPanel && recList && aiBox) {
    const recs = Array.isArray(matrix.fusion_recommendations) ? matrix.fusion_recommendations : [];
    clearChildren(recList);
    clearChildren(aiBox);
    if (recs.length > 0 || matrix.ai_synthesis) {
      fusionPanel.hidden = false;
      for (const r of recs) {
        recList.appendChild(el('li', { text: r }));
      }
      if (matrix.ai_synthesis) {
        aiBox.appendChild(el('strong', { text: 'AI 综合融合建议：' }));
        aiBox.appendChild(el('div', { text: matrix.ai_synthesis, style: { marginTop: '4px' } }));
      }
    } else {
      fusionPanel.hidden = true;
    }
  }
}

function updateTabButtons() {
  const tabs = document.querySelectorAll('#perspective-tabs .matrix-tab-btn');
  tabs.forEach((btn) => {
    const mode = btn.getAttribute('data-mode');
    if (mode === state.activeMode) {
      btn.classList.add('is-active');
    } else {
      btn.classList.remove('is-active');
    }
  });
}

async function runCompare() {
  if (state.busy) return;
  let sources;
  try {
    sources = buildSourcesPayload();
  } catch (err) {
    showToast(err.message, 'error');
    return;
  }

  const titleEl = getEl('matrix-title');
  const aiChk = getEl('chk-ai-synthesis');
  const btn = getEl('btn-run-compare');

  state.busy = true;
  if (btn) btn.disabled = true;
  try {
    const res = await apiPost('/api/scheme-matrix/compare', {
      title: titleEl ? titleEl.value.trim() : '',
      mode: state.activeMode,
      sources,
      include_ai_synthesis: Boolean(aiChk && aiChk.checked),
    });
    state.currentMatrix = res && res.matrix ? res.matrix : res;
    renderMatrixView();
    showToast('多方案三维对决矩阵已生成', 'success');
  } catch (err) {
    showToast(err && err.message ? err.message : '矩阵生成失败', 'error');
  } finally {
    state.busy = false;
    if (btn) btn.disabled = false;
  }
}

async function handleExport(format) {
  if (!state.currentMatrix || !state.currentMatrix.matrix_id) {
    showToast('请先生成多方案对决矩阵后再导出', 'warn');
    return;
  }
  const matrixId = state.currentMatrix.matrix_id;
  const ackEl = getEl('chk-ack-priority');
  const acknowledge = Boolean(ackEl && ackEl.checked);

  try {
    const resp = await fetch(`/api/scheme-matrix/${encodeURIComponent(matrixId)}/export`, {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        format,
        mode: state.activeMode,
        acknowledge_priority_warning: acknowledge,
      }),
    });

    if (!resp.ok) {
      const errJson = await resp.json().catch(() => null);
      const msg = (errJson && errJson.message) || `导出失败 (HTTP ${resp.status})`;
      showToast(msg, 'error');
      return;
    }

    const blob = await resp.blob();
    const ext = format === 'pdf' ? 'pdf' : 'xlsx';
    const url = URL.createObjectURL(blob);
    const link = el('a', {
      href: url,
      download: `scheme_matrix_${matrixId}.${ext}`,
    });
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
    showToast(`已导出 ${ext.toUpperCase()} 文件`, 'success');
  } catch (err) {
    showToast(err && err.message ? err.message : '导出请求异常', 'error');
  }
}

function bindEvents() {
  const addBtn = getEl('btn-add-slot');
  if (addBtn) {
    addBtn.addEventListener('click', () => {
      if (state.slots.length >= MAX_SLOTS) {
        showToast(`最多支持并排对比 ${MAX_SLOTS} 个方案`, 'warn');
        return;
      }
      state.slots.push({
        label: `方案 ${state.slots.length + 1}`,
        docId: '',
        runId: '',
        role: 'candidate',
        inlineText: '',
        metricsText: '',
      });
      renderSlots();
    });
  }

  const vaultBtn = getEl('btn-open-vault');
  const picker = getEl('vault-picker');
  if (vaultBtn && picker) {
    vaultBtn.addEventListener('click', async () => {
      picker.show();
      try {
        await picker.loadFromApi('/api/vault/docs');
      } catch (err) {
        showToast(err && err.message ? err.message : '加载知识金库失败', 'error');
      }
    });

    picker.addEventListener('if-select-change', (ev) => {
      const selected = (ev.detail && Array.isArray(ev.detail.selectedItems))
        ? ev.detail.selectedItems
        : [];
      if (!selected.length) return;
      const sliced = selected.slice(0, MAX_SLOTS);
      state.slots = sliced.map((item) => ({
        label: item.title || item.id,
        docId: item.isExperiment ? '' : item.id,
        runId: item.isExperiment ? item.id : '',
        role: item.role || 'general_ref',
        inlineText: '',
        metricsText: '',
      }));
      while (state.slots.length < MIN_SLOTS) {
        state.slots.push({
          label: `方案 ${state.slots.length + 1}`,
          docId: '',
          runId: '',
          role: 'candidate',
          inlineText: '',
          metricsText: '',
        });
      }
      renderSlots();
    });
  }

  const runBtn = getEl('btn-run-compare');
  if (runBtn) {
    runBtn.addEventListener('click', runCompare);
  }

  const tabs = document.querySelectorAll('#perspective-tabs .matrix-tab-btn');
  tabs.forEach((btn) => {
    btn.addEventListener('click', () => {
      const mode = btn.getAttribute('data-mode') || 'engineering_spec';
      state.activeMode = mode;
      updateTabButtons();
      renderMatrixView();
    });
  });

  const table = getEl('matrix-table');
  if (table) {
    table.addEventListener('if-export-request', (ev) => {
      const format = (ev.detail && ev.detail.format) || 'xlsx';
      handleExport(format);
    });
    table.addEventListener('if-cell-anchor-click', (ev) => {
      const detail = ev.detail || {};
      showToast(`引证锚点：${detail.anchorTag || detail.value || '未命名'} (${detail.verdict || 'unverified'})`, 'info');
    });
  }

  const xlsxBtn = getEl('btn-export-xlsx');
  if (xlsxBtn) xlsxBtn.addEventListener('click', () => handleExport('xlsx'));

  const pdfBtn = getEl('btn-export-pdf');
  if (pdfBtn) pdfBtn.addEventListener('click', () => handleExport('pdf'));
}

function init() {
  if (typeof window !== 'undefined' && typeof window.renderSidebar === 'function') {
    window.renderSidebar('matrix');
  }
  renderSlots();
  updateTabButtons();
  bindEvents();
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', init);
} else {
  init();
}
