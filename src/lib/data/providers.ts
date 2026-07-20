// 服务商目录
// 数据来源:Cherry Studio 内置服务商(apiHost 取自其打包配置) + cc-switch 的 Anthropic 兼容端点;
// 部分条目参考 OmniRoute(MIT)——端点取自其 provider registry 源码(baseUrl 字段),非凭记忆。
// 图标来自 lobe-icons(@lobehub/icons-static-svg,MIT),见 providerIcons.ts。
// 每条记录包含品牌色与图标标识,ProviderLogo 据此渲染 logo,无图标者回退为品牌色首字母。
// name/description/分类 label 为内嵌双语(Localized);国际厂商名两语相同用单串。

import type { Localized } from './localized';

export type ProviderCategory = 'intl' | 'cn' | 'aggregator' | 'local';

export interface ProviderCatalogEntry {
  id: string;
  name: string | Localized;
  /** OpenAI 兼容端点 */
  apiHost: string;
  website?: string;
  category: ProviderCategory;
  /** 品牌主色 */
  color: string;
  /** Anthropic 兼容端点(cc-switch 风格,部分国内厂商提供) */
  anthropicHost?: string;
  description?: Localized;
  defaultModel?: string;
  /** 免费额度亮点(有免费层/试用金的服务商);卡片显示绿色小徽章 */
  freeNote?: Localized;
}

export const PROVIDER_CATEGORIES: { id: ProviderCategory; label: Localized }[] = [
  { id: 'intl', label: { en: 'International', zh: '国际主流' } },
  { id: 'cn', label: { en: 'China', zh: '国内主流' } },
  { id: 'aggregator', label: { en: 'Aggregators', zh: '聚合中转' } },
  { id: 'local', label: { en: 'Local', zh: '本地' } },
];

export const PROVIDER_CATALOG: ProviderCatalogEntry[] = [
  // ===== 国际主流 =====
  {
    id: 'openai', name: 'OpenAI', apiHost: 'https://api.openai.com/v1', website: 'https://openai.com',
    category: 'intl', color: '#10A37F',
    description: { en: 'GPT model family', zh: 'GPT 系列模型' }, defaultModel: 'gpt-4o',
  },
  {
    id: 'anthropic', name: 'Anthropic', apiHost: 'https://api.anthropic.com/v1', website: 'https://anthropic.com',
    category: 'intl', color: '#D97757',
    description: { en: 'Claude model family', zh: 'Claude 系列模型' }, defaultModel: 'claude-sonnet-4-5',
  },
  {
    id: 'gemini', name: 'Google Gemini', apiHost: 'https://generativelanguage.googleapis.com/v1beta', website: 'https://ai.google.dev',
    category: 'intl', color: '#4285F4',
    description: { en: 'Gemini model family', zh: 'Gemini 系列模型' }, defaultModel: 'gemini-2.0-flash',
  },
  {
    id: 'mistral', name: 'Mistral AI', apiHost: 'https://api.mistral.ai/v1', website: 'https://mistral.ai',
    category: 'intl', color: '#FF7000',
    description: { en: 'Mistral / Mixtral models', zh: 'Mistral / Mixtral 模型' }, defaultModel: 'mistral-large-latest',
  },
  {
    id: 'grok', name: 'Grok (xAI)', apiHost: 'https://api.x.ai/v1', website: 'https://x.ai',
    category: 'intl', color: '#1D1D1F',
    description: { en: 'xAI Grok models', zh: 'xAI Grok 模型' }, defaultModel: 'grok-3',
  },
  {
    id: 'perplexity', name: 'Perplexity', apiHost: 'https://api.perplexity.ai', website: 'https://perplexity.ai',
    category: 'intl', color: '#20B8CD',
    description: { en: 'Search-augmented models', zh: '在线搜索增强模型' }, defaultModel: 'sonar',
  },
  {
    id: 'groq', name: 'Groq', apiHost: 'https://api.groq.com/openai/v1', website: 'https://groq.com',
    category: 'intl', color: '#F55036',
    description: { en: 'Ultra-low-latency inference', zh: '超低延迟推理' }, defaultModel: 'llama-3.3-70b-versatile',
  },
  {
    id: 'openrouter', name: 'OpenRouter', apiHost: 'https://openrouter.ai/api/v1', website: 'https://openrouter.ai',
    category: 'intl', color: '#6366F1',
    description: { en: 'Hundreds of models, one API', zh: '聚合数百种模型' },
  },
  {
    id: 'together', name: 'Together AI', apiHost: 'https://api.together.xyz/v1', website: 'https://together.ai',
    category: 'intl', color: '#0F6FFF',
    description: { en: 'Hosted open-source models', zh: '开源模型托管' },
  },
  {
    id: 'fireworks', name: 'Fireworks AI', apiHost: 'https://api.fireworks.ai/inference/v1', website: 'https://fireworks.ai',
    category: 'intl', color: '#EF5333',
    description: { en: 'Fast open-source inference', zh: '高速开源推理' },
  },
  {
    id: 'github', name: 'GitHub Models', apiHost: 'https://models.github.ai/inference', website: 'https://github.com',
    category: 'intl', color: '#E6EDF3',
    description: { en: 'GitHub model marketplace', zh: 'GitHub 模型市场' },
  },
  {
    id: 'nvidia', name: 'NVIDIA', apiHost: 'https://integrate.api.nvidia.com/v1', website: 'https://build.nvidia.com',
    category: 'intl', color: '#76B900',
    description: { en: 'NIM inference microservices', zh: 'NIM 推理微服务' },
  },
  {
    id: 'cerebras', name: 'Cerebras', apiHost: 'https://api.cerebras.ai/v1', website: 'https://cerebras.ai',
    category: 'intl', color: '#FC4444',
    description: { en: 'Blazing-fast inference', zh: '极速推理' },
  },
  {
    id: 'huggingface', name: 'Hugging Face', apiHost: 'https://router.huggingface.co/v1', website: 'https://huggingface.co',
    category: 'intl', color: '#FFD21E',
    description: { en: 'Model routing', zh: '模型路由' },
  },
  {
    id: 'cohere', name: 'Cohere', apiHost: 'https://api.cohere.ai/compatibility/v1', website: 'https://cohere.com',
    category: 'intl', color: '#39594D',
    description: { en: 'Command model family', zh: 'Command 系列模型' }, defaultModel: 'command-r-plus',
  },

  {
    id: 'meta-llama', name: 'Meta Llama API', apiHost: 'https://api.llama.com/compat/v1', website: 'https://llama.developer.meta.com',
    category: 'intl', color: '#0F766E',
    description: { en: 'Official Llama API (OpenAI-compatible)', zh: 'Meta 官方 Llama API(OpenAI 兼容)' },
  },
  {
    id: 'reka', name: 'Reka', apiHost: 'https://api.reka.ai/v1', website: 'https://docs.reka.ai/chat/overview',
    category: 'intl', color: '#111827',
    description: { en: 'Reka multimodal models', zh: 'Reka 多模态模型' },
    freeNote: { en: '$10/month recurring free API credits', zh: '每月循环 $10 免费 API 额度' },
  },
  {
    id: 'ai21', name: 'AI21 Labs', apiHost: 'https://api.ai21.com/studio/v1', website: 'https://www.ai21.com',
    category: 'intl', color: '#0284C7',
    description: { en: 'Jamba model family', zh: 'Jamba 系列模型' },
    freeNote: { en: '$10 trial credits on signup (3-month validity), no credit card', zh: '注册送 $10 试用金(3 个月有效),免绑卡' },
  },
  {
    id: 'venice', name: 'Venice.ai', apiHost: 'https://api.venice.ai/api/v1', website: 'https://venice.ai',
    category: 'intl', color: '#0EA5E9',
    description: { en: 'Privacy-focused inference', zh: '隐私优先的推理服务' },
  },
  {
    id: 'codestral', name: 'Codestral', apiHost: 'https://codestral.mistral.ai/v1', website: 'https://mistral.ai',
    category: 'intl', color: '#FF7000',
    description: { en: 'Mistral dedicated coding endpoint', zh: 'Mistral 代码专用端点' },
  },
  {
    id: 'upstage', name: 'Upstage', apiHost: 'https://api.upstage.ai/v1', website: 'https://www.upstage.ai',
    category: 'intl', color: '#0F766E',
    description: { en: 'Solar model family', zh: 'Solar 系列模型' },
  },
  {
    id: 'nous-research', name: 'Nous Research', apiHost: 'https://inference-api.nousresearch.com/v1', website: 'https://portal.nousresearch.com',
    category: 'intl', color: '#2563EB',
    description: { en: 'Hermes open models', zh: 'Hermes 系列开源模型' },
    freeNote: { en: 'Free tier: 50 RPM / 500K TPM — no credit card', zh: '免费层:50 RPM / 50 万 TPM,免绑卡' },
  },
  {
    id: 'morph', name: 'Morph', apiHost: 'https://api.morphllm.com/v1', website: 'https://morphllm.com',
    category: 'intl', color: '#2563EB',
    description: { en: 'Fast code-apply models', zh: '高速代码合并(apply)模型' },
    freeNote: { en: 'Free tier: 250K credits/month', zh: '免费层:每月 25 万额度' },
  },
  {
    id: 'blackbox', name: 'Blackbox AI', apiHost: 'https://api.blackbox.ai/v1', website: 'https://blackbox.ai',
    category: 'intl', color: '#1A1A2E',
    description: { en: 'Coding-focused aggregation API', zh: '面向编程的聚合 API' },
    freeNote: { en: 'Free tier available — no credit card required', zh: '提供免费层,免绑卡' },
  },
  {
    id: 'deepinfra', name: 'DeepInfra', apiHost: 'https://api.deepinfra.com/v1/openai', website: 'https://deepinfra.com',
    category: 'intl', color: '#2563EB',
    description: { en: 'Serverless open-model inference', zh: '开源模型无服务器推理' },
    freeNote: { en: 'Free signup credits for API testing', zh: '注册送免费测试额度' },
  },
  {
    id: 'hyperbolic', name: 'Hyperbolic', apiHost: 'https://api.hyperbolic.xyz/v1', website: 'https://hyperbolic.xyz',
    category: 'intl', color: '#00D4FF',
    description: { en: 'Serverless GPU inference', zh: '无服务器 GPU 推理' },
    freeNote: { en: '$1-5 trial credits on signup', zh: '注册送 $1-5 试用金' },
  },
  {
    id: 'sambanova', name: 'SambaNova', apiHost: 'https://api.sambanova.ai/v1', website: 'https://sambanova.ai',
    category: 'intl', color: '#DC2626',
    description: { en: 'RDU-accelerated fast inference', zh: 'RDU 加速的高速推理' },
    freeNote: { en: '$5 free credits on signup (30-day validity), no credit card', zh: '注册送 $5(30 天有效),免绑卡' },
  },
  {
    id: 'lambda-ai', name: 'Lambda AI', apiHost: 'https://api.lambda.ai/v1', website: 'https://lambda.ai',
    category: 'intl', color: '#7C3AED',
    description: { en: 'Lambda cloud inference API', zh: 'Lambda 云推理 API' },
  },
  {
    id: 'nebius', name: 'Nebius AI', apiHost: 'https://api.tokenfactory.nebius.com/v1', website: 'https://nebius.com',
    category: 'intl', color: '#6C5CE7',
    description: { en: 'Token Factory open-model inference', zh: 'Token Factory 开源模型推理' },
    freeNote: { en: '~$1 trial credits on signup', zh: '注册送约 $1 试用金' },
  },
  {
    id: 'baseten', name: 'Baseten', apiHost: 'https://inference.baseten.co/v1', website: 'https://baseten.co',
    category: 'intl', color: '#111827',
    description: { en: 'Dedicated + serverless model APIs', zh: '专用/无服务器模型 API' },
    freeNote: { en: '$30 free trial credits', zh: '注册送 $30 试用金' },
  },
  {
    id: 'nscale', name: 'nScale', apiHost: 'https://inference.api.nscale.com/v1', website: 'https://nscale.com',
    category: 'intl', color: '#0891B2',
    description: { en: 'European GPU inference', zh: '欧洲 GPU 推理服务' },
    freeNote: { en: '$5 free credits on signup', zh: '注册送 $5 免费额度' },
  },
  {
    id: 'featherless-ai', name: 'Featherless AI', apiHost: 'https://api.featherless.ai/v1', website: 'https://featherless.ai',
    category: 'intl', color: '#EA580C',
    description: { en: 'Unlimited open-model subscriptions', zh: '开源模型订阅制推理' },
    freeNote: { en: 'Free tier available — no credit card required', zh: '提供免费层,免绑卡' },
  },
  {
    id: 'friendliai', name: 'FriendliAI', apiHost: 'https://api.friendli.ai/serverless/v1', website: 'https://friendli.ai',
    category: 'intl', color: '#EC4899',
    description: { en: 'Serverless endpoints for open models', zh: '开源模型无服务器端点' },
    freeNote: { en: 'Free tier for serverless inference — no credit card', zh: '无服务器推理免费层,免绑卡' },
  },
  {
    id: 'inference-net', name: 'Inference.net', apiHost: 'https://api.inference.net/v1', website: 'https://inference.net',
    category: 'intl', color: '#2563EB',
    description: { en: 'Distributed GPU network inference', zh: '分布式 GPU 网络推理' },
    freeNote: { en: '$25 free credits on signup', zh: '注册送 $25 免费额度' },
  },
  {
    id: 'wandb', name: 'W&B Inference', apiHost: 'https://api.inference.wandb.ai/v1', website: 'https://wandb.ai',
    category: 'intl', color: '#FFBE0B',
    description: { en: 'Weights & Biases hosted inference', zh: 'Weights & Biases 托管推理' },
  },
  {
    id: 'ollama-cloud', name: 'Ollama Cloud', apiHost: 'https://ollama.com/v1', website: 'https://ollama.com',
    category: 'intl', color: '#58A6FF',
    description: { en: 'Hosted Ollama models', zh: 'Ollama 官方云端模型' },
    freeNote: { en: 'Free tier available', zh: '提供免费层' },
  },
  {
    id: 'digitalocean', name: 'DigitalOcean Gradient', apiHost: 'https://inference.do-ai.run/v1', website: 'https://docs.digitalocean.com/products/ai-platform/',
    category: 'intl', color: '#0060FF',
    description: { en: 'DigitalOcean serverless inference', zh: 'DigitalOcean 无服务器推理' },
  },
  {
    id: 'scaleway', name: 'Scaleway', apiHost: 'https://api.scaleway.ai/v1', website: 'https://www.scaleway.com',
    category: 'intl', color: '#4F0599',
    description: { en: 'European cloud generative APIs', zh: '欧洲云生成式 API' },
  },
  {
    id: 'ovhcloud', name: 'OVHcloud AI', apiHost: 'https://oai.endpoints.kepler.ai.cloud.ovh.net/v1', website: 'https://www.ovhcloud.com',
    category: 'intl', color: '#000E9C',
    description: { en: 'European cloud AI endpoints', zh: '欧洲云 AI 端点' },
  },

  // ===== 国内主流 =====
  {
    id: 'deepseek', name: 'DeepSeek', apiHost: 'https://api.deepseek.com', website: 'https://platform.deepseek.com',
    category: 'cn', color: '#4D6BFE', anthropicHost: 'https://api.deepseek.com/anthropic',
    description: { en: 'DeepSeek V3 / R1', zh: '深度求索 V3 / R1' }, defaultModel: 'deepseek-chat',
  },
  {
    id: 'qwen', name: { en: 'Alibaba Cloud Bailian', zh: '阿里云百炼' },
    apiHost: 'https://dashscope.aliyuncs.com/compatible-mode/v1', website: 'https://bailian.console.aliyun.com',
    // Anthropic 兼容层是 /apps/anthropic(仅 Messages 接口,无 /v1/models);
    // compatible-mode/v1 是 OpenAI 兼容端点,两者不可混用。
    category: 'cn', color: '#615CED', anthropicHost: 'https://dashscope.aliyuncs.com/apps/anthropic',
    description: { en: 'Bailian platform · Qwen family', zh: '百炼平台 · 通义千问 Qwen 系列' }, defaultModel: 'qwen-max',
  },
  {
    id: 'zhipu', name: { en: 'Zhipu AI', zh: '智谱' },
    apiHost: 'https://open.bigmodel.cn/api/paas/v4', website: 'https://open.bigmodel.cn',
    category: 'cn', color: '#1E6FFF', anthropicHost: 'https://open.bigmodel.cn/api/anthropic',
    description: { en: 'GLM model family', zh: 'GLM 系列模型' }, defaultModel: 'glm-4-plus',
  },
  {
    id: 'moonshot', name: { en: 'Moonshot AI', zh: '月之暗面' },
    apiHost: 'https://api.moonshot.cn/v1', website: 'https://platform.moonshot.cn',
    category: 'cn', color: '#16181D', anthropicHost: 'https://api.moonshot.cn/anthropic',
    description: { en: 'Kimi / Moonshot models', zh: 'Kimi / Moonshot' }, defaultModel: 'moonshot-v1-8k',
  },
  {
    id: 'baichuan', name: { en: 'Baichuan', zh: '百川' },
    apiHost: 'https://api.baichuan-ai.com/v1', website: 'https://platform.baichuan-ai.com',
    category: 'cn', color: '#F7941E',
    description: { en: 'Baichuan large models', zh: '百川大模型' }, defaultModel: 'Baichuan4',
  },
  {
    id: 'minimax', name: 'MiniMax', apiHost: 'https://api.minimaxi.com/v1', website: 'https://platform.minimaxi.com',
    category: 'cn', color: '#FF6B6B', anthropicHost: 'https://api.minimaxi.com/anthropic',
    description: { en: 'MiniMax model family', zh: 'MiniMax 系列' }, defaultModel: 'MiniMax-Text-01',
  },
  {
    id: 'hunyuan', name: { en: 'Tencent Hunyuan', zh: '腾讯混元' },
    apiHost: 'https://api.hunyuan.cloud.tencent.com/v1', website: 'https://cloud.tencent.com/product/hunyuan',
    category: 'cn', color: '#0053E0',
    description: { en: 'Tencent Hunyuan models', zh: '腾讯混元大模型' }, defaultModel: 'hunyuan-pro',
  },
  {
    id: 'yi', name: { en: '01.AI', zh: '零一万物' },
    apiHost: 'https://api.lingyiwanwu.com/v1', website: 'https://platform.lingyiwanwu.com',
    category: 'cn', color: '#00B8A9',
    description: { en: 'Yi model family', zh: 'Yi 系列模型' }, defaultModel: 'yi-large',
  },
  {
    id: 'doubao', name: { en: 'ByteDance Doubao', zh: '字节豆包' },
    apiHost: 'https://ark.cn-beijing.volces.com/api/v3', website: 'https://www.volcengine.com/product/doubao',
    category: 'cn', color: '#1664FF',
    description: { en: 'Doubao on Volcengine Ark', zh: '火山方舟豆包' }, defaultModel: 'doubao-pro',
  },
  {
    id: 'stepfun', name: { en: 'StepFun', zh: '阶跃星辰' },
    apiHost: 'https://api.stepfun.com/v1', website: 'https://platform.stepfun.com',
    category: 'cn', color: '#7B61FF',
    description: { en: 'Step model family', zh: 'Step 系列模型' }, defaultModel: 'step-2-16k',
  },
  {
    id: 'qianfan', name: { en: 'Baidu Qianfan', zh: '百度千帆' },
    apiHost: 'https://qianfan.baidubce.com/v2', website: 'https://qianfan.cloud.baidu.com',
    category: 'cn', color: '#2932E1',
    description: { en: 'ERNIE / Qianfan platform', zh: '文心一言 / 千帆' }, defaultModel: 'ernie-4.0-8k-latest',
  },
  {
    id: 'longcat', name: { en: 'Meituan LongCat', zh: '美团 LongCat' },
    apiHost: 'https://api.longcat.chat/openai/v1', website: 'https://longcat.chat',
    category: 'cn', color: '#FFC300',
    description: { en: 'Meituan large models', zh: '美团大模型' }, defaultModel: 'longcat-flash',
  },
  {
    id: 'ctyun', name: { en: 'China Telecom Cloud', zh: '天翼云' },
    apiHost: 'https://wishub-x1.ctyun.cn/v1', website: 'https://ctyun.cn',
    category: 'cn', color: '#E60012',
    description: { en: 'CTYun Xingchen models', zh: '天翼云星辰' }, defaultModel: '星辰大模型',
  },

  {
    id: 'zai', name: { en: 'Z.AI (GLM Intl)', zh: 'Z.AI(智谱国际版)' },
    apiHost: 'https://api.z.ai/api/coding/paas/v4', website: 'https://z.ai',
    category: 'cn', color: '#2563EB', anthropicHost: 'https://api.z.ai/api/anthropic',
    description: { en: 'GLM international platform', zh: 'GLM 国际版平台' },
  },
  {
    id: 'moonshot-global', name: { en: 'Moonshot AI (Global)', zh: '月之暗面(国际版)' },
    apiHost: 'https://api.moonshot.ai/v1', website: 'https://platform.moonshot.ai',
    category: 'cn', color: '#1E40AF',
    description: { en: 'Kimi international endpoint', zh: 'Kimi 国际端点' },
  },
  {
    id: 'bailian-intl', name: { en: 'Alibaba Model Studio (Intl)', zh: '阿里云百炼(国际版)' },
    apiHost: 'https://dashscope-intl.aliyuncs.com/compatible-mode/v1', website: 'https://bailian.console.alibabacloud.com',
    category: 'cn', color: '#FF6600',
    description: { en: 'Qwen international endpoint', zh: '通义千问国际端点' },
  },
  {
    id: 'byteplus', name: 'BytePlus ModelArk', apiHost: 'https://ark.ap-southeast.bytepluses.com/api/v3', website: 'https://console.byteplus.com/ark',
    category: 'cn', color: '#2563EB',
    description: { en: 'ByteDance international Ark platform', zh: '字节跳动国际版方舟平台' },
    freeNote: { en: 'Free credits for new accounts', zh: '新账户送免费额度' },
  },
  {
    id: 'xiaomi-mimo', name: { en: 'Xiaomi MiMo', zh: '小米 MiMo' },
    apiHost: 'https://api.xiaomimimo.com/v1', website: 'https://mimo.mi.com',
    category: 'cn', color: '#EA580C',
    description: { en: 'Xiaomi MiMo models', zh: '小米 MiMo 系列模型' },
  },

  // ===== 聚合 / 中转 =====
  {
    id: 'siliconflow', name: { en: 'SiliconFlow', zh: '硅基流动' },
    apiHost: 'https://api.siliconflow.cn/v1', website: 'https://siliconflow.cn',
    category: 'aggregator', color: '#FF8A00',
    description: { en: 'Aggregated open-source models', zh: '聚合海量开源模型' },
  },
  {
    id: '302ai', name: '302.AI', apiHost: 'https://api.302.ai/v1', website: 'https://302.ai',
    category: 'aggregator', color: '#2D6CDF',
    description: { en: 'Multi-model aggregator', zh: '多模型聚合中转' },
  },
  {
    id: 'aihubmix', name: 'AiHubMix', apiHost: 'https://aihubmix.com/v1', website: 'https://aihubmix.com',
    category: 'aggregator', color: '#6C5CE7',
    description: { en: 'Aggregated API gateway', zh: '聚合 API 中转' },
  },
  {
    id: 'dmxapi', name: 'DMXAPI', apiHost: 'https://www.dmxapi.cn/v1', website: 'https://dmxapi.cn',
    category: 'aggregator', color: '#F5A623',
    description: { en: 'Aggregator + image generation', zh: '聚合中转 + 绘图' },
  },
  {
    id: 'modelscope', name: 'ModelScope', apiHost: 'https://api-inference.modelscope.cn/v1', website: 'https://modelscope.cn',
    category: 'aggregator', color: '#6B4FBB',
    description: { en: 'ModelScope community inference', zh: '魔搭社区推理' },
  },
  {
    id: 'tokenflux', name: 'TokenFlux', apiHost: 'https://api.tokenflux.ai/openai/v1', website: 'https://tokenflux.ai',
    category: 'aggregator', color: '#00C2A8',
    description: { en: 'Aggregated gateway', zh: '聚合中转' },
  },
  {
    id: 'cherryai', name: 'Cherry AI', apiHost: 'https://api.cherry-ai.com/v1', website: 'https://cherry-ai.com',
    category: 'aggregator', color: '#FF4D4F',
    description: { en: 'Cherry official gateway', zh: 'Cherry 官方中转' },
  },
  {
    id: 'infini', name: 'Infini AI', apiHost: 'https://cloud.infini-ai.com/maas', website: 'https://infini-ai.com',
    category: 'aggregator', color: '#00B4D8',
    description: { en: 'Infini heterogeneous cloud', zh: 'Infini 异构云' },
  },

  {
    id: 'agentrouter', name: 'AgentRouter', apiHost: 'https://agentrouter.org/v1', website: 'https://agentrouter.org',
    category: 'aggregator', color: '#10B981',
    description: { en: 'Multi-model routing gateway', zh: '多模型路由网关' },
    freeNote: { en: '$200 free credits on signup — no credit card', zh: '注册送 $200 免费额度,免绑卡' },
  },
  {
    id: 'requesty', name: 'Requesty', apiHost: 'https://router.requesty.ai/v1', website: 'https://requesty.ai',
    category: 'aggregator', color: '#6366F1',
    description: { en: '300+ models, one router', zh: '一个路由聚合 300+ 模型' },
    freeNote: { en: 'Free tier ~200 requests/day', zh: '免费层约每日 200 次请求' },
  },
  {
    id: 'novita', name: 'Novita AI', apiHost: 'https://api.novita.ai/openai/v1', website: 'https://novita.ai',
    category: 'aggregator', color: '#FF4081',
    description: { en: 'Aggregated model marketplace', zh: '聚合模型市场' },
    freeNote: { en: '$0.50 trial credits on signup (~1-year validity)', zh: '注册送 $0.5 试用金(约 1 年有效)' },
  },
  {
    id: 'aimlapi', name: 'AI/ML API', apiHost: 'https://api.aimlapi.com/v1', website: 'https://aimlapi.com',
    category: 'aggregator', color: '#6366F1',
    description: { en: '400+ models behind one API', zh: '一个 API 聚合 400+ 模型' },
  },
  {
    id: 'nanogpt', name: 'NanoGPT', apiHost: 'https://nano-gpt.com/api/v1', website: 'https://nano-gpt.com',
    category: 'aggregator', color: '#4F46E5',
    description: { en: 'Pay-per-use model aggregator', zh: '按量付费模型聚合' },
  },
  {
    id: 'vercel-ai-gateway', name: 'Vercel AI Gateway', apiHost: 'https://ai-gateway.vercel.sh/v1', website: 'https://vercel.com/docs/ai-gateway',
    category: 'aggregator', color: '#111827',
    description: { en: 'Vercel official model gateway', zh: 'Vercel 官方模型网关' },
  },
  {
    id: 'poe', name: 'Poe', apiHost: 'https://api.poe.com/v1', website: 'https://creator.poe.com/api-reference',
    category: 'aggregator', color: '#F97316',
    description: { en: 'Poe subscription-backed API', zh: 'Poe 订阅额度 API' },
  },
  {
    id: 'chutes', name: 'Chutes.ai', apiHost: 'https://llm.chutes.ai/v1', website: 'https://chutes.ai',
    category: 'aggregator', color: '#06B6D4',
    description: { en: 'Decentralized open-model inference', zh: '去中心化开源模型推理' },
  },
  {
    id: 'zenmux', name: 'ZenMux', apiHost: 'https://zenmux.ai/api/v1', website: 'https://zenmux.ai',
    category: 'aggregator', color: '#7C3AED', anthropicHost: 'https://zenmux.ai/api/anthropic',
    description: { en: 'Multi-protocol model gateway', zh: '多协议模型网关' },
    freeNote: { en: 'Free tier: Gemini Flash, DeepSeek, Grok Fast and more', zh: '免费层含 Gemini Flash、DeepSeek、Grok Fast 等' },
  },
  {
    id: 'qiniu', name: { en: 'Qiniu AI', zh: '七牛云 AI' },
    apiHost: 'https://api.qnaigc.com/v1', website: 'https://www.qiniu.com',
    category: 'aggregator', color: '#1E88E5',
    description: { en: 'One key for DeepSeek/Claude/Kimi and more', zh: '一把 key 代理 DeepSeek/Claude/Kimi 等' },
  },
  {
    id: 'omniroute', name: 'OmniRoute', apiHost: 'http://localhost:20128/v1', website: 'https://github.com/OmniRouteAI/omniroute',
    category: 'aggregator', color: '#7C5CFF',
    description: { en: 'Local multi-provider gateway (self-hosted, install required)', zh: '本地多服务商网关(需自行安装运行)' },
  },

  // ===== 本地 =====
  {
    id: 'ollama', name: 'Ollama', apiHost: 'http://localhost:11434/v1', website: 'https://ollama.com',
    category: 'local', color: '#E8E8E8',
    description: { en: 'Local model runtime', zh: '本地模型运行时' },
  },
  {
    id: 'lmstudio', name: 'LM Studio', apiHost: 'http://localhost:1234/v1', website: 'https://lmstudio.ai',
    category: 'local', color: '#5046E5',
    description: { en: 'Local model desktop app', zh: '本地模型桌面端' },
  },
];
