import { apiPost } from '../core/api-client.js';
import { el, clearChildren, setText } from '../core/dom-safe.js';
import { showToast } from '../core/toast.js';
import '../widgets/if-citation-chip.js';
import '../widgets/if-vault-picker.js';
import '../widgets/if-multimodal-box.js';

const state = {
  mode: 'patent_oa',
  caseId: null,
  currentCase: null,
  activeRoundIndex: -1,
};

function byId(id) {
  return document.getElementById(id);
}

function splitCsv(raw) {
  if (!raw) return [];
  return String(raw)
    .split(/[,，\n]/)
    .map((s) => s.trim())
    .filter(Boolean);
}

function updateDeadlineBadge(deadlineInfo) {
  const badge = byId('reb-deadline-badge');
  if (!badge) return;
  badge.classList.remove('is-urgent');
  if (!deadlineInfo || !deadlineInfo.deadline) {
    const dateVal = byId('reb-dispatch-date')?.value;
    if (!dateVal) {
      setText(badge, '待设定发文日');
      return;
    }
    setText(badge, `发文日: ${dateVal}（保存案件后计算法定期限）`);
    return;
  }
  const days = deadlineInfo.days_left;
  const label = days < 0
    ? `已逾期 ${Math.abs(days)} 天（截止 ${deadlineInfo.deadline}）`
    : `剩余 ${days} 天（截止 ${deadlineInfo.deadline}）`;
  setText(badge, label);
  if (deadlineInfo.is_urgent || days <= 7) {
    badge.classList.add('is-urgent');
  }
}

function renderRounds(rounds) {
  const bar = byId('reb-rounds-bar');
  if (!bar) return;
  clearChildren(bar);
  if (!Array.isArray(rounds) || rounds.length === 0) return;

  rounds.forEach((rnd, idx) => {
    const active = idx === (state.activeRoundIndex >= 0 ? state.activeRoundIndex : rounds.length - 1);
    const pill = el('button', {
      type: 'button',
      className: active ? 'round-pill is-active' : 'round-pill',
      text: rnd.round_label || `Round ${rnd.round_number || idx + 1}`,
      onClick: () => {
        state.activeRoundIndex = idx;
        const noticeBox = byId('reb-notice-box');
        if (noticeBox && typeof noticeBox.setValue === 'function') {
          noticeBox.setValue(rnd.notice_text || '');
        }
        if (rnd.draft_text) {
          byId('reb-draft-text').value = rnd.draft_text;
        }
        updateDeadlineBadge(rnd.deadline_info);
        renderRounds(rounds);
      },
    });
    bar.appendChild(pill);
  });
}

function renderIssues(issues) {
  const container = byId('reb-issues-list');
  if (!container) return;
  clearChildren(container);

  if (!Array.isArray(issues) || issues.length === 0) {
    container.appendChild(
      el('div', { className: 'muted', style: 'font-size:12px;', text: '暂无质疑拆解条目。' })
    );
    return;
  }

  for (const item of issues) {
    const sev = ['high', 'medium', 'low'].includes(item.severity) ? item.severity : 'medium';
    const chips = (item.cited_anchors || []).map((anc) =>
      el('if-citation-chip', {
        'doc-id': state.currentCase?.own_doc_id || '',
        'anchor-tag': anc,
        verdict: anc.includes('UNVERIFIED') ? 'unverified' : 'summary',
        summary: item.title || '',
      })
    );

    const card = el('div', { className: 'issue-card' }, [
      el('div', { className: 'issue-card-head' }, [
        el('span', { text: `[${item.issue_id || 'ISSUE'}] ${item.title || ''}` }),
        el('span', { className: `sev-pill sev-${sev}`, text: sev }),
      ]),
      el('div', {
        style: 'font-size:12px;color:var(--text-secondary,#c9d1d9);line-height:1.5;',
        text: item.detail || '',
      }),
      chips.length > 0 ? el('div', { className: 'chips-wrap' }, chips) : null,
    ]);
    container.appendChild(card);
  }
}

function renderCitationChips(citations) {
  const wrap = byId('reb-anchor-chips');
  if (!wrap) return;
  clearChildren(wrap);

  if (!Array.isArray(citations) || citations.length === 0) {
    wrap.appendChild(
      el('span', { className: 'muted', style: 'font-size:12px;', text: '核验后将在此展示引证锚点红黄绿芯片' })
    );
    return;
  }

  for (const c of citations) {
    const chip = el('if-citation-chip', {
      'doc-id': c.doc_id || '',
      'anchor-tag': c.anchor_tag || c.raw_mention || '',
      verdict: c.verdict || 'unverified',
      summary: c.matched_excerpt || c.reason || '',
    });
    wrap.appendChild(chip);
  }
}

function setDimBar(index, score) {
  const valEl = byId(`dim-val-${index}`);
  const barEl = byId(`dim-bar-${index}`);
  const num = typeof score === 'number' ? Math.max(0, Math.min(100, Math.round(score))) : 0;
  if (valEl) setText(valEl, `${num}`);
  if (barEl) {
    barEl.style.width = `${num}%`;
    barEl.style.background = num >= 85 ? '#3fb950' : num >= 60 ? '#d29922' : '#f85149';
  }
}

function renderGateReport(outcome) {
  const report = outcome?.gate_report;
  if (!report) return;

  const lamp = byId('reb-gate-lamp');
  const decEl = byId('reb-gate-decision-text');
  const scoreEl = byId('reb-gate-overall-score');

  if (lamp) {
    lamp.classList.remove('lamp-green', 'lamp-yellow', 'lamp-red');
    if (report.decision === 'pass_submittable') {
      lamp.classList.add('lamp-green');
      setText(decEl, '绿灯 · 核验通过可提交');
    } else if (report.decision === 'hard_blocked') {
      lamp.classList.add('lamp-red');
      setText(decEl, '红灯 · 硬阻断拦截');
    } else {
      lamp.classList.add('lamp-yellow');
      setText(decEl, '黄灯 · 待人工补证');
    }
  }
  if (scoreEl) {
    setText(scoreEl, `${Math.round(report.overall_score || 0)} / 100`);
  }

  setDimBar(1, report.anchor_accuracy_score);
  setDimBar(2, report.scope_compliance_score);
  setDimBar(3, report.logic_completeness_score);
  setDimBar(4, report.embodiment_consistency_score);
  setDimBar(5, report.priority_privacy_score);

  const alertsEl = byId('reb-gate-alerts');
  if (alertsEl) {
    clearChildren(alertsEl);
    const lines = [
      ...(report.blocking_reasons || []).map((m) => `【硬阻断】${m}`),
      ...(report.downgrade_warnings || []).map((m) => `【降级提醒】${m}`),
    ];
    if (outcome.priority_check?.advisory_zh) {
      lines.push(`【公开时序】${outcome.priority_check.advisory_zh}`);
    }
    if (lines.length === 0) {
      alertsEl.appendChild(el('li', { text: '五维全部通过，未发现超范围、拼凑或虚构引证。' }));
    } else {
      for (const msg of lines) {
        alertsEl.appendChild(el('li', { text: msg }));
      }
    }
  }

  renderCitationChips(report.verified_citations || []);
}

function collectCasePayload() {
  const noticeBox = byId('reb-notice-box');
  const noticeText = noticeBox && typeof noticeBox.getValue === 'function' ? noticeBox.getValue() : '';
  return {
    mode: state.mode,
    title: byId('reb-title')?.value?.trim() || '',
    own_doc_id: byId('reb-own-doc')?.value?.trim() || null,
    reference_doc_ids: splitCsv(byId('reb-ref-docs')?.value),
    experiment_ids: splitCsv(byId('reb-exp-ids')?.value),
    oa_type: byId('reb-oa-type')?.value || 'first_exam',
    dispatch_date: byId('reb-dispatch-date')?.value || null,
    original_claim_or_text: byId('reb-original-text')?.value || '',
    amended_claim_or_text: byId('reb-amended-text')?.value || '',
    notice_or_review_text: noticeText,
    knowhow_markers: splitCsv(byId('reb-knowhow')?.value),
  };
}

async function ensureCaseCreated() {
  if (state.caseId) return state.caseId;
  const payload = collectCasePayload();
  const res = await apiPost('/api/rebuttal-hub/case', payload);
  state.currentCase = res.case;
  state.caseId = res.case.case_id;
  setText(byId('rebuttal-case-status'), `案件 ${state.caseId} · 模式 ${state.mode}`);
  updateDeadlineBadge(res.case.deadline_info);
  renderRounds(res.case.rounds || []);
  return state.caseId;
}

async function handleCreateCase() {
  try {
    const payload = collectCasePayload();
    const res = await apiPost('/api/rebuttal-hub/case', payload);
    state.currentCase = res.case;
    state.caseId = res.case.case_id;
    state.activeRoundIndex = (res.case.rounds || []).length - 1;
    setText(byId('rebuttal-case-status'), `案件 ${state.caseId} · 模式 ${state.mode}`);
    updateDeadlineBadge(res.case.deadline_info);
    renderRounds(res.case.rounds || []);
    showToast('抗辩案件已创建并持久化', 'success');
  } catch (err) {
    showToast(`创建案件失败: ${err.message}`, 'error');
  }
}

async function handleAddRound() {
  try {
    const caseId = await ensureCaseCreated();
    const noticeBox = byId('reb-notice-box');
    const noticeText = noticeBox && typeof noticeBox.getValue === 'function' ? noticeBox.getValue() : '';
    const res = await apiPost(`/api/rebuttal-hub/case/${encodeURIComponent(caseId)}/round`, {
      oa_type: byId('reb-oa-type')?.value || 'second_rejection',
      dispatch_date: byId('reb-dispatch-date')?.value || null,
      notice_text: noticeText || '新增审查/审稿轮次',
      strategy_used: byId('reb-strategy-track')?.value || null,
      draft_text: byId('reb-draft-text')?.value || null,
    });
    state.currentCase = res.case;
    state.activeRoundIndex = (res.case.rounds || []).length - 1;
    updateDeadlineBadge(res.case.deadline_info);
    renderRounds(res.case.rounds || []);
    showToast(`已新增轮次：${res.round?.round_label || ''}`, 'success');
  } catch (err) {
    showToast(`新增轮次失败: ${err.message}`, 'error');
  }
}

async function handleMockReview() {
  try {
    const caseId = await ensureCaseCreated();
    const res = await apiPost(`/api/rebuttal-hub/case/${encodeURIComponent(caseId)}/mock-review`, {
      focus_query: byId('reb-title')?.value?.trim() || '',
    });
    renderIssues(res.issues || []);
    updateDeadlineBadge(res.deadline_info);
    showToast(`已生成 ${(res.issues || []).length} 条质疑拆解`, 'success');
  } catch (err) {
    showToast(`模拟审查失败: ${err.message}`, 'error');
  }
}

async function handleDraftRebuttal() {
  try {
    const caseId = await ensureCaseCreated();
    const track = byId('reb-strategy-track')?.value || 'amend_and_argue';
    const res = await apiPost(`/api/rebuttal-hub/case/${encodeURIComponent(caseId)}/draft`, {
      preferred_track: track,
      user_instruction: byId('reb-title')?.value?.trim() || '',
    });
    const draft = res.draft;
    if (draft) {
      if (byId('reb-draft-text')) byId('reb-draft-text').value = draft.full_draft_text || '';
      if (draft.suggested_amended_text && !byId('reb-amended-text').value.trim()) {
        byId('reb-amended-text').value = draft.suggested_amended_text;
      }
      showToast(`草稿已生成（轨道: ${draft.strategy_track}）`, 'success');
    }
  } catch (err) {
    showToast(`生成草稿失败: ${err.message}`, 'error');
  }
}

async function handleVerifyGate() {
  try {
    const caseId = await ensureCaseCreated();
    const res = await apiPost(`/api/rebuttal-hub/case/${encodeURIComponent(caseId)}/verify`, {
      draft_text: byId('reb-draft-text')?.value || '',
      original_claim_or_text: byId('reb-original-text')?.value || '',
      amended_claim_or_text: byId('reb-amended-text')?.value || '',
      require_citations: true,
    });
    renderGateReport(res);
    showToast('五维硬阻断门禁核验完成', 'info');
  } catch (err) {
    showToast(`门禁核验失败: ${err.message}`, 'error');
  }
}

async function handleExportDocx() {
  try {
    const caseId = await ensureCaseCreated();
    const format = byId('reb-export-format')?.value || 'docx';
    const resp = await fetch(`/api/rebuttal-hub/case/${encodeURIComponent(caseId)}/export`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      credentials: 'same-origin',
      body: JSON.stringify({ format, acknowledge_priority_warning: false }),
    });
    if (!resp.ok) {
      const errJson = await resp.json().catch(() => null);
      throw new Error(errJson?.message || `HTTP ${resp.status}`);
    }
    const blob = await resp.blob();
    const url = URL.createObjectURL(blob);
    const link = el('a', {
      href: url,
      download: `rebuttal_${caseId}.${format}`,
      style: 'display:none',
    });
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
    showToast(`已导出修订留痕答复包 (.${format})`, 'success');
  } catch (err) {
    showToast(`导出被拦截或失败: ${err.message}`, 'error');
  }
}

function switchMode(newMode) {
  state.mode = newMode;
  document.querySelectorAll('.mode-tab').forEach((btn) => {
    btn.classList.toggle('is-active', btn.dataset.mode === newMode);
  });
  setText(
    byId('rebuttal-case-status'),
    state.caseId ? `案件 ${state.caseId} · 模式 ${newMode}` : `当前模式: ${newMode} · 请创建案件`
  );
}

function bindEvents() {
  document.querySelectorAll('.mode-tab').forEach((btn) => {
    btn.addEventListener('click', () => switchMode(btn.dataset.mode || 'patent_oa'));
  });

  byId('reb-dispatch-date')?.addEventListener('change', () => updateDeadlineBadge(null));
  byId('btn-create-case')?.addEventListener('click', handleCreateCase);
  byId('btn-add-round')?.addEventListener('click', handleAddRound);
  byId('btn-mock-review')?.addEventListener('click', handleMockReview);
  byId('btn-draft-rebuttal')?.addEventListener('click', handleDraftRebuttal);
  byId('btn-verify-gate')?.addEventListener('click', handleVerifyGate);
  byId('btn-export-docx')?.addEventListener('click', handleExportDocx);

  const picker = byId('reb-vault-picker');
  byId('btn-open-vault')?.addEventListener('click', () => {
    if (picker && typeof picker.show === 'function') {
      picker.show();
      if (typeof picker.loadFromApi === 'function') {
        picker.loadFromApi('/api/vault/docs').catch(() => {});
      }
    }
  });

  picker?.addEventListener('if-vault-confirm', (ev) => {
    const ids = ev?.detail?.selectedIds || [];
    if (ids.length > 0 && byId('reb-ref-docs')) {
      const existing = splitCsv(byId('reb-ref-docs').value);
      const merged = [...new Set([...existing, ...ids])];
      byId('reb-ref-docs').value = merged.join(', ');
      showToast(`已挂载 ${ids.length} 篇金库文献`, 'success');
    }
  });
}

document.addEventListener('DOMContentLoaded', () => {
  bindEvents();
  renderCitationChips([]);
});
