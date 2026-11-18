import { apiPost } from '../core/api-client.js';
import { clearChildren, el, setText, tr } from '../core/dom-safe.js';
import { connectSSE } from '../core/sse-client.js';
import { showToast } from '../core/toast.js';
import '../widgets/if-multimodal-box.js';
import '../widgets/if-vault-picker.js';
import '../widgets/if-citation-chip.js';

const state = {
  sessionId: null,
  baseline: null,
  mountedDocIds: [],
  turns: [],
  selectedTier: 'quick_scout',
  runId: null,
  parentRunId: null,
  nodes: [],
  childForks: [],
  sseHandle: null,
  pendingCheckpointStep: null,
};

function getEl(id) {
  return document.getElementById(id);
}

function switchCabin(target) {
  const front = getEl('cabin-front');
  const rear = getEl('cabin-rear');
  const tabFront = getEl('tab-front-cabin');
  const tabRear = getEl('tab-rear-cabin');
  const isFront = target === 'front';

  if (front) front.classList.toggle('is-hidden', !isFront);
  if (rear) rear.classList.toggle('is-hidden', isFront);
  if (tabFront) {
    tabFront.classList.toggle('is-active', isFront);
    tabFront.setAttribute('aria-selected', String(isFront));
  }
  if (tabRear) {
    tabRear.classList.toggle('is-active', !isFront);
    tabRear.setAttribute('aria-selected', String(!isFront));
  }
}

async function ensureSession() {
  if (state.sessionId) return state.sessionId;
  const resp = await apiPost('/api/idea-studio/session', {
    title: '',
    mounted_own_doc_ids: state.mountedDocIds,
  });
  if (resp && resp.session_id) {
    state.sessionId = resp.session_id;
    state.baseline = resp.session ? resp.session.baseline : null;
    updateSessionMeta();
    renderBaselineCard();
    return state.sessionId;
  }
  throw new Error((resp && resp.message) || 'Failed to create studio session');
}

async function createFreshSession() {
  state.sessionId = null;
  state.turns = [];
  state.baseline = null;
  renderChatLog();
  await ensureSession();
  showToast(tr('studio.session.created', null, '已创建新共创会话'), 'success');
}

function updateSessionMeta() {
  const meta = getEl('studio-session-meta');
  if (!meta) return;
  const parts = [];
  if (state.sessionId) parts.push(`会话: ${state.sessionId}`);
  if (state.runId) parts.push(`运行: ${state.runId}`);
  if (state.mountedDocIds.length) parts.push(`已挂载自有资产: ${state.mountedDocIds.length} 项`);
  setText(meta, parts.length ? parts.join(' · ') : '前舱多轮图文共创收敛基线卡 · 后舱三档推演、A/B 分叉与实测证据锚点');
}

async function uploadMultimodalFiles(items) {
  const uploaded = [];
  for (const item of items) {
    if (!item || !item.file) continue;
    const form = new FormData();
    form.append('file', item.file, item.name || 'attachment.bin');
    const res = await apiPost('/api/multimodal/upload', form);
    if (res && res.attachment_id) {
      uploaded.push({
        attachment_id: res.attachment_id,
        filename: res.filename || item.name || 'attachment',
        mime_type: res.mime_type || item.mime || 'application/octet-stream',
        extracted_text: res.extracted_text || '',
        tier_used: res.tier_used || 'native_text_layer',
        page_count: 1,
        figure_numerals: Array.isArray(res.figure_numerals) ? res.figure_numerals : [],
        structured_summary_json: res.structured_summary || null,
        warnings: [],
      });
    }
  }
  return uploaded;
}

function renderChatLog() {
  const log = getEl('studio-chat-log');
  if (!log) return;
  clearChildren(log);

  if (!state.turns.length) {
    log.appendChild(el('div', {
      className: 'studio-turn',
      text: '请在下方输入技术构想、粘贴截图或拖入 PDF/DOCX/CSV 文件，系统将多轮追问并实时收敛右侧基线卡。',
    }));
    return;
  }

  for (const turn of state.turns) {
    const card = el('div', { className: 'studio-turn' }, [
      el('div', { className: 'studio-turn-role', text: turn.role === 'user' ? '工程师输入' : '创研架构师' }),
      el('div', { text: turn.text }),
    ]);
    if (Array.isArray(turn.followUps) && turn.followUps.length) {
      const strip = el('div', { className: 'studio-followups' });
      for (const q of turn.followUps) {
        strip.appendChild(el('button', {
          type: 'button',
          className: 'studio-followup-btn',
          text: q,
          onClick: () => {
            const box = getEl('studio-multimodal-box');
            if (box && typeof box.setValue === 'function') {
              const cur = box.getValue();
              box.setValue(cur ? `${cur}\n${q}` : q);
            }
          },
        }));
      }
      card.appendChild(strip);
    }
    log.appendChild(card);
  }
  log.scrollTop = log.scrollHeight;
}

function renderBaselineField(label, value) {
  const wrap = el('div', { className: 'baseline-field' }, [
    el('div', { className: 'baseline-label', text: label }),
  ]);
  if (Array.isArray(value)) {
    if (!value.length) {
      wrap.appendChild(el('div', { style: 'font-size:13px; color:#8b949e;', text: '待补充' }));
    } else {
      const ul = el('ul', { className: 'baseline-list' });
      for (const item of value) {
        const text = typeof item === 'string'
          ? item
          : `${item.numeral_code || ''} ${item.part_name || ''}`.trim();
        ul.appendChild(el('li', { text }));
      }
      wrap.appendChild(ul);
    }
  } else {
    wrap.appendChild(el('div', {
      style: 'font-size:13px;',
      text: value && String(value).trim() ? String(value) : '待收敛',
    }));
  }
  return wrap;
}

function renderBaselineCard() {
  const container = getEl('baseline-card-view');
  const badge = getEl('baseline-lock-badge');
  if (!container) return;
  clearChildren(container);

  const b = state.baseline || {};
  if (badge) {
    setText(badge, b.locked ? '基线已锁定' : '草稿收敛中');
    badge.className = b.locked ? 'dag-status-badge is-completed' : 'dag-status-badge';
  }

  container.appendChild(renderBaselineField('方案标题', b.title));
  container.appendChild(renderBaselineField('核心技术痛点', b.core_problem));
  container.appendChild(renderBaselineField('关键技术手段', b.technical_means || []));
  container.appendChild(renderBaselineField('核心部件标号', b.key_components || []));
  container.appendChild(renderBaselineField('预期技术效果', b.expected_effects || []));
  container.appendChild(renderBaselineField('对照实验指标', b.experiment_metrics || []));
  container.appendChild(renderBaselineField('已挂载我方前序资产', b.mounted_own_doc_ids || state.mountedDocIds));
  container.appendChild(renderBaselineField('待澄清追问', b.unresolved_questions || []));
}

async function handleMultimodalSubmit(detail) {
  const text = (detail && detail.text) || '';
  const files = (detail && detail.attachments) || [];
  const images = (detail && detail.images) || [];
  if (!text && !files.length && !images.length) {
    showToast('请输入想法或添加图文附件', 'warn');
    return;
  }

  try {
    const sessionId = await ensureSession();
    const allItems = [...files, ...images];
    const uploaded = allItems.length ? await uploadMultimodalFiles(allItems) : [];

    state.turns.push({ role: 'user', text: text || `[提交 ${uploaded.length} 份附件]` });
    renderChatLog();

    const resp = await apiPost(`/api/idea-studio/session/${encodeURIComponent(sessionId)}/turn`, {
      user_text: text,
      attachments: uploaded,
      mounted_own_doc_ids: state.mountedDocIds,
      lock_baseline: false,
    });

    if (resp && resp.baseline) {
      state.baseline = resp.baseline;
      renderBaselineCard();
    }
    state.turns.push({
      role: 'assistant',
      text: (resp && resp.reply_message) || '已更新方案基线卡。',
      followUps: (resp && resp.follow_up_questions) || [],
    });
    renderChatLog();

    const box = getEl('studio-multimodal-box');
    if (box && typeof box.clear === 'function') {
      box.clear();
    }
  } catch (err) {
    showToast(err && err.message ? err.message : '共创轮次提交失败', 'error');
  }
}

async function lockBaselineAndEnterRear() {
  try {
    const sessionId = await ensureSession();
    if (!state.baseline || !state.baseline.locked) {
      const resp = await apiPost(`/api/idea-studio/session/${encodeURIComponent(sessionId)}/turn`, {
        user_text: '确认锁定当前技术方案基线并进入后舱推演',
        attachments: [],
        mounted_own_doc_ids: state.mountedDocIds,
        lock_baseline: true,
      });
      if (resp && resp.baseline) {
        state.baseline = resp.baseline;
        renderBaselineCard();
      }
    }
    switchCabin('rear');
    showToast('基线卡已锁定，可选择运行深度启动推演', 'success');
  } catch (err) {
    showToast(err && err.message ? err.message : '锁定基线失败', 'error');
  }
}

function renderDagRail() {
  const rail = getEl('dag-step-rail');
  const genBadge = getEl('run-genealogy-badge');
  if (!rail) return;
  clearChildren(rail);

  if (genBadge) {
    const parts = [];
    if (state.parentRunId) parts.push(`分叉自 ${state.parentRunId}`);
    if (state.childForks.length) parts.push(`子路线数: ${state.childForks.length}`);
    setText(genBadge, parts.join(' · '));
  }

  if (!state.nodes.length) {
    rail.appendChild(el('div', {
      style: 'font-size:13px; color:#8b949e;',
      text: '点击右上角“启动推演流水线”后，此处将实时展示各步骤节点并支持任意步分叉。',
    }));
    return;
  }

  for (const node of state.nodes) {
    const statusClass = node.status === 'completed'
      ? 'dag-status-badge is-completed'
      : (node.status === 'paused_checkpoint' ? 'dag-status-badge is-paused' : 'dag-status-badge');

    let summaryText = '';
    if (node.output_json && node.output_json !== '{}') {
      try {
        const parsed = JSON.parse(node.output_json);
        summaryText = parsed.summary || JSON.stringify(parsed);
      } catch {
        summaryText = node.output_json;
      }
    }

    const forkBtn = el('button', {
      type: 'button',
      className: 'studio-btn-secondary',
      text: '分叉推演 (对比路线 B)',
      onClick: () => forkFromStep(node.step_key),
    });

    const card = el('div', { className: 'dag-step-card' }, [
      el('div', { className: 'dag-step-head' }, [
        el('strong', { text: `${node.step_index + 1}. ${node.step_key}` }),
        el('div', { style: 'display:flex; gap:8px; align-items:center;' }, [
          el('span', { className: statusClass, text: node.status }),
          forkBtn,
        ]),
      ]),
      summaryText ? el('div', { style: 'font-size:13px; color:#c9d1d9;', text: summaryText }) : null,
    ]);
    rail.appendChild(card);
  }
}

function showCheckpointPanel(stepKey, candidates) {
  const panel = getEl('checkpoint-panel');
  if (!panel) return;
  state.pendingCheckpointStep = stepKey || 'experiment_design_checkpoint';
  panel.classList.remove('is-hidden');
  const titleEl = getEl('cp-title');
  const ctrlEl = getEl('cp-control');
  const testEl = getEl('cp-test');
  if (titleEl && candidates && candidates.title) titleEl.value = candidates.title;
  if (ctrlEl && candidates && candidates.control_group_spec) ctrlEl.value = candidates.control_group_spec;
  if (testEl && candidates && candidates.test_group_spec) testEl.value = candidates.test_group_spec;
}

function hideCheckpointPanel() {
  const panel = getEl('checkpoint-panel');
  if (panel) panel.classList.add('is-hidden');
  state.pendingCheckpointStep = null;
}

function bindRunSse(runId) {
  if (state.sseHandle && typeof state.sseHandle.close === 'function') {
    state.sseHandle.close();
  }
  const progressBar = getEl('run-progress-bar');

  state.sseHandle = connectSSE(`/api/idea-studio/run/${encodeURIComponent(runId)}/events`, {
    stage_update: (payload) => {
      if (progressBar) {
        setText(progressBar, `[${payload.percent ?? 0}%] ${payload.stage || ''}: ${payload.message || ''}`);
      }
      refreshNodeStatusFromEvent(payload.stage, 'completed');
    },
    checkpoint_required: (payload) => {
      if (progressBar) {
        setText(progressBar, `卡点暂停：${payload.checkpoint_kind || 'experiment_design_checkpoint'} 等待人工微调确认`);
      }
      showCheckpointPanel(payload.checkpoint_kind, payload.candidates || {});
    },
    completed: () => {
      if (progressBar) {
        setText(progressBar, `运行 ${runId} 已全部完成`);
      }
      hideCheckpointPanel();
    },
    error: (payload) => {
      showToast((payload && payload.message_zh) || '流水线执行出错', 'error');
    },
  });
}

function refreshNodeStatusFromEvent(stepKey, status) {
  if (!stepKey || !state.nodes.length) return;
  let changed = false;
  for (const node of state.nodes) {
    if (node.step_key === stepKey && node.status === 'pending') {
      node.status = status;
      changed = true;
    }
  }
  if (changed) renderDagRail();
}

async function startPipelineRun() {
  try {
    const resp = await apiPost('/api/idea-studio/run/start', {
      session_id: state.sessionId,
      tier_mode: state.selectedTier,
      scheme_baseline: state.baseline,
      mounted_own_doc_ids: state.mountedDocIds,
    });
    state.runId = resp.run_id;
    state.parentRunId = resp.run ? resp.run.parent_run_id : null;
    state.nodes = Array.isArray(resp.nodes) ? resp.nodes : [];
    state.childForks = [];
    hideCheckpointPanel();
    updateSessionMeta();
    renderDagRail();
    bindRunSse(state.runId);
    showToast(`已启动推演 (${state.selectedTier})`, 'success');
  } catch (err) {
    showToast(err && err.message ? err.message : '启动推演失败', 'error');
  }
}

async function forkFromStep(stepKey) {
  if (!state.runId) {
    showToast('请先启动一次推演运行', 'warn');
    return;
  }
  try {
    const resp = await apiPost(`/api/idea-studio/run/${encodeURIComponent(state.runId)}/fork`, {
      fork_from_step: stepKey,
      override_baseline: state.baseline,
      route_label: '路线 B',
    });
    state.runId = resp.run_id;
    state.parentRunId = resp.parent_run_id;
    state.nodes = Array.isArray(resp.nodes) ? resp.nodes : [];
    state.childForks = (resp.genealogy && resp.genealogy.child_forks) || [];
    hideCheckpointPanel();
    updateSessionMeta();
    renderDagRail();
    bindRunSse(state.runId);
    showToast(`已从 ${stepKey} 分叉出对比路线 B (${state.runId})`, 'success');
  } catch (err) {
    showToast(err && err.message ? err.message : '分叉推演失败', 'error');
  }
}

async function resumeCheckpoint() {
  if (!state.runId) return;
  const titleEl = getEl('cp-title');
  const ctrlEl = getEl('cp-control');
  const testEl = getEl('cp-test');
  try {
    const resp = await apiPost(`/api/idea-studio/run/${encodeURIComponent(state.runId)}/checkpoint`, {
      step_key: state.pendingCheckpointStep || 'experiment_design_checkpoint',
      confirmed_output: {
        title: titleEl ? titleEl.value : '',
        control_group_spec: ctrlEl ? ctrlEl.value : '',
        test_group_spec: testEl ? testEl.value : '',
        confirmed: true,
      },
    });
    state.nodes = Array.isArray(resp.nodes) ? resp.nodes : state.nodes;
    hideCheckpointPanel();
    renderDagRail();
    showToast('卡点已确认，继续推进后续步骤', 'success');
  } catch (err) {
    showToast(err && err.message ? err.message : '恢复卡点失败', 'error');
  }
}

async function submitExperimentBackfill(event) {
  event.preventDefault();
  const runId = state.runId || 'standalone_run';
  const title = (getEl('exp-title')?.value || '').trim();
  const hypothesis = (getEl('exp-hypothesis')?.value || '').trim();
  const control = (getEl('exp-control')?.value || '').trim();
  const test = (getEl('exp-test')?.value || '').trim();
  const rawMeasured = (getEl('exp-measured-json')?.value || '').trim();
  const verified = Boolean(getEl('exp-verified')?.checked);

  let measuredJson;
  try {
    measuredJson = JSON.parse(rawMeasured);
  } catch {
    showToast('实测数据必须是合法的 JSON 对象', 'error');
    return;
  }

  try {
    const resp = await apiPost(`/api/idea-studio/run/${encodeURIComponent(runId)}/experiment`, {
      title,
      hypothesis,
      control_group_spec: control,
      test_group_spec: test,
      measured_data_json: measuredJson,
      verified,
    });

    const anchor = resp.evidence_anchor || `Exp:${resp.exp_id}`;
    const list = getEl('experiment-anchors-list');
    if (list) {
      const chip = el('if-citation-chip', {
        'doc-id': resp.exp_id,
        'anchor-tag': anchor,
        verdict: verified ? 'verified' : 'summary',
        summary: title,
      });
      list.appendChild(chip);
    }
    showToast(`实测数据已入库，生成铁证锚点 ${anchor}`, 'success');
  } catch (err) {
    showToast(err && err.message ? err.message : '实测数据回填失败', 'error');
  }
}

function initStudio() {
  getEl('tab-front-cabin')?.addEventListener('click', () => switchCabin('front'));
  getEl('tab-rear-cabin')?.addEventListener('click', () => switchCabin('rear'));
  getEl('btn-new-session')?.addEventListener('click', () => createFreshSession());
  getEl('btn-lock-enter-rear')?.addEventListener('click', () => lockBaselineAndEnterRear());
  getEl('btn-start-run')?.addEventListener('click', () => startPipelineRun());
  getEl('btn-resume-checkpoint')?.addEventListener('click', () => resumeCheckpoint());
  getEl('experiment-backfill-form')?.addEventListener('submit', submitExperimentBackfill);

  const picker = getEl('studio-vault-picker');
  const box = getEl('studio-multimodal-box');

  getEl('btn-toggle-vault-picker')?.addEventListener('click', () => {
    if (!picker) return;
    if (picker.isOpen) {
      picker.hide();
    } else {
      picker.show();
      if (typeof picker.loadFromApi === 'function') {
        picker.loadFromApi('/api/vault/docs').catch(() => {});
      }
    }
  });

  if (picker) {
    picker.addEventListener('if-select-change', (ev) => {
      const detail = ev.detail || {};
      const items = Array.isArray(detail.selectedItems) ? detail.selectedItems : [];
      state.mountedDocIds = Array.isArray(detail.docIds) ? detail.docIds : [];
      if (box && typeof box.setMountedDocs === 'function') {
        box.setMountedDocs(items.map((it) => ({ id: it.id, title: it.title })));
      }
      updateSessionMeta();
      renderBaselineCard();
    });
  }

  if (box) {
    box.addEventListener('if-submit', (ev) => handleMultimodalSubmit(ev.detail));
    box.addEventListener('if-open-vault-picker', () => {
      if (picker && typeof picker.show === 'function') {
        picker.show();
        if (typeof picker.loadFromApi === 'function') {
          picker.loadFromApi('/api/vault/docs').catch(() => {});
        }
      }
    });
  }

  for (const btn of document.querySelectorAll('.tier-btn')) {
    btn.addEventListener('click', () => {
      for (const b of document.querySelectorAll('.tier-btn')) {
        b.classList.remove('is-active');
      }
      btn.classList.add('is-active');
      state.selectedTier = btn.getAttribute('data-tier') || 'quick_scout';
    });
  }

  renderChatLog();
  renderBaselineCard();
  renderDagRail();
}

if (typeof document !== 'undefined') {
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initStudio);
  } else {
    initStudio();
  }
}
