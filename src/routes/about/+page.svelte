<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { onMount } from 'svelte';
  import { feedback_submit, feedback_list, type Feedback } from '$lib/api/feedback';

  let appVersion = $state('0.1.0');
  let checking = $state(false);
  let updateAvailable = $state(false);
  let updateMessage = $state('');

  let feedbackMessage = $state('');
  let feedbackContact = $state('');
  let feedbackCategory = $state('bug');
  let feedbackSubmitting = $state(false);
  let feedbackSubmitted = $state(false);
  let feedbackError = $state('');
  let feedbackHistory = $state<Feedback[]>([]);

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
      const entry = await feedback_submit(feedbackCategory, feedbackMessage, feedbackContact || undefined);
      feedbackHistory = [entry, ...feedbackHistory];
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
    feedbackHistory = await feedback_list();
  });
</script>


<div class="about-page">
  <div class="about-header">
    <div class="logo-large">🎮</div>
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
      <h2>{$_('about.links')}</h2>
      <div class="link-list">
        <a href="https://github.com/openclaw/clawbox" class="link-item" target="_blank" rel="noopener">
          <span class="link-icon">📦</span>
          <span class="link-text">GitHub Repository</span>
          <span class="link-arrow">→</span>
        </a>
        <a href="https://docs.openclaw.ai" class="link-item" target="_blank" rel="noopener">
          <span class="link-icon">📚</span>
          <span class="link-text">{$_('about.documentation')}</span>
          <span class="link-arrow">→</span>
        </a>
        <a href="https://github.com/openclaw/clawbox/issues" class="link-item" target="_blank" rel="noopener">
          <span class="link-icon">🐛</span>
          <span class="link-text">{$_('about.reportIssue')}</span>
          <span class="link-arrow">→</span>
        </a>
        <a href="https://discord.gg/openclaw" class="link-item" target="_blank" rel="noopener">
          <span class="link-icon">💬</span>
          <span class="link-text">{$_('about.discord')}</span>
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

      {#if feedbackHistory.length > 0}
        <div class="feedback-history">
          <h3>{$_('about.feedbackHistory')}</h3>
          {#each feedbackHistory as fb (fb.id)}
            <div class="feedback-entry">
              <span class="feedback-badge" data-category={fb.category}>{fb.category}</span>
              <span class="feedback-entry-text">{fb.message}</span>
              <span class="feedback-entry-date">{new Date(fb.created_at * 1000).toLocaleDateString()}</span>
            </div>
          {/each}
        </div>
      {/if}
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
    text-align: center;
    margin-bottom: 2rem;
  }
  
  .logo-large {
    font-size: 4rem;
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

  .feedback-history {
    margin-top: 1.5rem;
    padding-top: 1rem;
    border-top: 1px solid rgba(255, 255, 255, 0.1);
  }

  .feedback-history h3 {
    margin: 0 0 0.75rem;
    font-size: 0.95rem;
    color: var(--text-secondary);
  }

  .feedback-entry {
    display: flex;
    align-items: baseline;
    gap: 0.75rem;
    padding: 0.5rem 0;
    font-size: 0.85rem;
  }

  .feedback-badge {
    padding: 0.1rem 0.5rem;
    border-radius: 999px;
    font-size: 0.7rem;
    font-weight: 600;
    background: rgba(0, 245, 255, 0.15);
    color: var(--neon-cyan);
    flex-shrink: 0;
  }

  .feedback-badge[data-category='bug'] {
    background: rgba(255, 0, 110, 0.15);
    color: var(--neon-pink);
  }

  .feedback-badge[data-category='feature'] {
    background: rgba(0, 255, 136, 0.15);
    color: var(--neon-green);
  }

  .feedback-entry-text {
    flex: 1;
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .feedback-entry-date {
    color: var(--text-muted);
    flex-shrink: 0;
  }
</style>
