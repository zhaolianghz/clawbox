<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { agents, addAgent, updateAgent, deleteAgent, editingItem, showModal } from '$lib/stores/config';
  import type { Agent } from '$lib/api/config';
  
  let newAgent: Agent = createEmptyAgent();
  
  function createEmptyAgent(): Agent {
    return {
      id: crypto.randomUUID(),
      name: '',
      systemPrompt: '',
      model: 'gpt-4',
      temperature: 0.7,
      maxTokens: 4096,
      enabled: true
    };
  }
  
  function handleAdd() {
    newAgent = createEmptyAgent();
    editingItem.set(null);
    showModal.set(true);
  }
  
  function handleEdit(agent: Agent) {
    newAgent = { ...agent };
    editingItem.set(agent);
    showModal.set(true);
  }
  
  function handleDelete(id: string) {
    if (confirm('Delete this agent?')) {
      deleteAgent(id);
    }
  }
  
  function handleSave() {
    if ($editingItem) {
      updateAgent($editingItem.id, newAgent);
    } else {
      addAgent(newAgent);
    }
    showModal.set(false);
    editingItem.set(null);
  }
  
  function handleCancel() {
    showModal.set(false);
    editingItem.set(null);
  }
</script>

<div class="agent-config">
  <div class="config-header">
    <h3>{$_('config.agents')}</h3>
    <button class="neon-button" onclick={handleAdd}>{$_('config.addAgent')}</button>
  </div>
  
  <div class="agent-list">
    {#each $agents as agent (agent.id)}
      <div class="agent-item glass-card">
        <div class="agent-info">
          <div class="agent-name">
            {agent.name}
            {#if !agent.enabled}
              <span class="disabled-badge">Disabled</span>
            {/if}
          </div>
          <div class="agent-details">
            <span class="detail">{agent.model}</span>
            <span class="detail">Temp: {agent.temperature}</span>
            <span class="detail">Tokens: {agent.maxTokens}</span>
          </div>
          <div class="agent-prompt">
            {agent.systemPrompt.slice(0, 100)}{agent.systemPrompt.length > 100 ? '...' : ''}
          </div>
        </div>
        <div class="agent-actions">
          <button class="neon-button small" onclick={() => handleEdit(agent)}>Edit</button>
          <button class="neon-button small danger" onclick={() => handleDelete(agent.id)}>Delete</button>
        </div>
      </div>
    {/each}
    
    {#if $agents.length === 0}
      <div class="empty-state">
        <p>No agents configured</p>
      </div>
    {/if}
  </div>
</div>

{#if $showModal}
  <div class="modal-overlay" onclick={handleCancel}>
    <div class="modal glass-card" onclick={(e) => e.stopPropagation()}>
      <h3>{$editingItem ? 'Edit Agent' : $_('config.addAgent')}</h3>
      
      <div class="form-group">
        <label>{$_('config.agentName')}</label>
        <input type="text" bind:value={newAgent.name} class="neon-input" />
      </div>
      
      <div class="form-group">
        <label>{$_('config.systemPrompt')}</label>
        <textarea bind:value={newAgent.systemPrompt} class="neon-input" rows="4"></textarea>
      </div>
      
      <div class="form-row">
        <div class="form-group half">
          <label>Model</label>
          <input type="text" bind:value={newAgent.model} class="neon-input" />
        </div>
        <div class="form-group half">
          <label>Temperature</label>
          <input type="number" bind:value={newAgent.temperature} class="neon-input" min="0" max="2" step="0.1" />
        </div>
      </div>
      
      <div class="form-group">
        <label>Max Tokens</label>
        <input type="number" bind:value={newAgent.maxTokens} class="neon-input" min="1" />
      </div>
      
      <div class="form-group">
        <label class="checkbox-label">
          <input type="checkbox" bind:checked={newAgent.enabled} />
          Enabled
        </label>
      </div>
      
      <div class="modal-actions">
        <button class="neon-button" onclick={handleCancel}>{$_('config.cancel')}</button>
        <button class="neon-button primary" onclick={handleSave}>{$_('config.save')}</button>
      </div>
    </div>
  </div>
{/if}

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
  
  .agent-list {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  
  .agent-item {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    padding: 1rem 1.5rem;
  }
  
  .agent-name {
    font-weight: 600;
    color: var(--text-primary);
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  
  .disabled-badge {
    font-size: 0.75rem;
    padding: 0.15rem 0.5rem;
    background: var(--bg-tertiary);
    border-radius: 0.25rem;
    color: var(--text-secondary);
    font-weight: normal;
  }
  
  .agent-details {
    display: flex;
    gap: 1rem;
    margin-top: 0.25rem;
    font-size: 0.85rem;
    color: var(--text-secondary);
  }
  
  .agent-prompt {
    margin-top: 0.5rem;
    font-size: 0.85rem;
    color: var(--text-secondary);
    font-style: italic;
  }
  
  .agent-actions {
    display: flex;
    gap: 0.5rem;
    flex-shrink: 0;
  }
  
  .empty-state {
    text-align: center;
    padding: 3rem;
    color: var(--text-secondary);
  }
  
  .modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }
  
  .modal {
    width: 100%;
    max-width: 500px;
    padding: 2rem;
    max-height: 90vh;
    overflow-y: auto;
  }
  
  .modal h3 {
    margin: 0 0 1.5rem 0;
    color: var(--text-primary);
  }
  
  .form-group {
    margin-bottom: 1rem;
  }
  
  .form-group label {
    display: block;
    margin-bottom: 0.5rem;
    color: var(--text-secondary);
    font-size: 0.9rem;
  }
  
  .form-row {
    display: flex;
    gap: 1rem;
  }
  
  .form-group.half {
    flex: 1;
  }
  
  .neon-input {
    width: 100%;
    padding: 0.75rem 1rem;
    background: var(--bg-tertiary);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 0.5rem;
    color: var(--text-primary);
    font-size: 0.9rem;
    resize: vertical;
  }
  
  .neon-input:focus {
    outline: none;
    border-color: var(--neon-cyan);
    box-shadow: var(--glow-cyan);
  }
  
  .checkbox-label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    cursor: pointer;
  }
  
  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 1.5rem;
  }
  
  .neon-button.primary {
    border-color: var(--neon-cyan);
    color: var(--neon-cyan);
  }
  
  .neon-button.primary:hover {
    background: rgba(0, 245, 255, 0.1);
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
