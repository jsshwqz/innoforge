(function () {
  'use strict';

  const INTENT_PATTERNS = [
    /(?:画|绘制|建模|生成).{0,12}(?:3d|三维|结构|零件|模型|图)/i,
    /freecad.{0,12}(?:画|模型|建模|图)/i,
    /(?:给我看图|画出来看看|生成3d结构)/i
  ];

  function tr(key, fallback) {
    return typeof window.t === 'function' ? window.t(key) : fallback;
  }

  function el(tag, className, text) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined) node.textContent = text;
    return node;
  }

  function createController(options) {
    const state = { busy: false, latest: null, parentForNext: null, originalPrompt: '' };

    function shouldHandle(prompt) {
      return INTENT_PATTERNS.some((pattern) => pattern.test(prompt || ''));
    }

    function messageContainer() {
      return typeof options.messages === 'function' ? options.messages() : options.messages;
    }

    function addProgress(prompt) {
      const card = el('section', 'cad-card cad-card-progress');
      const title = el('div', 'cad-card-title', tr('cad.drawing', 'FreeCAD drawing'));
      const phase = el('div', 'cad-progress-text', tr('cad.checking', 'Checking FreeCAD…'));
      card.appendChild(title);
      card.appendChild(phase);
      card.dataset.originalPrompt = prompt;
      messageContainer().appendChild(card);
      return { card, phase };
    }

    function button(label, handler, className) {
      const node = el('button', className || 'btn btn-secondary', label);
      node.type = 'button';
      node.addEventListener('click', handler);
      return node;
    }

    function openFullscreen(artifact) {
      const overlay = el('div', 'cad-fullscreen');
      overlay.tabIndex = -1;
      const close = button('×', () => overlay.remove(), 'cad-fullscreen-close');
      close.setAttribute('aria-label', tr('cad.close', 'Close'));
      const image = el('img', 'cad-fullscreen-image');
      image.src = `/api/cad/artifacts/${encodeURIComponent(artifact.id)}/preview`;
      image.alt = tr('cad.preview', 'FreeCAD preview');
      overlay.appendChild(close);
      overlay.appendChild(image);
      overlay.addEventListener('keydown', (event) => {
        if (event.key === 'Escape') overlay.remove();
      });
      document.body.appendChild(overlay);
      overlay.focus();
    }

    function renderArtifact(holder, artifact, warnings) {
      holder.className = 'cad-card cad-card-complete';
      while (holder.firstChild) holder.removeChild(holder.firstChild);
      const heading = el('div', 'cad-card-title', `${tr('cad.revision', 'Revision')} ${artifact.revision}`);
      const image = el('img', 'cad-preview');
      image.src = `/api/cad/artifacts/${encodeURIComponent(artifact.id)}/preview`;
      image.alt = tr('cad.preview', 'FreeCAD preview');
      image.addEventListener('click', () => openFullscreen(artifact));
      holder.appendChild(heading);
      holder.appendChild(image);

      if (artifact.assumptions && artifact.assumptions.length) {
        const assumptions = el('div', 'cad-assumptions');
        assumptions.appendChild(el('strong', '', tr('cad.assumptions', 'Assumptions')));
        const list = el('ul');
        artifact.assumptions.forEach((item) => list.appendChild(el('li', '', item)));
        assumptions.appendChild(list);
        holder.appendChild(assumptions);
      }
      const validationText = artifact.validation && artifact.validation.valid
        ? tr('cad.valid', 'Shape validation passed')
        : tr('cad.invalid', 'Shape needs review');
      holder.appendChild(el('div', 'cad-validation', validationText));
      holder.appendChild(el('time', 'cad-time', artifact.created_at || ''));
      if (warnings && warnings.length) holder.appendChild(el('div', 'cad-warning', warnings.join('；')));

      const actions = el('div', 'cad-actions');
      actions.appendChild(button(tr('cad.continue', 'Continue modifying'), () => {
        state.latest = artifact;
        state.parentForNext = artifact.id;
        if (typeof options.onContinue === 'function') options.onContinue(artifact);
      }));
      actions.appendChild(button(tr('cad.fullscreen', 'Fullscreen'), () => openFullscreen(artifact)));
      const fcstd = el('a', 'btn btn-secondary', 'FCStd');
      fcstd.href = `/api/cad/artifacts/${encodeURIComponent(artifact.id)}/download/fcstd`;
      fcstd.download = '';
      actions.appendChild(fcstd);
      if (artifact.step_rel_path) {
        const step = el('a', 'btn btn-secondary', 'STEP');
        step.href = `/api/cad/artifacts/${encodeURIComponent(artifact.id)}/download/step`;
        step.download = '';
        actions.appendChild(step);
      }
      holder.appendChild(actions);
    }

    function renderDegraded(holder, error) {
      holder.className = 'cad-card cad-card-degraded';
      while (holder.firstChild) holder.removeChild(holder.firstChild);
      holder.appendChild(el('div', 'cad-card-title', tr('cad.unavailable', 'FreeCAD unavailable')));
      holder.appendChild(el('div', 'cad-warning', error || tr('cad.textContinues', 'Text chat is still available.')));
      holder.appendChild(button(tr('cad.retry', 'Retry'), () => draw(state.originalPrompt, state.latest && state.latest.id)));
    }

    async function draw(prompt, parentArtifactId) {
      if (state.busy || !(prompt || '').trim()) return false;
      state.busy = true;
      state.originalPrompt = prompt;
      const progress = addProgress(prompt);
      try {
        progress.phase.textContent = tr('cad.starting', 'Starting FreeCAD…');
        const context = options.context();
        progress.phase.textContent = tr('cad.briefing', 'Preparing complete drawing brief…');
        const response = await fetch('/api/cad/draw', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            context_kind: context.kind,
            context_id: context.id,
            prompt: prompt,
            conversation_context: typeof options.history === 'function' ? options.history() : '',
            parent_artifact_id: parentArtifactId || state.parentForNext || null
          })
        });
        const payload = await response.json();
        if (!response.ok) throw new Error(payload.error || tr('cad.failed', 'Drawing failed'));
        state.latest = payload.artifact;
        state.parentForNext = null;
        renderArtifact(progress.card, payload.artifact, payload.warnings || []);
      } catch (error) {
        renderDegraded(progress.card, error.message);
      } finally {
        state.busy = false;
      }
      return true;
    }

    async function restore() {
      const context = options.context();
      if (!context.id) return;
      const url = `/api/cad/artifacts?context_kind=${encodeURIComponent(context.kind)}&context_id=${encodeURIComponent(context.id)}`;
      try {
        const response = await fetch(url);
        if (!response.ok) return;
        const payload = await response.json();
        (payload.artifacts || []).slice().reverse().forEach((artifact) => {
          const holder = el('section', 'cad-card');
          messageContainer().appendChild(holder);
          renderArtifact(holder, artifact, []);
          state.latest = artifact;
        });
      } catch (_) {
        // History restoration is best-effort and never blocks ordinary chat.
      }
    }

    return {
      shouldHandle: shouldHandle,
      draw: draw,
      drawFromInput: () => {
        const input = typeof options.input === 'function' ? options.input() : options.input;
        return draw(input ? input.value : '');
      },
      intercept: (prompt) => shouldHandle(prompt) ? draw(prompt) : false,
      restore: restore,
      latest: () => state.latest
    };
  }

  window.InnoForgeCad = { createController: createController };
}());
