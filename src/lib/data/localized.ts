// 数据目录的内嵌双语文案:目录条目是数据不是 UI 框架文案,双语值直接内嵌
// 在数据文件里({en, zh}),不塞进 i18n json。组件用 $locale + localize()
// 响应式取值,语言切换即时生效。

export interface Localized {
  en: string;
  zh: string;
}

/** 取当前语言的值;字段允许 string(两语言相同,如国际厂商名)或 Localized */
export function localize(value: string | Localized, locale: string | null | undefined): string {
  if (typeof value === 'string') return value;
  return locale?.startsWith('zh') ? value.zh : value.en;
}
