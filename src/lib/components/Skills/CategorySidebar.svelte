<script lang="ts">
  import { _ } from 'svelte-i18n';
  import type { Category } from '$lib/api/skills';
  
  interface Props {
    categories: Category[];
    selectedCategory: string;
    onSelect: (id: string) => void;
  }
  
  let { categories, selectedCategory, onSelect }: Props = $props();
</script>

<div class="category-sidebar">
  <div class="sidebar-header">
    <span class="title">{$_('skills.categories')}</span>
  </div>
  
  <div class="category-list">
    {#each categories as category}
      <button
        class="category-item"
        class:selected={selectedCategory === category.id}
        onclick={() => onSelect(category.id)}
      >
        <span class="category-icon">{category.icon}</span>
        <span class="category-name">{category.name}</span>
        <span class="category-count">{category.count}</span>
      </button>
    {/each}
  </div>
</div>

<style>
  .category-sidebar {
    width: 220px;
    background: var(--bg-secondary);
    border-right: 1px solid rgba(255, 255, 255, 0.1);
    display: flex;
    flex-direction: column;
  }
  
  .sidebar-header {
    padding: 1rem;
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
  }
  
  .title {
    font-weight: 600;
    color: var(--text-primary);
  }
  
  .category-list {
    flex: 1;
    overflow-y: auto;
    padding: 0.5rem;
  }
  
  .category-item {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.75rem 1rem;
    background: transparent;
    border: none;
    border-radius: 0.5rem;
    cursor: pointer;
    transition: all 0.2s ease;
    width: 100%;
    text-align: left;
    margin-bottom: 0.25rem;
    color: var(--text-secondary);
  }
  
  .category-item:hover {
    background: var(--bg-tertiary);
    color: var(--text-primary);
  }
  
  .category-item.selected {
    background: rgba(0, 245, 255, 0.1);
    color: var(--neon-cyan);
  }
  
  .category-icon {
    font-size: 1.1rem;
  }
  
  .category-name {
    flex: 1;
  }
  
  .category-count {
    background: var(--bg-tertiary);
    padding: 0.125rem 0.5rem;
    border-radius: 0.25rem;
    font-size: 0.75rem;
    color: var(--text-muted);
  }
  
  .category-item.selected .category-count {
    background: rgba(0, 245, 255, 0.2);
    color: var(--neon-cyan);
  }
</style>
