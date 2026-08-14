// 服务商/agent 真实品牌 logo:来自 @lobehub/icons-static-svg(lobe-icons,MIT)。
// `?raw` 构建期静态内联为 SVG 字符串——离线可用、零运行时请求。
// 优先 `-color` 品牌彩色版;无彩色版的用单色版(currentColor,渲染侧用品牌色上色)。
// 未命中的条目由 ProviderLogo/AgentLogo 回退到首字母/手绘图形。

// ===== 服务商目录(key = ProviderCatalogEntry.id)=====
import openai from '@lobehub/icons-static-svg/icons/openai.svg?raw';
import anthropic from '@lobehub/icons-static-svg/icons/anthropic.svg?raw';
import gemini from '@lobehub/icons-static-svg/icons/gemini-color.svg?raw';
import mistral from '@lobehub/icons-static-svg/icons/mistral-color.svg?raw';
import grok from '@lobehub/icons-static-svg/icons/grok.svg?raw';
import perplexity from '@lobehub/icons-static-svg/icons/perplexity-color.svg?raw';
import groq from '@lobehub/icons-static-svg/icons/groq.svg?raw';
import openrouter from '@lobehub/icons-static-svg/icons/openrouter-color.svg?raw';
import together from '@lobehub/icons-static-svg/icons/together-color.svg?raw';
import fireworks from '@lobehub/icons-static-svg/icons/fireworks-color.svg?raw';
import github from '@lobehub/icons-static-svg/icons/github.svg?raw';
import nvidia from '@lobehub/icons-static-svg/icons/nvidia-color.svg?raw';
import cerebras from '@lobehub/icons-static-svg/icons/cerebras-color.svg?raw';
import huggingface from '@lobehub/icons-static-svg/icons/huggingface-color.svg?raw';
import cohere from '@lobehub/icons-static-svg/icons/cohere-color.svg?raw';
import metaLlama from '@lobehub/icons-static-svg/icons/meta-color.svg?raw';
import ai21 from '@lobehub/icons-static-svg/icons/ai21.svg?raw';
import venice from '@lobehub/icons-static-svg/icons/venice-color.svg?raw';
import codestral from '@lobehub/icons-static-svg/icons/mistral-color.svg?raw';
import upstage from '@lobehub/icons-static-svg/icons/upstage-color.svg?raw';
import nousResearch from '@lobehub/icons-static-svg/icons/nousresearch.svg?raw';
import morph from '@lobehub/icons-static-svg/icons/morph-color.svg?raw';
import deepinfra from '@lobehub/icons-static-svg/icons/deepinfra-color.svg?raw';
import hyperbolic from '@lobehub/icons-static-svg/icons/hyperbolic-color.svg?raw';
import sambanova from '@lobehub/icons-static-svg/icons/sambanova-color.svg?raw';
import lambdaAi from '@lobehub/icons-static-svg/icons/lambda.svg?raw';
import nebius from '@lobehub/icons-static-svg/icons/nebius.svg?raw';
import baseten from '@lobehub/icons-static-svg/icons/baseten.svg?raw';
import featherlessAi from '@lobehub/icons-static-svg/icons/featherless-color.svg?raw';
import friendliai from '@lobehub/icons-static-svg/icons/friendli.svg?raw';
import ollama from '@lobehub/icons-static-svg/icons/ollama.svg?raw';
import deepseek from '@lobehub/icons-static-svg/icons/deepseek-color.svg?raw';
import qwen from '@lobehub/icons-static-svg/icons/bailian-color.svg?raw';
import zhipu from '@lobehub/icons-static-svg/icons/zhipu-color.svg?raw';
import moonshot from '@lobehub/icons-static-svg/icons/moonshot.svg?raw';
import baichuan from '@lobehub/icons-static-svg/icons/baichuan-color.svg?raw';
import minimax from '@lobehub/icons-static-svg/icons/minimax-color.svg?raw';
import hunyuan from '@lobehub/icons-static-svg/icons/hunyuan-color.svg?raw';
import yi from '@lobehub/icons-static-svg/icons/yi-color.svg?raw';
import doubao from '@lobehub/icons-static-svg/icons/doubao-color.svg?raw';
import stepfun from '@lobehub/icons-static-svg/icons/stepfun-color.svg?raw';
import qianfan from '@lobehub/icons-static-svg/icons/wenxin-color.svg?raw';
import longcat from '@lobehub/icons-static-svg/icons/longcat-color.svg?raw';
import zai from '@lobehub/icons-static-svg/icons/zai.svg?raw';
import byteplus from '@lobehub/icons-static-svg/icons/volcengine-color.svg?raw';
import xiaomiMimo from '@lobehub/icons-static-svg/icons/xiaomimimo.svg?raw';
import siliconflow from '@lobehub/icons-static-svg/icons/siliconcloud-color.svg?raw';
import ai302 from '@lobehub/icons-static-svg/icons/ai302-color.svg?raw';
import aihubmix from '@lobehub/icons-static-svg/icons/aihubmix-color.svg?raw';
import modelscope from '@lobehub/icons-static-svg/icons/modelscope-color.svg?raw';
import cherryai from '@lobehub/icons-static-svg/icons/cherrystudio-color.svg?raw';
import infini from '@lobehub/icons-static-svg/icons/infinigence-color.svg?raw';
import novita from '@lobehub/icons-static-svg/icons/novita-color.svg?raw';
import vercelAiGateway from '@lobehub/icons-static-svg/icons/vercel.svg?raw';
import poe from '@lobehub/icons-static-svg/icons/poe-color.svg?raw';
import zenmux from '@lobehub/icons-static-svg/icons/zenmux.svg?raw';
import qiniu from '@lobehub/icons-static-svg/icons/qiniu-color.svg?raw';
import lmstudio from '@lobehub/icons-static-svg/icons/lmstudio.svg?raw';

// ===== agent 注册表(key = agent id)=====
import claudeCode from '@lobehub/icons-static-svg/icons/claudecode-color.svg?raw';
import kimi from '@lobehub/icons-static-svg/icons/kimi-color.svg?raw';
import openclaw from '@lobehub/icons-static-svg/icons/openclaw-color.svg?raw';
import opencode from '@lobehub/icons-static-svg/icons/opencode.svg?raw';
import codebuddy from '@lobehub/icons-static-svg/icons/codebuddy-color.svg?raw';
import cursorAgent from '@lobehub/icons-static-svg/icons/cursor.svg?raw';
import qodercli from '@lobehub/icons-static-svg/icons/qoder-color.svg?raw';
import geminiCli from '@lobehub/icons-static-svg/icons/geminicli-color.svg?raw';
import cline from '@lobehub/icons-static-svg/icons/cline.svg?raw';
import qwenCode from '@lobehub/icons-static-svg/icons/qwen-color.svg?raw';
import trae from '@lobehub/icons-static-svg/icons/trae-color.svg?raw';

/** 服务商目录 id → 内联 SVG。未命中的 id 不在表内(回退首字母)。 */
export const PROVIDER_ICONS: Record<string, string> = {
  openai,
  anthropic,
  gemini,
  mistral,
  grok,
  perplexity,
  groq,
  openrouter,
  together,
  fireworks,
  github,
  nvidia,
  cerebras,
  huggingface,
  cohere,
  'meta-llama': metaLlama,
  ai21,
  venice,
  codestral,
  upstage,
  'nous-research': nousResearch,
  morph,
  deepinfra,
  hyperbolic,
  sambanova,
  'lambda-ai': lambdaAi,
  nebius,
  baseten,
  'featherless-ai': featherlessAi,
  friendliai,
  'ollama-cloud': ollama,
  deepseek,
  qwen,
  'bailian-intl': qwen,
  zhipu,
  moonshot,
  'moonshot-global': moonshot,
  baichuan,
  minimax,
  hunyuan,
  yi,
  doubao,
  stepfun,
  qianfan,
  longcat,
  zai,
  byteplus,
  'xiaomi-mimo': xiaomiMimo,
  siliconflow,
  '302ai': ai302,
  aihubmix,
  modelscope,
  cherryai,
  infini,
  novita,
  'vercel-ai-gateway': vercelAiGateway,
  poe,
  zenmux,
  qiniu,
  ollama,
  lmstudio,
};

/** agent id → 内联 SVG(AgentLogo 用)。 */
export const AGENT_ICONS: Record<string, string> = {
  'claude-code': claudeCode,
  codex: openai,
  kimi,
  openclaw,
  opencode,
  codebuddy,
  'cursor-agent': cursorAgent,
  qodercli,
  gemini: geminiCli,
  cline,
  'qwen-code': qwenCode,
  'trae-agent': trae,
  // pi:lobe-icons 无该品牌,AgentLogo 回退 π 字符
};
