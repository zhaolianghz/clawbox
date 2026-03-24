<script lang="ts">
  import { _ } from 'svelte-i18n';
  import type { Skill } from '$lib/api/skills';
  
  interface Props {
    skill: Skill;
    onInstall: (id: string) => void;
    onUninstall: (id: string) => void;
  }
  
  let { skill, onInstall, onUninstall }: Props = $props();
  let isInstalling = $state(false);
  
  async function handleAction() {
    isInstalling = true;
    try {
      if (skill.installed) {
        await onUninstall(skill.id);
      } else {
        await onInstall(skill.id);
      }
    } finally {
      isInstalling = false;
    }
  }
  
  function formatDownloads(num: number): string {
    if (num >= 1000) return `${(num / 1000).toFixed(1)}k`;
    return String(num);
  }
</script>

<div class="skill-card glass-card">
  <div class="skill-header">
    <span class="skill-icon">{skill.icon}</span>
    <div class="skill-info">
      <h3 class="skill-name">{skill.name}</h3>
      <span class="skill-author">by {skill.author}</span>
    </div>
    <span class="skill-version">v{skill.version}</span>
  </div>
  
  <p class="skill-description">{skill.description}</p>
  
  <div class="skill-tags">
    {#each skill.tags as tag}
      <span class="tag">{tag}</span>
    {/each}
  </div>
  
  <div class="skill-footer">
    <div class="skill-stats">
      <span class="stat">
        <span class="stat-icon">⬇️</span>
        {formatDownloads(skill.downloads)}
      </span>
      <span class="stat">
        <span class="stat-icon">⭐</span>
        {skill.rating.toFixed(1)}
      </span>
    </div>
    
    <button
      class="neon-button action-btn"
      class:installed={skill.installed}
      onclick={handleAction}
      disabled={isInstalling}
    >
      {#if isInstalling}
        <span class="spinner-small"></span>
      {:else if skill.installed}
        {$_('skills.uninstall')}
      {:else}
        {$_('skills.install')}
      {/if}
    </button>
  </div>
</div>

<style>
  .skill-card {
    padding: 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    transition: transform 0.2s ease, box-shadow 0.2s ease;
  }
  
  .skill-card:hover {
    transform: translateY(-2px);
    box-shadow: 0 8px 32px rgba(0, 245, 255, 0.1);
  }
  
  .skill-header {
    display: flex;
    align-items: flex-start;
    gap: 0.75rem;
  }
  
  .skill-icon {
    font-size: 2rem;
    line-height: 1;
  }
  
  .skill-info {
    flex: 1;
    min-width: 0;
  }
  
  .skill-name {
    font-size: 1rem;
    font-weight: 600;
    margin: 0;
    color: var(--text-primary);
  }
  
  .skill-author {
    font-size: 0.75rem;
    color: var(--text-muted);
  }
  
  .skill-version {
    font-size: 0.75rem;
    color: var(--text-muted);
    background: var(--bg-tertiary);
    padding: 0.25rem 0.5rem;
    border-radius: 0.25rem;
  }
  
  .skill-description {
    font-size: 0.875rem;
    color: var(--text-secondary);
    line-height: 1.5;
    margin: 0;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  
  .skill-tags {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  
  .tag {
    font-size: 0.75rem;
    color: var(--neon-cyan);
    background: rgba(0, 245, 255, 0.1);
    padding: 0.25rem 0.5rem;
    border-radius: 0.25rem;
  }
  
  .skill-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: auto;
  }
  
  .skill-stats {
    display: flex;
    gap: 1rem;
  }
  
  .stat {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    font-size: 0.875rem;
    color: var(--text-muted);
  }
  
  .stat-icon {
    font-size: 0.875rem;
  }
  
  .action-btn {
    padding: 0.5rem 1rem;
    font-size: 0.875rem;
  }
  
  .action-btn.installed {
    border-color: var(--neon-pink);
    color: var(--neon-pink);
  }
  
  .action-btn.installed:hover {
    box-shadow: 0 0 20px rgba(255, 0, 110, 0.5);
  }
  
  .spinner-small {
    width: 14px;
    height: 14px;
    border: 2px solid var(--bg-tertiary);
    border-top-color: currentColor;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    display: inline-block;
  }
  
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
