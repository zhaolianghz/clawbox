// 服务商目录
// 数据来源:Cherry Studio 内置服务商(apiHost 取自其打包配置) + cc-switch 的 Anthropic 兼容端点。
// 每条记录包含品牌色与图标标识,ProviderLogo 据此渲染 logo,无图标者回退为品牌色首字母。

export type ProviderCategory = 'intl' | 'cn' | 'aggregator' | 'local';

export interface ProviderCatalogEntry {
  id: string;
  name: string;
  /** OpenAI 兼容端点 */
  apiHost: string;
  website?: string;
  category: ProviderCategory;
  /** 品牌主色 */
  color: string;
  /** Anthropic 兼容端点(cc-switch 风格,部分国内厂商提供) */
  anthropicHost?: string;
  description?: string;
  defaultModel?: string;
}

export const PROVIDER_CATEGORIES: { id: ProviderCategory; label: string }[] = [
  { id: 'intl', label: '国际主流' },
  { id: 'cn', label: '国内主流' },
  { id: 'aggregator', label: '聚合中转' },
  { id: 'local', label: '本地' },
];

export const PROVIDER_CATALOG: ProviderCatalogEntry[] = [
  // ===== 国际主流 =====
  {
    id: 'openai', name: 'OpenAI', apiHost: 'https://api.openai.com/v1', website: 'https://openai.com',
    category: 'intl', color: '#10A37F', description: 'GPT 系列模型', defaultModel: 'gpt-4o',
  },
  {
    id: 'anthropic', name: 'Anthropic', apiHost: 'https://api.anthropic.com/v1', website: 'https://anthropic.com',
    category: 'intl', color: '#D97757', description: 'Claude 系列模型', defaultModel: 'claude-sonnet-4-5',
  },
  {
    id: 'gemini', name: 'Google Gemini', apiHost: 'https://generativelanguage.googleapis.com/v1beta', website: 'https://ai.google.dev',
    category: 'intl', color: '#4285F4', description: 'Gemini 系列模型', defaultModel: 'gemini-2.0-flash',
  },
  {
    id: 'mistral', name: 'Mistral AI', apiHost: 'https://api.mistral.ai/v1', website: 'https://mistral.ai',
    category: 'intl', color: '#FF7000', description: 'Mistral / Mixtral 模型', defaultModel: 'mistral-large-latest',
  },
  {
    id: 'grok', name: 'Grok (xAI)', apiHost: 'https://api.x.ai/v1', website: 'https://x.ai',
    category: 'intl', color: '#1D1D1F', description: 'xAI Grok 模型', defaultModel: 'grok-3',
  },
  {
    id: 'perplexity', name: 'Perplexity', apiHost: 'https://api.perplexity.ai', website: 'https://perplexity.ai',
    category: 'intl', color: '#20B8CD', description: '在线搜索增强模型', defaultModel: 'sonar',
  },
  {
    id: 'groq', name: 'Groq', apiHost: 'https://api.groq.com/openai/v1', website: 'https://groq.com',
    category: 'intl', color: '#F55036', description: '超低延迟推理', defaultModel: 'llama-3.3-70b-versatile',
  },
  {
    id: 'openrouter', name: 'OpenRouter', apiHost: 'https://openrouter.ai/api/v1', website: 'https://openrouter.ai',
    category: 'intl', color: '#6366F1', description: '聚合数百种模型',
  },
  {
    id: 'together', name: 'Together AI', apiHost: 'https://api.together.xyz/v1', website: 'https://together.ai',
    category: 'intl', color: '#0F6FFF', description: '开源模型托管',
  },
  {
    id: 'fireworks', name: 'Fireworks AI', apiHost: 'https://api.fireworks.ai/inference/v1', website: 'https://fireworks.ai',
    category: 'intl', color: '#EF5333', description: '高速开源推理',
  },
  {
    id: 'github', name: 'GitHub Models', apiHost: 'https://models.github.ai/inference', website: 'https://github.com',
    category: 'intl', color: '#E6EDF3', description: 'GitHub 模型市场',
  },
  {
    id: 'nvidia', name: 'NVIDIA', apiHost: 'https://integrate.api.nvidia.com/v1', website: 'https://build.nvidia.com',
    category: 'intl', color: '#76B900', description: 'NIM 推理微服务',
  },
  {
    id: 'cerebras', name: 'Cerebras', apiHost: 'https://api.cerebras.ai/v1', website: 'https://cerebras.ai',
    category: 'intl', color: '#FC4444', description: '极速推理',
  },
  {
    id: 'huggingface', name: 'Hugging Face', apiHost: 'https://router.huggingface.co/v1', website: 'https://huggingface.co',
    category: 'intl', color: '#FFD21E', description: '模型路由',
  },
  {
    id: 'cohere', name: 'Cohere', apiHost: 'https://api.cohere.ai/compatibility/v1', website: 'https://cohere.com',
    category: 'intl', color: '#39594D', description: 'Command 系列模型', defaultModel: 'command-r-plus',
  },

  // ===== 国内主流 =====
  {
    id: 'deepseek', name: 'DeepSeek', apiHost: 'https://api.deepseek.com', website: 'https://platform.deepseek.com',
    category: 'cn', color: '#4D6BFE', anthropicHost: 'https://api.deepseek.com/anthropic',
    description: '深度求索 V3 / R1', defaultModel: 'deepseek-chat',
  },
  {
    id: 'qwen', name: '通义千问', apiHost: 'https://dashscope.aliyuncs.com/compatible-mode/v1', website: 'https://tongyi.aliyun.com',
    category: 'cn', color: '#615CED', anthropicHost: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
    description: '阿里 Qwen 系列', defaultModel: 'qwen-max',
  },
  {
    id: 'zhipu', name: '智谱', apiHost: 'https://open.bigmodel.cn/api/paas/v4', website: 'https://open.bigmodel.cn',
    category: 'cn', color: '#1E6FFF', anthropicHost: 'https://open.bigmodel.cn/api/anthropic',
    description: 'GLM 系列模型', defaultModel: 'glm-4-plus',
  },
  {
    id: 'moonshot', name: '月之暗面', apiHost: 'https://api.moonshot.cn/v1', website: 'https://platform.moonshot.cn',
    category: 'cn', color: '#16181D', anthropicHost: 'https://api.moonshot.cn/anthropic',
    description: 'Kimi / Moonshot', defaultModel: 'moonshot-v1-8k',
  },
  {
    id: 'baichuan', name: '百川', apiHost: 'https://api.baichuan-ai.com/v1', website: 'https://platform.baichuan-ai.com',
    category: 'cn', color: '#F7941E', description: '百川大模型', defaultModel: 'Baichuan4',
  },
  {
    id: 'minimax', name: 'MiniMax', apiHost: 'https://api.minimaxi.com/v1', website: 'https://platform.minimaxi.com',
    category: 'cn', color: '#FF6B6B', anthropicHost: 'https://api.minimaxi.com/anthropic',
    description: 'MiniMax 系列', defaultModel: 'MiniMax-Text-01',
  },
  {
    id: 'hunyuan', name: '腾讯混元', apiHost: 'https://api.hunyuan.cloud.tencent.com/v1', website: 'https://cloud.tencent.com/product/hunyuan',
    category: 'cn', color: '#0053E0', description: '腾讯混元大模型', defaultModel: 'hunyuan-pro',
  },
  {
    id: 'yi', name: '零一万物', apiHost: 'https://api.lingyiwanwu.com/v1', website: 'https://platform.lingyiwanwu.com',
    category: 'cn', color: '#00B8A9', description: 'Yi 系列模型', defaultModel: 'yi-large',
  },
  {
    id: 'doubao', name: '字节豆包', apiHost: 'https://ark.cn-beijing.volces.com/api/v3', website: 'https://www.volcengine.com/product/doubao',
    category: 'cn', color: '#1664FF', description: '火山方舟豆包', defaultModel: 'doubao-pro',
  },
  {
    id: 'stepfun', name: '阶跃星辰', apiHost: 'https://api.stepfun.com/v1', website: 'https://platform.stepfun.com',
    category: 'cn', color: '#7B61FF', description: 'Step 系列模型', defaultModel: 'step-2-16k',
  },
  {
    id: 'qianfan', name: '百度千帆', apiHost: 'https://qianfan.baidubce.com/v2', website: 'https://qianfan.cloud.baidu.com',
    category: 'cn', color: '#2932E1', description: '文心一言 / 千帆', defaultModel: 'ernie-4.0-8k-latest',
  },
  {
    id: 'longcat', name: '美团 LongCat', apiHost: 'https://api.longcat.chat/openai/v1', website: 'https://longcat.chat',
    category: 'cn', color: '#FFC300', description: '美团大模型', defaultModel: 'longcat-flash',
  },
  {
    id: 'ctyun', name: '天翼云', apiHost: 'https://wishub-x1.ctyun.cn/v1', website: 'https://ctyun.cn',
    category: 'cn', color: '#E60012', description: '天翼云星辰', defaultModel: '星辰大模型',
  },

  // ===== 聚合 / 中转 =====
  {
    id: 'siliconflow', name: '硅基流动', apiHost: 'https://api.siliconflow.cn/v1', website: 'https://siliconflow.cn',
    category: 'aggregator', color: '#FF8A00', description: '聚合海量开源模型',
  },
  {
    id: '302ai', name: '302.AI', apiHost: 'https://api.302.ai/v1', website: 'https://302.ai',
    category: 'aggregator', color: '#2D6CDF', description: '多模型聚合中转',
  },
  {
    id: 'aihubmix', name: 'AiHubMix', apiHost: 'https://aihubmix.com/v1', website: 'https://aihubmix.com',
    category: 'aggregator', color: '#6C5CE7', description: '聚合 API 中转',
  },
  {
    id: 'dmxapi', name: 'DMXAPI', apiHost: 'https://www.dmxapi.cn/v1', website: 'https://dmxapi.cn',
    category: 'aggregator', color: '#F5A623', description: '聚合中转 + 绘图',
  },
  {
    id: 'modelscope', name: 'ModelScope', apiHost: 'https://api-inference.modelscope.cn/v1', website: 'https://modelscope.cn',
    category: 'aggregator', color: '#6B4FBB', description: '魔搭社区推理',
  },
  {
    id: 'tokenflux', name: 'TokenFlux', apiHost: 'https://api.tokenflux.ai/openai/v1', website: 'https://tokenflux.ai',
    category: 'aggregator', color: '#00C2A8', description: '聚合中转',
  },
  {
    id: 'cherryai', name: 'Cherry AI', apiHost: 'https://api.cherry-ai.com/v1', website: 'https://cherry-ai.com',
    category: 'aggregator', color: '#FF4D4F', description: 'Cherry 官方中转',
  },
  {
    id: 'infini', name: 'Infini AI', apiHost: 'https://cloud.infini-ai.com/maas', website: 'https://infini-ai.com',
    category: 'aggregator', color: '#00B4D8', description: 'Infini 异构云',
  },

  // ===== 本地 =====
  {
    id: 'ollama', name: 'Ollama', apiHost: 'http://localhost:11434/v1', website: 'https://ollama.com',
    category: 'local', color: '#E8E8E8', description: '本地模型运行时',
  },
  {
    id: 'lmstudio', name: 'LM Studio', apiHost: 'http://localhost:1234/v1', website: 'https://lmstudio.ai',
    category: 'local', color: '#5046E5', description: '本地模型桌面端',
  },
];
