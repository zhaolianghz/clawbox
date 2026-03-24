<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { skills, toggleSkill } from '$lib/stores/config';
</script>

<div class="skill-config">
  <div class="config-header">
    <h3>{$_('config.skills')}</h3>
  </div>
  
  <div class="skill-list">
    {#each $skills as skill (skill.id)}
      <div class="skill-item glass-card">
        <div class="skill-info">
          <div class="skill-header">
            <span class="skill-name">{skill.name}</span>
            <span class="skill-version">v{skill.version}</span>
          </div>
          <div class="skill-description">{skill.description}</div>
        </div>
        <div class="skill-actions">
          <button 
            class="neon-button small" 
            class:primary={!skill.enabled}
            class:danger={skill.enabled}
            onclick={() => toggleSkill(skill.id)}
          >
            {skill.enabled ? $_('config.disableSkill') : $_('config.enableSkill')}
          </button>
        </div>
      </div>
    {/each}
    
    {#if $skills.length === 0}
      <div class="empty-state">
        <p>No skills installed</p>
      </div>
    {/if}
  </div>
</div>

<style>
  .config-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1.5rem;
  }
  
  .config-header h3 {
    margin: 0;
    color: var(--text-primary);
  }
  
  .skill-list {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  
  .skill-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1rem 1.5rem;
  }
  
  .skill-header {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }
  
  .skill-name {
    font-weight: 600;
    color: var(--text-primary);
  }
  
  .skill-version {
    font-size: 0.75rem;
    padding: 0.15rem 0.5rem;
    background: var(--bg-tertiary);
    border-radius: 0.25rem;
    color: var(--neon-cyan);
  }
  
  .skill-description {
    margin-top: 0.25rem;
    font-size: 0.85rem;
    color: var(--text-secondary);
  }
  
  .skill-actions {
    display: flex;
    gap: 0.5rem;
  }
  
  .empty-state {
    text-align: center;
    padding: 3rem;
    color: var(--text-secondary);
  }
  
  .neon-button.primary {
    border-color: var(--neon-green);
    color: var(--neon-green);
  }
  
  .neon-button.primary:hover {
    background: rgba(0, 255, 136, 0.1);
    box-shadow: var(--glow-green);
  }
  
  .neon-button.danger {
    border-color: var(--neon-pink);
    color: var(--neon-pink);
  }
  
  .neon-button.danger:hover {
    background: rgba(255, 0, 110, 0.1);
  }
  
  .neon-button.small {
    padding: 0.4rem 0.75rem;
    font-size: 0.8rem;
  }
</style>
