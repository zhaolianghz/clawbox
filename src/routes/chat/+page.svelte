<script lang="ts">
  import { _ } from 'svelte-i18n';
  
  interface Message {
    id: string;
    role: 'user' | 'assistant';
    content: string;
    timestamp: Date;
  }
  
  interface Tab {
    id: string;
    name: string;
    agent: string;
    messages: Message[];
  }
  
  let tabs = $state<Tab[]>([
    { id: '1', name: 'Chat 1', agent: 'Claude 3', messages: [] }
  ]);
  let activeTab = $state('1');
  let inputValue = $state('');
  let isLoading = $state(false);
  
  function sendMessage() {
    if (!inputValue.trim() || isLoading) return;
    
    const tab = tabs.find(t => t.id === activeTab);
    if (!tab) return;
    
    tab.messages = [...tab.messages, {
      id: Date.now().toString(),
      role: 'user',
      content: inputValue,
      timestamp: new Date()
    }];
    
    inputValue = '';
    isLoading = true;
    
    setTimeout(() => {
      const loadingMsg: Message = {
        id: (Date.now() + 1).toString(),
        role: 'assistant',
        content: 'This is a mock response. In production, this would connect to the OpenClaw Gateway API.',
        timestamp: new Date()
      };
      tab.messages = [...tab.messages, loadingMsg];
      isLoading = false;
    }, 1000);
  }
  
  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
  }
</script>

<div class="chat-page">
  <div class="chat-tabs">
    {#each tabs as tab}
      <button
        class="tab"
        class:active={activeTab === tab.id}
        onclick={() => activeTab = tab.id}
      >
        <span class="tab-name">{tab.name}</span>
        <span class="tab-agent">{tab.agent}</span>
      </button>
    {/each}
    <button class="tab new-tab">➕</button>
  </div>
  
  <div class="chat-messages">
    {#each tabs.find(t => t.id === activeTab)?.messages || [] as msg}
      <div class="message" class:user={msg.role === 'user'} class:assistant={msg.role === 'assistant'}>
        <div class="message-header">
          <span class="message-role">{msg.role === 'user' ? 'You' : 'Assistant'}</span>
          <span class="message-time">{msg.timestamp.toLocaleTimeString()}</span>
        </div>
        <div class="message-content">{msg.content}</div>
      </div>
    {/each}
    
    {#if isLoading}
      <div class="message assistant loading">
        <div class="message-content">Thinking...</div>
      </div>
    {/if}
  </div>
  
  <div class="chat-input-area">
    <textarea
      class="chat-input"
      placeholder="Type your message..."
      bind:value={inputValue}
      onkeydown={handleKeydown}
      disabled={isLoading}
    ></textarea>
    <button class="send-btn" onclick={sendMessage} disabled={isLoading || !inputValue.trim()}>
      ➤
    </button>
  </div>
</div>

<style>
  .chat-page {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-primary);
    border-radius: 0.5rem;
    overflow: hidden;
  }
  
  .chat-tabs {
    display: flex;
    gap: 0.25rem;
    padding: 0.5rem;
    background: var(--bg-secondary);
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
    overflow-x: auto;
  }
  
  .tab {
    display: flex;
    flex-direction: column;
    gap: 0.125rem;
    padding: 0.5rem 1rem;
    background: transparent;
    border: none;
    border-radius: 0.375rem;
    cursor: pointer;
    color: var(--text-secondary);
    transition: all 0.2s ease;
    min-width: 100px;
  }
  
  .tab:hover {
    background: var(--bg-tertiary);
  }
  
  .tab.active {
    background: rgba(0, 245, 255, 0.1);
    color: var(--neon-cyan);
  }
  
  .tab-name {
    font-size: 0.875rem;
    font-weight: 500;
  }
  
  .tab-agent {
    font-size: 0.7rem;
    opacity: 0.7;
  }
  
  .new-tab {
    min-width: 40px;
    align-items: center;
    justify-content: center;
    font-size: 1rem;
  }
  
  .chat-messages {
    flex: 1;
    overflow-y: auto;
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  
  .message {
    max-width: 80%;
    padding: 0.75rem 1rem;
    border-radius: 0.75rem;
  }
  
  .message.user {
    align-self: flex-end;
    background: rgba(0, 245, 255, 0.15);
    border: 1px solid var(--neon-cyan);
  }
  
  .message.assistant {
    align-self: flex-start;
    background: var(--bg-secondary);
    border: 1px solid rgba(255, 255, 255, 0.1);
  }
  
  .message.loading .message-content {
    opacity: 0.5;
  }
  
  .message-header {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    margin-bottom: 0.25rem;
    font-size: 0.75rem;
  }
  
  .message-role {
    color: var(--neon-cyan);
    font-weight: 500;
  }
  
  .message-time {
    color: var(--text-muted);
  }
  
  .message-content {
    color: var(--text-primary);
    line-height: 1.5;
    white-space: pre-wrap;
  }
  
  .chat-input-area {
    display: flex;
    gap: 0.75rem;
    padding: 1rem;
    background: var(--bg-secondary);
    border-top: 1px solid rgba(255, 255, 255, 0.1);
  }
  
  .chat-input {
    flex: 1;
    background: var(--bg-tertiary);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 0.5rem;
    padding: 0.75rem 1rem;
    color: var(--text-primary);
    font-family: inherit;
    font-size: 0.9rem;
    resize: none;
    min-height: 44px;
    max-height: 120px;
  }
  
  .chat-input:focus {
    outline: none;
    border-color: var(--neon-cyan);
  }
  
  .send-btn {
    width: 44px;
    height: 44px;
    background: var(--neon-cyan);
    border: none;
    border-radius: 0.5rem;
    color: var(--bg-primary);
    font-size: 1.25rem;
    cursor: pointer;
    transition: all 0.2s ease;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  
  .send-btn:hover:not(:disabled) {
    box-shadow: var(--glow-cyan);
    transform: scale(1.05);
  }
  
  .send-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
