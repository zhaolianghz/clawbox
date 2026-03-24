<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { channels, addChannel, updateChannel, deleteChannel, editingItem, showModal } from '$lib/stores/config';
  import type { Channel } from '$lib/api/config';
  
  let newChannel: Channel = createEmptyChannel();
  
  function createEmptyChannel(): Channel {
    return {
      id: crypto.randomUUID(),
      name: '',
      endpoint: '',
      priority: 1,
      loadBalance: 'round-robin',
      enabled: true
    };
  }
  
  function handleAdd() {
    newChannel = createEmptyChannel();
    editingItem.set(null);
    showModal.set(true);
  }
  
  function handleEdit(channel: Channel) {
    newChannel = { ...channel };
    editingItem.set(channel);
    showModal.set(true);
  }
  
  function handleDelete(id: string) {
    if (confirm('Delete this channel?')) {
      deleteChannel(id);
    }
  }
  
  function handleSave() {
    if ($editingItem) {
      updateChannel($editingItem.id, newChannel);
    } else {
      addChannel(newChannel);
    }
    showModal.set(false);
    editingItem.set(null);
  }
  
  function handleCancel() {
    showModal.set(false);
    editingItem.set(null);
  }
</script>

<div class="channel-config">
  <div class="config-header">
    <h3>{$_('config.channels')}</h3>
    <button class="neon-button" onclick={handleAdd}>{$_('config.addChannel')}</button>
  </div>
  
  <div class="channel-list">
    {#each $channels as channel (channel.id)}
      <div class="channel-item glass-card">
        <div class="channel-info">
          <div class="channel-name">
            {channel.name}
            {#if !channel.enabled}
              <span class="disabled-badge">Disabled</span>
            {/if}
          </div>
          <div class="channel-details">
            <span class="detail">{channel.endpoint}</span>
            <span class="detail">Priority: {channel.priority}</span>
            <span class="detail">{channel.loadBalance}</span>
          </div>
        </div>
        <div class="channel-actions">
          <button class="neon-button small" onclick={() => handleEdit(channel)}>Edit</button>
          <button class="neon-button small danger" onclick={() => handleDelete(channel.id)}>Delete</button>
        </div>
      </div>
    {/each}
    
    {#if $channels.length === 0}
      <div class="empty-state">
        <p>No channels configured</p>
      </div>
    {/if}
  </div>
</div>

{#if $showModal}
  <div class="modal-overlay" onclick={handleCancel}>
    <div class="modal glass-card" onclick={(e) => e.stopPropagation()}>
      <h3>{$editingItem ? 'Edit Channel' : $_('config.addChannel')}</h3>
      
      <div class="form-group">
        <label>{$_('config.channelName')}</label>
        <input type="text" bind:value={newChannel.name} class="neon-input" />
      </div>
      
      <div class="form-group">
        <label>{$_('config.endpoint')}</label>
        <input type="text" bind:value={newChannel.endpoint} class="neon-input" />
      </div>
      
      <div class="form-group">
        <label>Priority</label>
        <input type="number" bind:value={newChannel.priority} class="neon-input" min="1" />
      </div>
      
      <div class="form-group">
        <label>Load Balance</label>
        <select bind:value={newChannel.loadBalance} class="neon-input">
          <option value="round-robin">Round Robin</option>
          <option value="weighted">Weighted</option>
          <option value="least-connections">Least Connections</option>
        </select>
      </div>
      
      <div class="form-group">
        <label class="checkbox-label">
          <input type="checkbox" bind:checked={newChannel.enabled} />
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
  
  .channel-list {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  
  .channel-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1rem 1.5rem;
  }
  
  .channel-name {
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
  
  .channel-details {
    display: flex;
    gap: 1rem;
    margin-top: 0.25rem;
    font-size: 0.85rem;
    color: var(--text-secondary);
  }
  
  .channel-actions {
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
