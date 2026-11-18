import { el, clearChildren, tr } from '../core/dom-safe.js';

export class IfMultimodalBox extends HTMLElement {
  static get observedAttributes() {
    return ['placeholder-key', 'accept-types', 'allow-vault-mount'];
  }

  constructor() {
    super();
    this._images = [];
    this._attachments = [];
    this._mountedDocs = [];
    this._text = '';
    this._dragOver = false;
    this._textarea = null;
    this._fileInput = null;
    this._previewStrip = null;
  }

  connectedCallback() {
    this._render();
  }

  attributeChangedCallback(name) {
    if (!this.isConnected) return;
    if (name === 'placeholder-key' && this._textarea) {
      this._textarea.setAttribute('placeholder', this._resolvePlaceholder());
      return;
    }
    if (name === 'accept-types' && this._fileInput) {
      const accept = this.getAttribute('accept-types');
      if (accept) this._fileInput.setAttribute('accept', accept);
      else this._fileInput.removeAttribute('accept');
      return;
    }
    this._render();
  }

  get allowVaultMount() {
    if (!this.hasAttribute('allow-vault-mount')) return false;
    const val = this.getAttribute('allow-vault-mount');
    return val !== 'false' && val !== '0';
  }

  getValue() {
    return this._textarea ? this._textarea.value : this._text;
  }

  setValue(val) {
    this._text = val == null ? '' : String(val);
    if (this._textarea) {
      this._textarea.value = this._text;
    }
  }

  getAttachedFiles() {
    return [...this._attachments];
  }

  setAttachedFiles(files) {
    this._attachments = Array.isArray(files)
      ? files.map((f, idx) => ({
          id: f && f.id != null ? String(f.id) : `att-${idx}`,
          name: f && f.name != null ? String(f.name) : 'file',
          size: f && typeof f.size === 'number' ? f.size : 0,
          mime: f && f.mime ? String(f.mime) : '',
          status: f && f.status ? String(f.status) : 'ready',
          file: f && f.file ? f.file : null,
        }))
      : [];
    this._renderPreviews();
  }

  addAttachment(item) {
    if (!item) return;
    this._attachments.push({
      id: item.id != null ? String(item.id) : `att-${Date.now()}-${this._attachments.length}`,
      name: item.name != null ? String(item.name) : 'file',
      size: typeof item.size === 'number' ? item.size : 0,
      mime: item.mime ? String(item.mime) : '',
      status: item.status ? String(item.status) : 'ready',
      file: item.file || null,
    });
    this._renderPreviews();
  }

  getMountedDocs() {
    return [...this._mountedDocs];
  }

  setMountedDocs(docs) {
    this._mountedDocs = Array.isArray(docs)
      ? docs.map((d, idx) => ({
          id: d && d.id != null ? String(d.id) : `doc-${idx}`,
          title: d && d.title != null ? String(d.title) : String(d?.id || ''),
        }))
      : [];
    this._renderPreviews();
  }

  getPastedImages() {
    return [...this._images];
  }

  addPastedImage(img) {
    if (!img) return;
    this._images.push({
      id: img.id != null ? String(img.id) : `img-${Date.now()}-${this._images.length}`,
      name: img.name != null ? String(img.name) : 'pasted-image.png',
      mime: img.mime ? String(img.mime) : 'image/png',
      size: typeof img.size === 'number' ? img.size : 0,
      previewUrl: img.previewUrl || '',
      file: img.file || null,
    });
    this._renderPreviews();
  }

  clear() {
    this._text = '';
    this._images = [];
    this._attachments = [];
    this._mountedDocs = [];
    if (this._textarea) this._textarea.value = '';
    this._renderPreviews();
  }

  submit() {
    const text = this.getValue().trim();
    const attachmentIds = this._attachments
      .map((a) => a.id)
      .filter((id) => Boolean(id));
    const mountedDocIds = this._mountedDocs
      .map((d) => d.id)
      .filter((id) => Boolean(id));

    this.dispatchEvent(new CustomEvent('if-submit', {
      bubbles: true,
      composed: true,
      detail: {
        text,
        images: [...this._images],
        attachmentIds,
        attachments: [...this._attachments],
        mountedDocIds,
        mountedDocs: [...this._mountedDocs],
      },
    }));
  }

  _resolvePlaceholder() {
    const key = this.getAttribute('placeholder-key') || 'widget.multimodal.placeholder';
    return tr(key, null, 'Describe your idea, paste screenshots, or drop PDF/DOCX/CSV files…');
  }

  _handlePaste(event) {
    const items = event && event.clipboardData && event.clipboardData.items
      ? Array.from(event.clipboardData.items)
      : [];
    const imageItems = items.filter((it) => it && typeof it.type === 'string' && it.type.startsWith('image/'));
    if (!imageItems.length) return;

    event.preventDefault();
    for (const item of imageItems) {
      const file = typeof item.getAsFile === 'function' ? item.getAsFile() : null;
      if (!file) continue;
      const previewUrl = typeof URL !== 'undefined' && typeof URL.createObjectURL === 'function'
        ? URL.createObjectURL(file)
        : '';
      this.addPastedImage({
        name: file.name || `screenshot-${this._images.length + 1}.png`,
        mime: file.type || item.type,
        size: file.size || 0,
        previewUrl,
        file,
      });
    }
  }

  _handleFiles(fileList) {
    if (!fileList || !fileList.length) return;
    for (const file of Array.from(fileList)) {
      if (!file) continue;
      if (typeof file.type === 'string' && file.type.startsWith('image/')) {
        const previewUrl = typeof URL !== 'undefined' && typeof URL.createObjectURL === 'function'
          ? URL.createObjectURL(file)
          : '';
        this.addPastedImage({
          name: file.name || 'image.png',
          mime: file.type,
          size: file.size || 0,
          previewUrl,
          file,
        });
      } else {
        this.addAttachment({
          name: file.name || 'attachment',
          mime: file.type || '',
          size: file.size || 0,
          file,
        });
      }
    }
  }

  _renderPreviews() {
    if (!this._previewStrip) return;
    clearChildren(this._previewStrip);

    const hasAny = this._images.length || this._attachments.length || this._mountedDocs.length;
    this._previewStrip.hidden = !hasAny;
    if (!hasAny) return;

    for (let i = 0; i < this._images.length; i++) {
      const img = this._images[i];
      const thumb = img.previewUrl
        ? el('img', { className: 'if-mm-thumb', src: img.previewUrl, alt: img.name })
        : el('span', { className: 'if-mm-badge', text: 'IMG' });
      const chip = el('span', { className: 'if-mm-item if-mm-item--image' }, [
        thumb,
        el('span', { className: 'if-mm-name', text: img.name }),
        el('button', {
          type: 'button',
          className: 'if-mm-remove',
          'aria-label': tr('widget.multimodal.remove', null, 'Remove'),
          text: '×',
          onClick: () => {
            this._images.splice(i, 1);
            this._renderPreviews();
          },
        }),
      ]);
      this._previewStrip.appendChild(chip);
    }

    for (let i = 0; i < this._attachments.length; i++) {
      const att = this._attachments[i];
      const chip = el('span', { className: 'if-mm-item if-mm-item--file' }, [
        el('span', { className: 'if-mm-badge', text: tr('widget.multimodal.fileBadge', null, 'FILE') }),
        el('span', { className: 'if-mm-name', text: att.name }),
        el('button', {
          type: 'button',
          className: 'if-mm-remove',
          'aria-label': tr('widget.multimodal.remove', null, 'Remove'),
          text: '×',
          onClick: () => {
            this._attachments.splice(i, 1);
            this._renderPreviews();
          },
        }),
      ]);
      this._previewStrip.appendChild(chip);
    }

    for (let i = 0; i < this._mountedDocs.length; i++) {
      const doc = this._mountedDocs[i];
      const chip = el('span', { className: 'if-mm-item if-mm-item--vault' }, [
        el('span', { className: 'if-mm-badge', text: tr('widget.multimodal.vaultBadge', null, 'VAULT') }),
        el('span', { className: 'if-mm-name', text: doc.title }),
        el('button', {
          type: 'button',
          className: 'if-mm-remove',
          'aria-label': tr('widget.multimodal.remove', null, 'Remove'),
          text: '×',
          onClick: () => {
            this._mountedDocs.splice(i, 1);
            this._renderPreviews();
          },
        }),
      ]);
      this._previewStrip.appendChild(chip);
    }
  }

  _render() {
    clearChildren(this);

    const accept = this.getAttribute('accept-types') || '';
    const fileInputAttrs = {
      type: 'file',
      className: 'if-mm-file-input',
      multiple: true,
      hidden: true,
      onChange: (ev) => {
        this._handleFiles(ev.target && ev.target.files);
        if (ev.target) ev.target.value = '';
      },
    };
    if (accept) fileInputAttrs.accept = accept;
    this._fileInput = el('input', fileInputAttrs);

    this._previewStrip = el('div', { className: 'if-mm-previews', hidden: true });

    this._textarea = el('textarea', {
      className: 'if-mm-textarea',
      rows: '3',
      placeholder: this._resolvePlaceholder(),
      onInput: (ev) => {
        this._text = ev.target ? ev.target.value : '';
      },
      onPaste: (ev) => this._handlePaste(ev),
      onKeydown: (ev) => {
        if ((ev.ctrlKey || ev.metaKey) && ev.key === 'Enter') {
          ev.preventDefault();
          this.submit();
        }
      },
    });
    this._textarea.value = this._text;

    const toolbarChildren = [
      el('button', {
        type: 'button',
        className: 'if-mm-btn if-mm-btn--ghost',
        text: tr('widget.multimodal.attach', null, 'Attach File'),
        onClick: () => {
          if (this._fileInput && typeof this._fileInput.click === 'function') {
            this._fileInput.click();
          }
        },
      }),
    ];

    if (this.allowVaultMount) {
      toolbarChildren.push(el('button', {
        type: 'button',
        className: 'if-mm-btn if-mm-btn--ghost',
        text: tr('widget.multimodal.mountVault', null, 'Mount Vault Asset'),
        onClick: () => {
          this.dispatchEvent(new CustomEvent('if-request-vault-mount', {
            bubbles: true,
            composed: true,
            detail: { mountedDocIds: this._mountedDocs.map((d) => d.id) },
          }));
        },
      }));
    }

    toolbarChildren.push(el('span', {
      className: 'if-mm-hint',
      text: tr('widget.multimodal.dropHint', null, 'Supports screenshot paste & drag-drop'),
    }));

    toolbarChildren.push(el('button', {
      type: 'button',
      className: 'if-mm-btn if-mm-btn--primary',
      text: tr('widget.multimodal.submit', null, 'Send'),
      onClick: () => this.submit(),
    }));

    const box = el('div', {
      className: this._dragOver ? 'if-mm-box is-dragover' : 'if-mm-box',
      onDragover: (ev) => {
        ev.preventDefault();
        this._dragOver = true;
        box.className = 'if-mm-box is-dragover';
      },
      onDragleave: () => {
        this._dragOver = false;
        box.className = 'if-mm-box';
      },
      onDrop: (ev) => {
        ev.preventDefault();
        this._dragOver = false;
        box.className = 'if-mm-box';
        const files = ev && ev.dataTransfer && ev.dataTransfer.files;
        this._handleFiles(files);
      },
    }, [
      this._fileInput,
      this._previewStrip,
      this._textarea,
      el('div', { className: 'if-mm-toolbar' }, toolbarChildren),
    ]);

    this.appendChild(box);
    this._renderPreviews();
  }
}

if (typeof customElements !== 'undefined' && !customElements.get('if-multimodal-box')) {
  customElements.define('if-multimodal-box', IfMultimodalBox);
}
