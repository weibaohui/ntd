// 工艺模板测试辅助（029 系列 spec 共用）。
//
// 背景：040 之后工艺模板改用 guid 寻址——name 允许重复、不再唯一，详情/编辑一律按 guid。
// 因此 029 的编辑器类 spec 不能再写 `?processMode=edit&name=xxx`（会被当成无 guid 的 edit 请求，
// 编辑器拿不到目标工艺，ProcessPage 回退渲染列表页，导致 spec 整批「元素找不到」失败）。
//
// 正确写法是先按 name 查出 guid，再拼 `?processMode=edit&guid=…`。集中在本 helper，
// 供 029-m4 / 029-m5 等编辑器 spec 复用，避免每个 spec 重复实现查 guid 的逻辑。

import type { APIRequestContext } from '@playwright/test';

/** 工艺模板列表项（只声明测试关心的字段，省得引入完整 ProcessTemplate 类型）。 */
interface ProcessTemplateSummary {
  guid: string;
  name: string;
  is_system?: boolean;
}

/**
 * 按 name 查「系统工艺」的 guid，返回其编辑器直链（`?processMode=edit&guid=…`）。
 *
 * 仅在系统工艺里查：029 的编辑器 spec 固定用 bundled 系统工艺（如 4p12s-delivery），
 * 其 guid 由内嵌 YAML 决定、跨环境稳定；用 API 动态取 guid 而非硬编码，是为了在
 * 重新播种 / bundled 工艺改名时给出清晰报错，而不是悄悄渲染列表页让 spec 误判。
 *
 * `base` 默认开发服务 18088；支持传入是为了兼容用 `process.env.UI_BASE` 覆盖端口的 spec。
 *
 * 找不到时抛错（不静默回退）——让失败原因一目了然，区别于「编辑器有 bug」。
 */
export async function editUrlByName(
  request: APIRequestContext,
  name: string,
  base = 'http://localhost:18088',
): Promise<string> {
  // 只取系统工艺，缩小匹配范围；is_system=true 走服务端过滤（见 bundled.ts getProcesses）。
  const r = await request.get(`${base}/api/bundled/processes?is_system=true`);
  if (!r.ok()) {
    // 开发服务没起 / 端口不对时给出可操作提示，而非 opaque 的 JSON 解析错。
    throw new Error(`GET /api/bundled/processes 失败（HTTP ${r.status()}），确认开发服务在 ${base}`);
  }
  const body = await r.json();
  // 后端统一包 { data: ... }，旧调用方依赖裸数组，这里两种都兼容。
  const list: ProcessTemplateSummary[] = body.data ?? body;
  const hit = list.find((p) => p.name === name);
  if (!hit) {
    throw new Error(`未找到系统工艺「${name}」，现有：${list.map((p) => p.name).join(', ')}`);
  }
  return `${base}/#/processes?processMode=edit&guid=${hit.guid}`;
}
