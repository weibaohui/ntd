// 专家列表接口 mock 载荷构造（spec 共用）。
//
// 背景：多个 spec（026 分享用例、check_expert_source_filter 来源筛选）需要
// route mock 注入确定性专家数据——真实后端只保证系统专家（用户专家目录
// ~/.ntd/experts/ 在干净环境不存在），不 mock 的话「用户来源」卡片永远不出现，
// 用例只能 15s 超时。载荷字段曾是两处内联重复，收敛到本 helper 防漂移。

/** 最小可渲染的 ExpertMetadata：ExpertCard 只读展示字段 + source 驱动筛选/分享守卫。 */
export function mockExpert(name: string, source: 'user' | 'system') {
  return {
    name,
    expert_type: 'agent',
    version: '1.0.0',
    display_name_zh: name,
    // definition_dir 用 /home/tester 前缀而非 /Users/tester：toHomePath 只按 /.ntd/
    // 标记定位（ShareToRepoButton.tsx），平台无关写法避免 macOS 路径形态的隐性假设。
    definition_dir: `/home/tester/.ntd/experts/${name}`,
    plugin_json_path: `/home/tester/.ntd/experts/${name}/.codebuddy-plugin/plugin.json`,
    member_agents: [],
    skills: [],
    tags: [],
    loaded_at: '2026-01-01T00:00:00Z',
    is_active: true,
    source,
  };
}

/** 包装为 route.fulfill 参数：后端统一响应壳 { code, data }。 */
export function mockExpertsResponse(experts: ReturnType<typeof mockExpert>[]) {
  return {
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ code: 0, data: experts }),
  };
}
