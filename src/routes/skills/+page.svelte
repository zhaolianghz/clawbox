<script lang="ts">
  import { onMount } from 'svelte';
  import { _ } from 'svelte-i18n';
  import { get_skills, get_categories, install_skill, uninstall_skill, type Skill, type Category } from '$lib/api/skills';
  import CategorySidebar from '$lib/components/Skills/CategorySidebar.svelte';
  import SkillCard from '$lib/components/Skills/SkillCard.svelte';
  
  let categories = $state<Category[]>([]);
  let skills = $state<Skill[]>([]);
  let selectedCategory = $state('all');
  let searchQuery = $state('');
  let loading = $state(true);
  
  async function loadData() {
    loading = true;
    try {
      const [cats, sks] = await Promise.all([
        get_categories(),
        get_skills(selectedCategory),
      ]);
      categories = cats;
      skills = sks;
    } finally {
      loading = false;
    }
  }
  
  async function handleCategorySelect(id: string) {
    selectedCategory = id;
    skills = await get_skills(id);
  }
  
  async function handleInstall(id: string) {
    await install_skill(id);
    skills = skills.map(s => s.id === id ? { ...s, installed: true } : s);
  }
  
  async function handleUninstall(id: string) {
    await uninstall_skill(id);
    skills = skills.map(s => s.id === id ? { ...s, installed: false } : s);
  }
  
  let filteredSkills = $derived(
    searchQuery
      ? skills.filter(s => 
          s.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
          s.description.toLowerCase().includes(searchQuery.toLowerCase()) ||
          s.tags.some(t => t.toLowerCase().includes(searchQuery.toLowerCase()))
        )
      : skills
  );
  
  onMount(loadData);
</script>

<svelte:head>
  <title>{$_('skills.title')} - ClawBox</title>
</svelte:head>

<div class="skills-page">
  <div class="page-header">
    <h1>{$_('skills.title')}</h1>
    <div class="search-box">
      <span class="search-icon">🔍</span>
      <input
        type="text"
        class="search-input"
        placeholder={$_('skills.search')}
        bind:value={searchQuery}
      />
    </div>
  </div>
  
  <div class="skills-container">
    <CategorySidebar
      {categories}
      {selectedCategory}
      onSelect={handleCategorySelect}
    />
    
    <div class="skills-content">
      {#if loading}
        <div class="loading">
          <div class="spinner"></div>
          <span>{$_('skills.loading')}</span>
        </div>
      {:else if filteredSkills.length === 0}
        <div class="empty">
          <span class="empty-icon">🔍</span>
          <span>{$_('skills.noResults')}</span>
        </div>
      {:else}
        <div class="skills-grid">
          {#each filteredSkills as skill}
            <SkillCard
              {skill}
              onInstall={handleInstall}
              onUninstall={handleUninstall}
            />
          {/each}
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .skills-page {
    display: flex;
    flex-direction: column;
    height: calc(100vh - 60px - 32px - 3rem);
    margin: -1.5rem;
    background: var(--bg-primary);
  }
  
  .page-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 1rem 1.5rem;
    background: var(--bg-secondary);
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
  }
  
  .page-header h1 {
    font-size: 1.25rem;
    font-weight: 600;
    margin: 0;
  }
  
  .search-box {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    background: var(--bg-tertiary);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 0.5rem;
    padding: 0.5rem 1rem;
  }
  
  .search-icon {
    opacity: 0.5;
  }
  
  .search-input {
    background: transparent;
    border: none;
    color: var(--text-primary);
    font-size: 0.875rem;
    width: 200px;
    outline: none;
  }
  
  .search-input::placeholder {
    color: var(--text-muted);
  }
  
  .skills-container {
    flex: 1;
    display: flex;
    overflow: hidden;
  }
  
  .skills-content {
    flex: 1;
    overflow-y: auto;
    padding: 1.5rem;
  }
  
  .loading, .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1rem;
    height: 100%;
    color: var(--text-muted);
  }
  
  .spinner {
    width: 32px;
    height: 32px;
    border: 3px solid var(--bg-tertiary);
    border-top-color: var(--neon-cyan);
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }
  
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
  
  .empty-icon {
    font-size: 3rem;
    opacity: 0.5;
  }
  
  .skills-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 1rem;
  }
</style>
