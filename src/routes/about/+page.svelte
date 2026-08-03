<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import { getVersion } from '@tauri-apps/api/app';
  import logoUrl from '../../assets/logo.png';
  import { open } from '@tauri-apps/plugin-shell';
  import { themeChoice, setTheme, type ThemeChoice } from '$lib/theme';

  const REPO_URL = 'https://github.com/zhaolianghz/clawbox';
  // Feedback category -> existing GitHub label. Missing labels are ignored by
  // GitHub (the issue is still created), so this stays best-effort.
  const ISSUE_LABELS: Record<string, string> = {
    bug: 'bug',
    feature: 'enhancement',
    other: 'question',
  };

  let appVersion = $state('');
  let checking = $state(false);
  let updateAvailable = $state(false);
  let updateMessage = $state('');

  let feedbackMessage = $state('');
  let feedbackContact = $state('');
  let feedbackCategory = $state('bug');
  let feedbackSubmitting = $state(false);
  let feedbackSubmitted = $state(false);
  let feedbackError = $state('');

  async function checkForUpdates() {
    checking = true;
    updateMessage = '';
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const result = await invoke<{
        has_update: boolean;
        current_version: string;
        latest_version: string | null;
        message: string;
      }>('check_update');

      updateAvailable = result.has_update;
      updateMessage = result.message;
      appVersion = result.current_version;
    } catch {
      updateMessage = 'Failed to check for updates';
      updateAvailable = false;
    } finally {
      checking = false;
    }
  }

  async function submitFeedback() {
    if (!feedbackMessage.trim() || feedbackSubmitting) return;
    feedbackSubmitting = true;
    feedbackError = '';
    try {
      const msg = feedbackMessage.trim();
      const title = `[${feedbackCategory}] ${msg.split('\n')[0].slice(0, 60)}`;
      const bodyLines = [
        msg,
        '',
        '---',
        `- ClawBox: v${appVersion}`,
        `- Platform: ${navigator.userAgent}`,
      ];
      if (feedbackContact.trim()) bodyLines.push(`- Contact: ${feedbackContact.trim()}`);

      const params = new URLSearchParams({
        title,
        body: bodyLines.join('\n'),
      });
      const label = ISSUE_LABELS[feedbackCategory];
      if (label) params.set('labels', label);

      // Opens in the user's default browser; they submit under their own GitHub
      // account. Nothing is stored locally or sent anywhere by ClawBox itself.
      await open(`${REPO_URL}/issues/new?${params.toString()}`);

      feedbackMessage = '';
      feedbackContact = '';
      feedbackSubmitted = true;
      setTimeout(() => (feedbackSubmitted = false), 3000);
    } catch (e) {
      feedbackError = e instanceof Error ? e.message : String(e);
    } finally {
      feedbackSubmitting = false;
    }
  }

  onMount(async () => {
    try {
      appVersion = await getVersion();
    } catch {
      appVersion = 'unknown';
    }
  });
</script>


<div class="about-page">
  <div class="about-header">
    <img class="logo-large" src={logoUrl} alt="ClawBox" />
    <h1>ClawBox</h1>
    <p class="tagline">{$_('about.tagline')}</p>
  </div>
  
  <div class="about-content">
    <div class="info-section glass-card">
      <h2>{$_('about.versions')}</h2>
      <div class="version-list">
        <div class="version-item">
          <span class="label">ClawBox</span>
          <span class="value">v{appVersion}</span>
        </div>
        <div class="version-item">
          <span class="label">Tauri</span>
          <span class="value">v2.0</span>
        </div>
        <div class="version-item">
          <span class="label">Svelte</span>
          <span class="value">v5.0</span>
        </div>
      </div>
      
      <div class="update-section">
        <button class="neon-button" onclick={checkForUpdates} disabled={checking}>
          {#if checking}
            <span class="spinner-small"></span>
            {$_('about.checking')}
          {:else}
            🔄 {$_('about.checkUpdates')}
          {/if}
        </button>
        {#if updateMessage}
          <div class="update-message" class:has-update={updateAvailable}>
            {#if updateAvailable}🎉 {/if}{updateMessage}
          </div>
        {/if}
      </div>
    </div>
    
    <div class="info-section glass-card">
      <h2>{$_('about.theme')}</h2>
      <div class="theme-options">
        {#each [
          { id: 'cyberpunk', label: 'Cyberpunk', desc: 'about.themeCyberpunk' },
          { id: 'minimal', label: 'Minimal', desc: 'about.themeMinimal' },
          { id: 'liquid-glass', label: 'Liquid Glass', desc: 'about.themeLiquid' },
        ] as opt (opt.id)}
          <button
            class="theme-option"
            class:active={$themeChoice === opt.id}
            onclick={() => setTheme(opt.id as ThemeChoice)}
          >
            <div class="theme-preview theme-{opt.id}">
              <div class="preview-bar"></div>
              <div class="preview-content">
                <div class="preview-line"></div>
                <div class="preview-line short"></div>
              </div>
            </div>
            <div class="theme-info">
              <span class="theme-name">{opt.label}</span>
              <span class="theme-desc">{$_(opt.desc)}</span>
            </div>
            {#if $themeChoice === opt.id}
              <span class="theme-check">✓</span>
            {/if}
          </button>
        {/each}
      </div>
    </div>
    
    <div class="info-section glass-card">
      <h2>{$_('about.links')}</h2>
      <div class="link-list">
        <a href="https://github.com/zhaolianghz/clawbox" class="link-item" target="_blank" rel="noopener">
          <span class="link-icon">📦</span>
          <span class="link-text">GitHub Repository</span>
          <span class="link-arrow">→</span>
        </a>
        <a href="https://github.com/zhaolianghz/clawbox#readme" class="link-item" target="_blank" rel="noopener">
          <span class="link-icon">📚</span>
          <span class="link-text">{$_('about.documentation')}</span>
          <span class="link-arrow">→</span>
        </a>
        <a href="https://github.com/zhaolianghz/clawbox/issues" class="link-item" target="_blank" rel="noopener">
          <span class="link-icon">🐛</span>
          <span class="link-text">{$_('about.reportIssue')}</span>
          <span class="link-arrow">→</span>
        </a>
      </div>
    </div>
    
    <div class="info-section glass-card">
      <h2>{$_('about.feedback')}</h2>
      <div class="feedback-form">
        <div class="feedback-row">
          <label class="feedback-label" for="fb-category">{$_('about.feedbackCategory')}</label>
          <select id="fb-category" bind:value={feedbackCategory} class="feedback-select">
            <option value="bug">{$_('about.feedbackCategoryBug')}</option>
            <option value="feature">{$_('about.feedbackCategoryFeature')}</option>
            <option value="other">{$_('about.feedbackCategoryOther')}</option>
          </select>
        </div>
        <textarea
          class="feedback-textarea"
          bind:value={feedbackMessage}
          placeholder={$_('about.feedbackPlaceholder')}
          rows="4"
        ></textarea>
        <input
          class="feedback-input"
          type="text"
          bind:value={feedbackContact}
          placeholder={$_('about.feedbackContact')}
        />
        {#if feedbackError}
          <div class="feedback-error">{feedbackError}</div>
        {/if}
        {#if feedbackSubmitted}
          <div class="feedback-thanks">✓ {$_('about.feedbackThanks')}</div>
        {/if}
        <button
          class="neon-button"
          onclick={submitFeedback}
          disabled={feedbackSubmitting || !feedbackMessage.trim()}
        >
          {#if feedbackSubmitting}
            <span class="spinner-small"></span>
            {$_('about.feedbackSubmitting')}
          {:else}
            {$_('about.feedbackSubmit')}
          {/if}
        </button>
      </div>
    </div>
  </div>

  <footer class="about-footer">
    <p>©️ 2026 ClawBox. {$_('about.allRightsReserved')}</p>
  </footer>
</div>

<style>
  .about-page {
    max-width: 800px;
    margin: 0 auto;
    padding: 2rem;
  }
  
  .about-header {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    margin-bottom: 2rem;
  }
  
  .logo-large {
    width: 96px;
    height: 96px;
    border-radius: 22px;
    margin-bottom: 1rem;
  }
  
  .about-header h1 {
    font-size: 2rem;
    margin: 0 0 0.5rem;
    color: var(--neon-cyan);
    text-shadow: var(--glow-cyan);
  }
  
  .tagline {
    color: var(--text-secondary);
    margin: 0;
  }
  
  .about-content {
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
  }
  
  .info-section {
    padding: 1.5rem;
  }
  
  .info-section h2 {
    font-size: 1rem;
    font-weight: 600;
    margin: 0 0 1rem;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  
  .version-list {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  
  .version-item {
    display: flex;
    justify-content: space-between;
    padding: 0.5rem 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
  }
  
  .version-item:last-child {
    border-bottom: none;
  }
  
  .version-item .label {
    color: var(--text-secondary);
  }
  
  .version-item .value {
    color: var(--neon-cyan);
    font-family: monospace;
  }
  
  .update-section {
    margin-top: 1.5rem;
    padding-top: 1rem;
    border-top: 1px solid rgba(255, 255, 255, 0.1);
  }

  .update-message {
    margin-top: 0.75rem;
    padding: 0.75rem;
    background: var(--bg-tertiary);
    border-radius: 0.5rem;
    color: var(--text-secondary);
    font-size: 0.9rem;
  }

  .update-message.has-update {
    background: rgba(0, 255, 136, 0.1);
    color: var(--neon-green);
  }
  
  .link-list {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  
  .link-item {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.75rem 1rem;
    background: var(--bg-tertiary);
    border-radius: 0.5rem;
    color: var(--text-primary);
    text-decoration: none;
    transition: all 0.2s ease;
  }
  
  .link-item:hover {
    background: rgba(0, 245, 255, 0.1);
    color: var(--neon-cyan);
  }
  
  .link-icon {
    font-size: 1.25rem;
  }
  
  .link-text {
    flex: 1;
  }
  
  .link-arrow {
    opacity: 0.5;
  }

  .about-footer {
    text-align: center;
    margin-top: 2rem;
    padding-top: 1rem;
    border-top: 1px solid rgba(255, 255, 255, 0.1);
    color: var(--text-muted);
    font-size: 0.875rem;
  }
  
  .spinner-small {
    width: 14px;
    height: 14px;
    border: 2px solid var(--bg-tertiary);
    border-top-color: currentColor;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    display: inline-block;
    margin-right: 0.5rem;
  }
  
  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .feedback-form {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .feedback-row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .feedback-label {
    color: var(--text-secondary);
    font-size: 0.9rem;
  }

  .feedback-select,
  .feedback-input,
  .feedback-textarea {
    background: var(--bg-tertiary);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 0.5rem;
    color: var(--text-primary);
    padding: 0.6rem 0.75rem;
    font-size: 0.9rem;
    font-family: inherit;
  }

  .feedback-select:focus,
  .feedback-input:focus,
  .feedback-textarea:focus {
    outline: none;
    border-color: var(--neon-cyan);
  }

  .feedback-textarea {
    resize: vertical;
  }

  .feedback-error {
    color: var(--neon-pink);
    font-size: 0.85rem;
  }

  .feedback-thanks {
    color: var(--neon-green);
    font-size: 0.85rem;
  }

  /* Theme selector */
  .theme-options {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0.75rem;
  }

  .theme-option {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
    padding: 0.75rem;
    background: var(--bg-tertiary);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 0.75rem;
    cursor: pointer;
    transition: all 0.2s ease;
    position: relative;
  }

  .theme-option:hover {
    border-color: var(--neon-cyan);
    background: rgba(0, 245, 255, 0.05);
  }

  .theme-option.active {
    border-color: var(--neon-cyan);
    box-shadow: 0 0 0 1px var(--neon-cyan), var(--glow-cyan);
  }

  .theme-preview {
    width: 100%;
    aspect-ratio: 16 / 10;
    border-radius: 0.5rem;
    overflow: hidden;
    position: relative;
  }

  .theme-preview .preview-bar {
    height: 20%;
    width: 100%;
  }

  .theme-preview .preview-content {
    padding: 0.4rem;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .theme-preview .preview-line {
    height: 4px;
    border-radius: 2px;
    width: 80%;
  }

  .theme-preview .preview-line.short {
    width: 50%;
  }

  /* Cyberpunk preview */
  .theme-cyberpunk {
    background: #0a0a0f;
  }
  .theme-cyberpunk .preview-bar {
    background: linear-gradient(90deg, #ff0055, #00f5ff);
  }
  .theme-cyberpunk .preview-line {
    background: rgba(0, 245, 255, 0.4);
  }

  /* Minimal preview */
  .theme-minimal {
    background: #f6f7f9;
  }
  .theme-minimal .preview-bar {
    background: #1a1a1a;
  }
  .theme-minimal .preview-line {
    background: rgba(0, 0, 0, 0.2);
  }

  /* Liquid Glass preview */
  .theme-liquid-glass {
    background: linear-gradient(135deg, #e0e7ff 0%, #f3e8ff 100%);
  }
  .theme-liquid-glass .preview-bar {
    background: linear-gradient(90deg, #6366f1, #a855f7);
  }
  .theme-liquid-glass .preview-line {
    background: rgba(99, 102, 241, 0.3);
  }

  .theme-info {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.15rem;
  }

  .theme-name {
    font-size: 0.8rem;
    font-weight: 600;
    color: var(--text-primary);
  }

  .theme-desc {
    font-size: 0.7rem;
    color: var(--text-muted);
  }

  .theme-check {
    position: absolute;
    top: 0.4rem;
    right: 0.4rem;
    width: 18px;
    height: 18px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--neon-cyan);
    color: #000;
    border-radius: 50%;
    font-size: 0.7rem;
    font-weight: bold;
  }
</style>
