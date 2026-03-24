<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { providers, addProvider, updateProvider, deleteProvider, editingItem, showModal } from '$lib/stores/config';
  import type { ModelProvider } from '$lib/api/config';
  
  let newProvider: ModelProvider = createEmptyProvider();
  
  function createEmptyProvider(): ModelProvider {
    return {
      id: crypto.randomUUID(),
      name: '',
      apiKey: '',
      baseUrl: '',
      defaultModel: '',
      enabled: true
    };
  }
  
  function handleAdd() {
    newProvider = createEmptyProvider();
    editingItem.set(null);
    showModal.set(true);
  }
  
  function handleEdit(provider: ModelProvider) {
    newProvider = { ...provider };
    editingItem.set(provider);
    showModal.set(true);
  }
  
  function handleDelete(id: string) {
    if (confirm($_('config.deleteProvider') + '?')) {
      deleteProvider(id);
    }
  }
  
  function handleSave() {
    if ($editingItem) {
      updateProvider($editingItem.id, newProvider);
    } else {
      addProvider(newProvider);
    }
    showModal.set(false);
    editingItem.set(null);
  }
  
  function handleCancel() {
    showModal.set(false);
    editingItem.set(null);
  }
</script>

<div class="model-config">
  <div class="config-header">
    <h3>{$_('config.models')}</h3>
    <button class="neon-button" onclick={handleAdd}>{$_('config.addProvider')}</button>
  </div>
  
  <div class="provider-list">
    {#each $providers as provider (provider.id)}
      <div class="provider-item glass-card">
        <div class="provider-info">
          <div class="provider-name">
            {provider.name}
            {#if !provider.enabled}
              <span class="disabled-badge">Disabled</span>
            {/if}
          </div>
          <div class="provider-details">
            <span class="detail">{provider.defaultModel}</span>
            <span class="detail">{provider.baseUrl}</span>
          </div>
        </div>
        <div class="provider-actions">
          <button class="neon-button small" onclick={() => handleEdit(provider)}>{$_('config.editProvider')}</button>
          <button class="neon-button small danger" onclick={() => handleDelete(provider.id)}>{$_('config.deleteProvider')}</button>
        </div>
      </div>
    {/each}
    
    {#if $providers.length === 0}
      <div class="empty-state">
        <p>No providers configured</p>
      </div>
    {/if}
  </div>
</div>

{#if $showModal}
  <div class="modal-overlay" onclick={handleCancel}>
    <div class="modal glass-card" onclick={(e) => e.stopPropagation()}>
      <h3>{$editingItem ? $_('config.editProvider') : $_('config.addProvider')}</h3>
      
      <div class="form-group">
        <label>{$_('config.providerName')}</label>
        <input type="text" bind:value={newProvider.name} class="neon-input" />
      </div>
      
      <div class="form-group">
        <label>{$_('config.apiKey')}</label>
        <input type="password" bind:value={newProvider.apiKey} class="neon-input" />
      </div>
      
      <div class="form-group">
        <label>{$_('config.baseUrl')}</label>
        <input type="text" bind:value={newProvider.baseUrl} class="neon-input" />
      </div>
      
      <div class="form-group">
        <label>{$_('config.defaultModel')}</label>
        <input type="text" bind:value={newProvider.defaultModel} class="neon-input" />
      </div>
      
      <div class="form-group">
        <label class="checkbox-label">
          <input type="checkbox" bind:checked={newProvider.enabled} />
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
  
  .provider-list {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  
  .provider-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1rem 1.5rem;
  }
  
  .provider-name {
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
  
  .provider-details {
    display: flex;
    gap: 1rem;
    margin-top: 0.25rem;
    font-size: 0.85rem;
    color: var(--text-secondary);
  }
  
  .provider-actions {
    display: flex;
    gap: 0.5rem;
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
  
  .neon-input {
    width: 100%;
    padding: 0.75rem 1rem;
    background: var(--bg-tertiary);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 0.5rem;
    color: var(--text-primary);
    font-size: 0.9rem;
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
