<script lang="ts">
  import Sidebar from './lib/components/Sidebar.svelte';
  import TopBar from './lib/components/TopBar.svelte';
  import StatusBar from './lib/components/StatusBar.svelte';
</script>

<div class="layout">
  <Sidebar />
  
  <main class="main-area">
    <TopBar />
    
    <div class="content">
      <slot>
        <div class="welcome glass-card">
          <h1>Welcome to ClawBox</h1>
          <p>Select a section from the sidebar to get started.</p>
        </div>
      </slot>
    </div>
    
    <StatusBar />
  </main>
</div>

<style>
  .layout {
    display: flex;
    height: 100vh;
    background: var(--bg-primary);
    overflow: hidden;
  }
  
  .main-area {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  
  .content {
    flex: 1;
    overflow: auto;
    padding: 1.5rem;
  }
  
  .welcome {
    max-width: 400px;
    margin: 2rem auto;
    padding: 2rem;
    text-align: center;
  }
  
  .welcome h1 {
    color: var(--neon-cyan);
    text-shadow: var(--glow-cyan);
    margin-bottom: 0.5rem;
  }
  
  .welcome p {
    color: var(--text-secondary);
  }
</style>
