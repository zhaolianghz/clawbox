<script lang="ts">
  import { _ } from 'svelte-i18n';
  import { onMount } from 'svelte';
  import { 
    systemCheck, 
    installProgress, 
    needsInstall, 
    installComplete,
    type SystemCheck,
    type InstallProgress
  } from '../../stores/install';
  import { invoke } from '@tauri-apps/api/core';

  let agreedToTerms = $state(false);
  let isChecking = $state(true);
  let checkData: SystemCheck = { nodejs: false, openclaw: false, network: 'unknown' };
  let progressData: InstallProgress = { step: 'checking', progress: 0, log: [] };

  async function checkSystem() {
    isChecking = true;
    try {
      const result = await invoke<{
        nodejs: boolean;
        openclaw: boolean;
        network: string;
      }>('check_system');
      
      const newCheck: SystemCheck = {
        nodejs: result.nodejs,
        openclaw: result.openclaw,
        network: result.network as 'cn' | 'global' | 'unknown'
      };
      
      systemCheck.set(newCheck);
      checkData = newCheck;
      
      const needs = !result.nodejs || !result.openclaw;
      needsInstall.set(needs);
      
      if (needs) {
        const newProgress: InstallProgress = { ...progressData, step: 'terms' };
        installProgress.set(newProgress);
        progressData = newProgress;
      } else {
        const newProgress: InstallProgress = { ...progressData, step: 'complete' };
        installProgress.set(newProgress);
        progressData = newProgress;
        installComplete.set(true);
      }
    } catch {
      const newCheck: SystemCheck = { ...checkData, network: 'unknown' };
      systemCheck.set(newCheck);
      checkData = newCheck;
      needsInstall.set(true);
      const newProgress: InstallProgress = { ...progressData, step: 'terms' };
      installProgress.set(newProgress);
      progressData = newProgress;
    }
    isChecking = false;
  }

  async function startInstall() {
    let newProgress: InstallProgress = { 
      step: 'installing', 
      progress: 0,
      log: []
    };
    installProgress.set(newProgress);
    progressData = newProgress;

    try {      
      if (!checkData.nodejs) {
        addLog('Installing Node.js...');
        await installWithProgress('install_nodejs', 50);
      }
      
      if (!checkData.openclaw) {
        addLog('Installing OpenClaw CLI...');
        await installWithProgress('install_openclaw', checkData.nodejs ? 100 : 50);
      }
      
      newProgress = { 
        ...progressData,
        step: 'complete',
        progress: 100 
      };
      installProgress.set(newProgress);
      progressData = newProgress;
      installComplete.set(true);
    } catch (e) {
      const errorMsg = e instanceof Error ? e.message : String(e);
      addLog(`Error: ${errorMsg}`);
      newProgress = { ...progressData, error: errorMsg };
      installProgress.set(newProgress);
      progressData = newProgress;
    }
  }

  async function installWithProgress(cmd: string, targetProgress: number) {
    try {
      await invoke(cmd);
      const newProgress: InstallProgress = { ...progressData, progress: targetProgress };
      installProgress.set(newProgress);
      progressData = newProgress;
      addLog('Installation step completed.');
    } catch (e) {
      throw e;
    }
  }

  function addLog(message: string) {
    const newLog = [...progressData.log, `[${new Date().toLocaleTimeString()}] ${message}`];
    const newProgress: InstallProgress = {
      ...progressData,
      log: newLog
    };
    installProgress.set(newProgress);
    progressData = newProgress;
  }

  function skipInstall() {
    const newProgress: InstallProgress = { ...progressData, step: 'complete' };
    installProgress.set(newProgress);
    progressData = newProgress;
    installComplete.set(true);
  }

  onMount(() => {
    checkSystem();
  });
</script>

<div class="install-wizard">
  <div class="wizard-container glass-card">
    <div class="wizard-header">
      <h1>ClawBox Setup</h1>
    </div>

    {#if isChecking}
      <div class="checking-screen">
        <div class="spinner"></div>
        <p>{$_('install.checking')}</p>
      </div>
    {:else if progressData.step === 'terms'}
      <div class="terms-screen">
        <h2>{$_('install.needsInstall')}</h2>
        
        <ul class="requirements-list">
          {#if !checkData.nodejs}
            <li class="missing">
              <span class="status-icon">⚠️</span>
              {$_('install.nodejs')}
            </li>
          {:else}
            <li class="installed">
              <span class="status-icon">✓</span>
              {$_('install.nodejs')}
            </li>
          {/if}
          
          {#if !checkData.openclaw}
            <li class="missing">
              <span class="status-icon">⚠️</span>
              {$_('install.openclaw')}
            </li>
          {:else}
            <li class="installed">
              <span class="status-icon">✓</span>
              {$_('install.openclaw')}
            </li>
          {/if}
        </ul>

        <div class="network-status">
          <span>Network: </span>
          <span class="network-badge" class:cn={checkData.network === 'cn'} class:global={checkData.network === 'global'}>
            {checkData.network === 'cn' ? 'China' : checkData.network === 'global' ? 'Global' : 'Unknown'}
          </span>
        </div>

        <label class="terms-checkbox">
          <input type="checkbox" bind:checked={agreedToTerms} />
          <span>{$_('install.terms')}</span>
        </label>

        <div class="button-group">
          <button 
            class="neon-button primary" 
            onclick={startInstall}
            disabled={!agreedToTerms}
          >
            {$_('install.start')}
          </button>
          <button class="neon-button secondary" onclick={skipInstall}>
            {$_('install.skip')}
          </button>
        </div>
      </div>
    {:else if progressData.step === 'installing'}
      <div class="installing-screen">
        <h2>{$_('install.installing')}</h2>
        
        <div class="progress-container">
          <div class="progress-bar">
            <div 
              class="progress-fill" 
              style="width: {progressData.progress}%"
            ></div>
          </div>
          <span class="progress-text">{progressData.progress}%</span>
        </div>

        <div class="log-container">
          {#each progressData.log as log}
            <div class="log-line">{log}</div>
          {/each}
        </div>

        {#if progressData.error}
          <div class="error-message">
            Error: {progressData.error}
          </div>
        {/if}
      </div>
    {:else if progressData.step === 'complete'}
      <div class="complete-screen">
        <div class="success-icon">✓</div>
        <h2>{$_('install.complete')}</h2>
        <p>All components are ready!</p>
        <button class="neon-button primary" onclick={() => installComplete.set(true)}>
          {$_('install.startUsing')}
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .install-wizard {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-primary);
    z-index: 1000;
  }

  .wizard-container {
    width: 100%;
    max-width: 500px;
    padding: 2rem;
    margin: 1rem;
  }

  .wizard-header {
    text-align: center;
    margin-bottom: 2rem;
  }

  .wizard-header h1 {
    color: var(--neon-cyan);
    text-shadow: var(--glow-cyan);
    font-size: 1.75rem;
    margin: 0;
  }

  .checking-screen {
    text-align: center;
    padding: 2rem;
  }

  .spinner {
    width: 48px;
    height: 48px;
    border: 3px solid var(--bg-tertiary);
    border-top-color: var(--neon-cyan);
    border-radius: 50%;
    margin: 0 auto 1rem;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .terms-screen h2 {
    color: var(--text-primary);
    font-size: 1.25rem;
    margin-bottom: 1.5rem;
    text-align: center;
  }

  .requirements-list {
    list-style: none;
    padding: 0;
    margin: 0 0 1rem;
  }

  .requirements-list li {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.75rem 1rem;
    background: var(--bg-tertiary);
    border-radius: 0.5rem;
    margin-bottom: 0.5rem;
  }

  .requirements-list li.missing {
    border: 1px solid var(--neon-orange);
  }

  .requirements-list li.installed {
    border: 1px solid var(--neon-green);
  }

  .status-icon {
    font-size: 1.1rem;
  }

  .network-status {
    text-align: center;
    margin-bottom: 1.5rem;
    color: var(--text-secondary);
  }

  .network-badge {
    padding: 0.25rem 0.75rem;
    border-radius: 1rem;
    font-size: 0.85rem;
  }

  .network-badge.cn {
    background: rgba(255, 136, 0, 0.2);
    color: var(--neon-orange);
  }

  .network-badge.global {
    background: rgba(0, 255, 136, 0.2);
    color: var(--neon-green);
  }

  .terms-checkbox {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 1rem;
    background: var(--bg-tertiary);
    border-radius: 0.5rem;
    margin-bottom: 1.5rem;
    cursor: pointer;
  }

  .terms-checkbox input {
    width: 1.25rem;
    height: 1.25rem;
    accent-color: var(--neon-cyan);
  }

  .button-group {
    display: flex;
    gap: 1rem;
  }

  .button-group button {
    flex: 1;
  }

  .neon-button {
    background: var(--bg-tertiary);
    border: 1px solid var(--neon-cyan);
    color: var(--neon-cyan);
    padding: 0.75rem 1.5rem;
    border-radius: 0.5rem;
    font-size: 1rem;
    cursor: pointer;
    transition: all 0.3s ease;
  }

  .neon-button:hover:not(:disabled) {
    box-shadow: var(--glow-cyan);
    background: rgba(0, 245, 255, 0.1);
  }

  .neon-button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .neon-button.secondary {
    border-color: var(--text-muted);
    color: var(--text-secondary);
  }

  .neon-button.secondary:hover {
    border-color: var(--text-secondary);
    box-shadow: none;
    background: var(--bg-tertiary);
  }

  .installing-screen h2 {
    text-align: center;
    color: var(--neon-cyan);
    margin-bottom: 1.5rem;
  }

  .progress-container {
    display: flex;
    align-items: center;
    gap: 1rem;
    margin-bottom: 1.5rem;
  }

  .progress-bar {
    flex: 1;
    height: 8px;
    background: var(--bg-tertiary);
    border-radius: 4px;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: linear-gradient(90deg, var(--neon-cyan), var(--neon-purple));
    transition: width 0.3s ease;
  }

  .progress-text {
    color: var(--neon-cyan);
    font-size: 0.9rem;
    min-width: 40px;
  }

  .log-container {
    background: var(--bg-primary);
    border-radius: 0.5rem;
    padding: 1rem;
    max-height: 150px;
    overflow-y: auto;
    font-family: monospace;
    font-size: 0.85rem;
  }

  .log-line {
    color: var(--text-secondary);
    margin-bottom: 0.25rem;
  }

  .error-message {
    margin-top: 1rem;
    padding: 0.75rem;
    background: rgba(255, 0, 110, 0.1);
    border: 1px solid var(--neon-pink);
    border-radius: 0.5rem;
    color: var(--neon-pink);
  }

  .complete-screen {
    text-align: center;
    padding: 1rem 0;
  }

  .success-icon {
    width: 64px;
    height: 64px;
    background: rgba(0, 255, 136, 0.2);
    border: 2px solid var(--neon-green);
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    margin: 0 auto 1rem;
    font-size: 2rem;
    color: var(--neon-green);
    box-shadow: var(--glow-green);
  }

  .complete-screen h2 {
    color: var(--neon-green);
    margin-bottom: 0.5rem;
  }

  .complete-screen p {
    color: var(--text-secondary);
    margin-bottom: 1.5rem;
  }
</style>
