// useProcessPersistence.ts
// ---------------------------------------------------------------------------
// 096-W4-4-4：ProcessEditor 的「持久化」状态族 hook。
//
// 承接原主组件的保存/删除/返回/复制系统工艺四动作，连同 isSaving/isDeleting 加载态。
// 这组动作都调 bundledApi 写后端，且都需要回写编辑器状态（保存成功回刷 Monaco 文本、
// 保存/删除成功清未保存标记），故把编辑器状态的这几个出口以 deps 注入，避免两处 state 漂移。
//
// 设计取舍：
// - handleSave 用 yamlText（Monaco 当前文本）而非 definition：用户可能在 YAML tab
//   直接编辑未触发 debounced parseYaml 的中间态，yamlText 永远是最新文本（原注释逐字保留）。
// - handleDelete 弹 Modal.confirm 二次确认；title 优先显示名 → YAML name → GUID，
//   detail 必须进 deps，否则闭包捕获初始 null 永远显示 GUID（陈旧闭包，原踩坑结论）。
// - 保留静态 message/Modal（原实现即静态）。
// - 函数体超 50 行豁免（CLAUDE.md「强行拆分将导致数据碎片化」）：保存/删除/返回/复制
//   四动作共享 isSaving/isDeleting 加载态，且都经 deps 写回编辑器状态（markClean/
//   setYamlText）；拆成 4 个一次性子 hook 会把共享加载态与 deps 穿线打散。整段保留。
// ---------------------------------------------------------------------------

import { useCallback, useState } from 'react';
import { message, Modal } from 'antd';
import { bundledApi, type ProcessTemplateDetail } from '@/api/bundled';

/** 持久化动作回写编辑器状态所需的出口（由 useProcessEditorState 注入）。 */
export interface UseProcessPersistenceDeps {
  /** 工艺详情。handleDelete 确认框标题取其 display_name/name。 */
  detail: ProcessTemplateDetail | null;
  /** Monaco 当前文本。handleSave 以它为 PUT body。 */
  yamlText: string;
  /** 保存成功后回刷 Monaco（后端递增 version 后的真值 YAML）。 */
  setYamlText: (text: string) => void;
  /** 清未保存标记（保存/删除成功后调用）。 */
  markClean: () => void;
}

export interface UseProcessPersistenceReturn {
  isSaving: boolean;
  isDeleting: boolean;
  /** 保存回调（PUT /api/v1/processes/{guid}）。 */
  handleSave: () => Promise<void>;
  /** 删除回调（DELETE + Modal.confirm 二次确认）。 */
  handleDelete: () => void;
  /** 返回工艺列表页（仅置 hash，离开拦截由 useLeaveGuard 兜底）。 */
  handleBack: () => void;
  /** 复制系统工艺到用户层（成功后跳副本编辑器）。 */
  handleCopyToUser: () => Promise<void>;
}

export function useProcessPersistence(
  processGuid: string,
  deps: UseProcessPersistenceDeps,
): UseProcessPersistenceReturn {
  const { detail, yamlText, setYamlText, markClean } = deps;

  // 保存中状态，控制保存按钮禁用 + loading
  const [isSaving, setIsSaving] = useState(false);
  // 删除中状态，控制删除按钮禁用 + loading
  const [isDeleting, setIsDeleting] = useState(false);

  // ── 保存回调（PUT /api/v1/processes/{guid}）─────────
  // 用 yamlText（Monaco 当前文本）而非 definition，因为用户可能在 YAML tab
  // 直接编辑未触发 debounced parseYaml 的中间态。yamlText 永远是最新文本。
  const handleSave = useCallback(async () => {
    setIsSaving(true);
    try {
      const res = await bundledApi.putProcess(processGuid, yamlText);
      // 回刷 Monaco 为后端递增后的真值 YAML：避免陈旧 version 下次保存时
      // yaml_version != template.version 触发跳过 bump 并回写旧版本（需求 042 回归根因）。
      setYamlText(res.definition);
      message.success('工艺已保存');
      // 保存成功：清未保存标记，不跳路由不清表单（保留当前 definition + 节点位置）
      markClean();
    } catch (err) {
      // 兜底错误提示：后端 400（结构校验失败）/ 409（系统工艺）已在 message 反馈
      const msg = err instanceof Error ? err.message : String(err);
      message.error(`保存失败：${msg}`);
    } finally {
      setIsSaving(false);
    }
  }, [processGuid, yamlText, setYamlText, markClean]);

  // ── 删除回调（DELETE /api/v1/processes/{guid}）─────
  // 弹 Modal.confirm 二次确认，确认后调 DELETE，成功跳路由回列表页。
  const handleDelete = useCallback(() => {
    Modal.confirm({
      // NTD-014-D：优先显示工艺显示名（detail.display_name），
      // 其次 YAML name，最后才回退 GUID——原实现取 detail.name（恒为空）导致弹 GUID。
      // 注意：detail 必须进 deps——闭包若捕获初始 null，确认框永远显示 GUID（陈旧闭包）。
      title: `确认删除工艺「${detail?.display_name ?? detail?.name ?? processGuid}」？`,
      content: '此操作不可恢复。',
      okText: '删除',
      okType: 'danger',
      cancelText: '取消',
      onOk: async () => {
        setIsDeleting(true);
        try {
          await bundledApi.deleteProcess(processGuid);
          message.success('工艺已删除');
          // 先置 isDirty=false 避免离开拦截误触发（hashchange 监听在 useLeaveGuard）
          markClean();
          // 跳路由回列表页（hash 路由）
          window.location.hash = '#/processes';
        } catch (err) {
          const msg = err instanceof Error ? err.message : String(err);
          message.error(`删除失败：${msg}`);
          setIsDeleting(false);
        }
      },
    });
  }, [processGuid, detail, markClean]);

  // ── 返回工艺列表页 ─────────────────────────────────
  // 仅设置 location.hash 触发 hashchange；若 isDirty，离开拦截的 hashchange
  // 监听会弹「未保存修改」确认框，确认后才真正跳转，避免误丢改动。
  const handleBack = useCallback(() => {
    window.location.hash = '#/processes';
  }, []);

  // ── 复制系统工艺到用户层 ──────────────────────────
  // 成功后 message.success 提示用户重新打开编辑器
  // （M3 不自动刷新状态，避免复杂的状态重置逻辑；M5 会优化）
  const handleCopyToUser = useCallback(async () => {
    try {
      const copied = await bundledApi.copyProcessToUser(processGuid);
      // 040：副本是新 guid 的独立工艺，直接跳副本编辑器，不用手动重新打开。
      message.success('已复制为我的工艺，正在打开副本…');
      window.location.hash = `#/processes?processMode=edit&guid=${copied.guid}`;
    } catch {
      message.error('复制到用户层失败');
    }
  }, [processGuid]);

  return {
    isSaving,
    isDeleting,
    handleSave,
    handleDelete,
    handleBack,
    handleCopyToUser,
  };
}
