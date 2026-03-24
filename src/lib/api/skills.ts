export interface Skill {
  id: string;
  name: string;
  description: string;
  category: string;
  author: string;
  version: string;
  downloads: number;
  rating: number;
  installed: boolean;
  icon: string;
  tags: string[];
}

export interface Category {
  id: string;
  name: string;
  icon: string;
  count: number;
}

export async function get_skills(category?: string): Promise<Skill[]> {
  return get_mock_skills(category);
}

export async function get_categories(): Promise<Category[]> {
  return get_mock_categories();
}

export async function install_skill(id: string): Promise<void> {
  await new Promise(resolve => setTimeout(resolve, 1000));
  console.log(`Installing skill: ${id}`);
}

export async function uninstall_skill(id: string): Promise<void> {
  await new Promise(resolve => setTimeout(resolve, 500));
  console.log(`Uninstalling skill: ${id}`);
}

function get_mock_categories(): Category[] {
  return [
    { id: 'all', name: 'All', icon: '📦', count: 24 },
    { id: 'productivity', name: 'Productivity', icon: '⚡', count: 8 },
    { id: 'coding', name: 'Coding', icon: '💻', count: 6 },
    { id: 'analysis', name: 'Analysis', icon: '📊', count: 5 },
    { id: 'creative', name: 'Creative', icon: '🎨', count: 3 },
    { id: 'automation', name: 'Automation', icon: '🤖', count: 2 },
  ];
}

function get_mock_skills(category?: string): Skill[] {
  const allSkills: Skill[] = [
    {
      id: 'code-review',
      name: 'Code Review',
      description: 'Automatically review code for best practices, security issues, and performance improvements.',
      category: 'coding',
      author: 'OpenClaw',
      version: '1.2.0',
      downloads: 15234,
      rating: 4.8,
      installed: true,
      icon: '🔍',
      tags: ['code', 'review', 'quality'],
    },
    {
      id: 'doc-generator',
      name: 'Documentation Generator',
      description: 'Generate comprehensive documentation from code comments and structure.',
      category: 'productivity',
      author: 'OpenClaw',
      version: '2.0.1',
      downloads: 8921,
      rating: 4.6,
      installed: false,
      icon: '📝',
      tags: ['docs', 'markdown', 'api'],
    },
    {
      id: 'data-analyzer',
      name: 'Data Analyzer',
      description: 'Analyze datasets and generate insights with visualizations.',
      category: 'analysis',
      author: 'DataBot',
      version: '1.5.0',
      downloads: 6543,
      rating: 4.5,
      installed: false,
      icon: '📈',
      tags: ['data', 'analytics', 'charts'],
    },
    {
      id: 'test-gen',
      name: 'Test Generator',
      description: 'Automatically generate unit tests and integration tests for your code.',
      category: 'coding',
      author: 'TestMaster',
      version: '1.0.3',
      downloads: 4321,
      rating: 4.3,
      installed: false,
      icon: '🧪',
      tags: ['testing', 'unit-test', 'quality'],
    },
    {
      id: 'image-gen',
      name: 'Image Generator',
      description: 'Generate images from text descriptions using AI models.',
      category: 'creative',
      author: 'ArtBot',
      version: '3.1.0',
      downloads: 12876,
      rating: 4.9,
      installed: true,
      icon: '🖼️',
      tags: ['image', 'ai', 'creative'],
    },
    {
      id: 'workflow',
      name: 'Workflow Automation',
      description: 'Create and manage automated workflows between different services.',
      category: 'automation',
      author: 'FlowBot',
      version: '2.2.0',
      downloads: 7654,
      rating: 4.7,
      installed: false,
      icon: '⚡',
      tags: ['automation', 'workflow', 'integration'],
    },
    {
      id: 'translator',
      name: 'Multi-Language Translator',
      description: 'Translate text between 100+ languages with context awareness.',
      category: 'productivity',
      author: 'LinguaBot',
      version: '1.8.0',
      downloads: 19823,
      rating: 4.6,
      installed: false,
      icon: '🌐',
      tags: ['translation', 'language', 'i18n'],
    },
    {
      id: 'summarizer',
      name: 'Text Summarizer',
      description: 'Summarize long documents and articles into concise summaries.',
      category: 'productivity',
      author: 'OpenClaw',
      version: '1.4.0',
      downloads: 11234,
      rating: 4.5,
      installed: false,
      icon: '📋',
      tags: ['summary', 'nlp', 'documents'],
    },
  ];
  
  if (category && category !== 'all') {
    return allSkills.filter(s => s.category === category);
  }
  return allSkills;
}
