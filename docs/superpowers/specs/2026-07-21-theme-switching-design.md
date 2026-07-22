# 主题(皮肤)切换 设计

**日期**: 2026-07-21
**状态**: 待用户认可

## 背景与目标

ClawBox 目前只有一套深色霓虹配色,写死在 `src/app.css` 的 `:root` CSS 变量里,无法切换。
本设计新增**主题切换**:**跟随系统 / 浅色 / 深色** 三选一,首次启动默认「跟随系统」;
选「跟随系统」时随操作系统明暗实时联动。偏好本地持久化,启动即应用(无首帧闪烁)。

## 现状核对(2026-07-21 本机实测)

- 生效样式是 `src/app.css`:`:root` 定义整套变量(`--bg-*`、`--neon-*`、`--text-*`、
  `--glow-*`、`--radius-*`),全站组件引用。
- `src/styles.css` 未被任何地方 import(死文件,含 skeleton 主题引入),**本次不动它**。
- **没有**任何浅色变量、`prefers-color-scheme` 监听、`data-theme` 机制。
- 语言设置已有可照搬的模式:`localStorage('clawbox.locale')` + 系统探测,启动时在
  `src/lib/i18n/index.ts` 应用;切换器 UI 在 `SettingsLayout.svelte` 页脚 `.lang-switch`
  (两个 `.lang-btn`)。
- **注意**:部分组件 `<style>` 与 `app.css` 里有**写死的 rgba**(如 `.glass-card` 的
  `rgba(26,26,40,.9)` 渐变、`rgba(255,255,255,0.08)` 边框、`rgba(0,0,0,0.3)` 阴影、
  各页局部霓虹 rgba)。这些不吃 CSS 变量,是浅色主题的**主要工作量**(见第 3 节)。

## 设计决策(已与用户确认)

1. **三选项**:跟随系统 / 浅色 / 深色;首次默认「跟随系统」。「深色」= 现有霓虹外观。
2. **浅色范围**:做**完整**浅色配色(非仅开关)。
3. **强调色**:浅色下**保留霓虹强调色**,但**降饱和/加深**以保证白底可读(跨主题维持品牌识别)。
4. **持久化**:只存前端 `localStorage`,后端 `config.json` 不掺和(与 locale 一致;单机桌面无跨端同步需求)。

## 第 1 节:主题机制

新增 `src/lib/theme.ts`,导出:

```ts
export type ThemeChoice = 'system' | 'light' | 'dark';
export type Resolved = 'light' | 'dark';

// Svelte store,存用户「选择」(system|light|dark)
export const themeChoice = writable<ThemeChoice>(initialChoice());

// 读 localStorage('clawbox.theme');非法/缺失兜底 'system'
function initialChoice(): ThemeChoice { ... }

// 把 choice 解析成实际明暗:system → matchMedia 结果;否则原值
function resolve(choice: ThemeChoice): Resolved { ... }

// 应用:在 <html> 上设 data-theme=resolved,并设 style.colorScheme=resolved
function apply(choice: ThemeChoice): void { ... }

// 用户切换:更新 store + 写 localStorage + apply
export function setTheme(choice: ThemeChoice): void { ... }

// 启动初始化:apply(initial) + 挂 matchMedia change 监听
// (仅当当前 choice==='system' 时,系统明暗变化才重算 apply)
export function initTheme(): void { ... }
```

- **`system` 联动**:`window.matchMedia('(prefers-color-scheme: dark)')` 加 `change` 监听;
  回调里若 `get(themeChoice) === 'system'` 则重新 `apply`,实现实时跟随。
- **防闪烁**:在 `src/main.ts` 里 `mount(App, ...)` **之前**调 `initTheme()`,首帧即带
  正确 `data-theme`。
- **归属**:`data-theme` 打在 `document.documentElement`(`<html>`),`color-scheme` 同址,
  让原生控件(滚动条/下拉/input)跟随明暗。

## 第 2 节:配色变量分层

`app.css` 现有 `:root { ... }` 改为**深色为默认**、浅色为覆盖:

```css
:root, :root[data-theme="dark"] {
  /* 原有整套深色变量,原样保留(零回归) */
}

:root[data-theme="light"] {
  --bg-primary: #f7f8fa;   --bg-secondary: #ffffff;
  --bg-tertiary: #eef1f5;  --bg-elevated: #ffffff;
  --text-primary: #0f172a; --text-secondary: #475569; --text-muted: #94a3b8;
  /* 降饱和/加深的霓虹族,保证白底对比度 */
  --neon-cyan: #0891b2;  --neon-purple: #7c3aed; --neon-pink: #db2777;
  --neon-green: #059669;  --neon-orange: #d97706;
  /* 浅色下辉光大幅减弱为柔和投影(白底强辉光会脏) */
  --glow-cyan:  0 2px 12px rgba(8,145,178,0.18);
  --glow-purple:0 2px 12px rgba(124,58,237,0.18);
  --glow-green: 0 2px 10px rgba(5,150,105,0.16);
  --glow-pink:  0 2px 10px rgba(219,39,119,0.16);
}
```

(具体色值为推荐初值,实现时按逐页目测微调。)

## 第 3 节:写死 rgba 的浅色适配(主要工作量)

不吃变量的写死颜色,分两步处理:

**① 提炼语义 token(减少写死点)**:在变量层新增少量语义 token,并把最常见的写死点改用它们:
- `--border-subtle`(深:`rgba(255,255,255,0.08)` / 浅:`rgba(15,23,42,0.10)`)
- `--card-bg`(深:现有玻璃渐变 / 浅:`rgba(255,255,255,0.85)` 磨砂)
- `--shadow-card`(深:`0 4px 24px rgba(0,0,0,0.3)` / 浅:`0 4px 16px rgba(15,23,42,0.08)`)
- `--hover-surface`(深:`rgba(255,255,255,0.05)` / 浅:`rgba(15,23,42,0.04)`)

先覆盖 **`app.css` 里的共享类**(`.glass-card` / `.neon-button` / `.neon-input` / `select`
/ `.nav-item` / `.tab-button` / 滚动条 / `.neon-badge`),改用上述 token。

**② 逐页目测收尾**:各页 `+page.svelte` 与组件 `<style>` 里的局部写死色(如 providers 页的
`rgba(0,245,255,0.12)` 高亮、`#f87171` 错误红等)在浅色下逐一核对;确有穿帮/不可读的,
就地加 `:root[data-theme="light"] .xxx { ... }` 覆盖或改用 token。覆盖范围:
providers / mcp / agents / capabilities / about 五页 + `InstallWizard` + `SettingsLayout`
+ `Dashboard/StatsCard` + `ProviderLogo`/`AgentLogo`(logo 底色在浅色下的可读性)。

> 原则:**深色路径零改动语义**(只是把写死值抽成 token 的深色分支,值不变),浅色是新增分支。

## 第 4 节:切换器 UI

`SettingsLayout.svelte` 页脚,在 `.lang-switch` **旁边**新增 `.theme-switch`:三档
segmented 控件(图标:显示器=跟随系统 / 太阳=浅色 / 月亮=深色),复用 `.lang-btn` 视觉,
`aria-label` + `title` 走 i18n。点击调 `setTheme(choice)`,`class:active` 绑 `$themeChoice`。

`i18n/zh.json` / `en.json` 新增:`theme.label`(主题/Theme)、`theme.system`(跟随系统/System)、
`theme.light`(浅色/Light)、`theme.dark`(深色/Dark)。

## 第 5 节:测试

纯前端,无后端改动。
- `npm run check` 通过(类型 + svelte-check)。
- **手动验收**:
  1. 三档点击即时生效;
  2. 刷新/重开后保持上次选择;
  3. 选「跟随系统」时,改操作系统明暗设置,页面实时切换(matchMedia 监听);
  4. 首帧无深→浅闪烁(initTheme 在 mount 前);
  5. 逐页(五页 + 向导 + 侧栏)在浅色下无不可读文字、无穿帮的深色残块。

## 非目标 / 明确不做

- 不做自定义主题色 / 多套配色(只 浅 + 深 两套)。
- 不清理/改动死文件 `src/styles.css`。
- 主题不写入后端 `~/.clawbox/config.json`,不做跨设备同步。
- 不引入第三方主题库(skeleton 主题虽被 styles.css 引入但该文件未生效,不启用)。
