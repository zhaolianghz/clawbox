# 内置「官方默认」服务商实现计划

日期:2026-08-27 · spec:2026-08-27-default-provider-design.md

## 任务

- [x] 1. 后端:DEFAULT_PROVIDER_ID 哨兵 + config_providers_get 注入虚拟
  条目;bind_at 拦截(走解绑路径、保留绑定);plan_all 认哨兵;
  provider_update/delete 守卫。cargo test 全绿(TDD:先写测试)。
- [x] 2. 前端:Agents 页绑定选择器头部加「官方默认」选项(全 agent 可选);
  Providers 页过滤 `__default__`;i18n en/zh;npm run check 0 错误。
- [x] 3. (本地构建安装后人工验证) 文档:ROADMAP 登记;本地构建安装人工验证(绑定→同步→文件恢复
  →再绑定真实服务商)后才发布。
