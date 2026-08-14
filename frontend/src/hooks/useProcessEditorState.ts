// useProcessEditorState.ts
// ---------------------------------------------------------------------------
// 096-W4-4-4：ProcessEditor 的「加载 + 可视化数据 + M5 双向联动」状态族 hook。
//
// 承接原主组件的三块职责：
//  1. 加载族——detail/loading/isSystem，按 processGuid 拉取工艺详情并解析 YAML。
//  2. 可视化数据族——definition（React Flow source of truth）/ yamlText（Monaco 文本）
//     / selectedNodeId（当前选中节点）。
//  3. M5 双向联动——isSyncing 防循环 flag / isDirty 未保存标记 / debounceRef，
//     以及 handleDefinitionChange（可视化→YAML）与 handleYamlChange（YAML→可视化）。
//
// 设计取舍（行为零变化优先）：
// - 双向联动的 isSyncing 防循环 + debounce 300ms 语义逐字搬移，不改时序。
// - 保留静态 message（原实现即静态；refactor 不转 App.useApp，避免引入行为差异）。
// - setYamlText / markClean 对外暴露：前者供 useProcessPersistence 保存成功后回刷
//   Monaco（后端递增 version 的真值 YAML），后者供其清未保存标记。两个写出口都属
//   「持久化回路写回编辑器状态」，故集中暴露而非各自塞回调。
// - 函数体超 50 行豁免（CLAUDE.md「强行拆分将导致数据碎片化」）：definition/yamlText/
//   isSyncing/isDirty/debounceRef 构成原子双向联动状态机——isSyncing 防循环 flag 须被
//   两条联动路径共享，拆成 useEditorData + useEditorSync 会在两个 hook 间穿线 5+ setter
//   并复制 flag，正是规范明令禁止的碎片化拆分动机。故整段保留；单一抽象层级（编辑器
//   状态机），非 SQL/JSON/IO 多业务层混居的红线。
// ---------------------------------------------------------------------------

import { useCallback, useEffect, useRef, useState } from 'react';
import { message } from 'antd';
import { bundledApi, type ProcessTemplateDetail } from '@/api/bundled';
import type { ProcessDefinition } from '@/types/process';
import { parseYaml, yamlDump } from '@/components/process/processYamlValidator';

export interface UseProcessEditorStateReturn {
  // ── 加载族 ──
  detail: ProcessTemplateDetail | null;
  loading: boolean;
  isSystem: boolean;

  // ── 可视化数据族 ──
  definition: ProcessDefinition | null;
  /** Monaco 当前文本。yamlText 永远是最新文本（含未触发 debounced parse 的中间态）。 */
  yamlText: string;
  /** 保存成功后回刷 Monaco：用后端递增 version 后的真值 YAML 覆盖本地，避免陈旧 version 下次保存跳过 bump。 */
  setYamlText: (text: string) => void;
  /** 当前选中节点 YAML id（null = 全局面板）。 */
  selectedNodeId: string | null;
  /** ProcessVisualEditor 选中节点时回写。 */
  setSelectedNodeId: (id: string | null) => void;

  // ── M5 双向联动族 ──
  /** 未保存修改标记（任一字段改动置 true，保存成功置 false）。useLeaveGuard 读它做拦截。 */
  isDirty: boolean;
  /** 清未保存标记（保存/删除成功后调用）。 */
  markClean: () => void;
  /** 可视化操作回调（可视化 → YAML 路径）。 */
  handleDefinitionChange: (newDefinition: ProcessDefinition) => void;
  /** Monaco 编辑回调（YAML → 可视化 路径，debounced 300ms）。 */
  handleYamlChange: (newYaml: string) => void;
}

export function useProcessEditorState(processGuid: string): UseProcessEditorStateReturn {
  // ── 加载族 ──
  const [detail, setDetail] = useState<ProcessTemplateDetail | null>(null);
  const [loading, setLoading] = useState(true);
  // yamlText 驱动 Monaco，受双向联动影响（可视化操作 → yamlDump 刷新）
  const [yamlText, setYamlText] = useState('');
  const [isSystem, setIsSystem] = useState(false);

  // ── 可视化数据族 ──
  // 工艺定义对象（React Flow 的 source of truth）
  const [definition, setDefinition] = useState<ProcessDefinition | null>(null);
  // 当前选中的节点 YAML id（null = 全局面板）
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);

  // ── M5 双向联动族 ──
  // 双向联动防循环 flag：可视化→YAML 或 YAML→可视化 推送期间置 true，
  // 让对端的 onChange 在 flag 期间忽略，推完后清 flag（setTimeout 0）。
  const [isSyncing, setIsSyncing] = useState(false);
  // 未保存修改标记，离开拦截用（用户改了任何字段就置 true，保存成功后置回 false）
  const [isDirty, setIsDirty] = useState(false);
  // debounced parseYaml 的 timer ref，避免快速编辑触发多次解析
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  /** 清未保存标记（保存/删除成功后调用）。 */
  const markClean = useCallback(() => setIsDirty(false), []);

  // ── 加载工艺详情副作用 ──
  // processGuid 变化时全量重载：先清旧数据避免切换工艺时闪现上一个工艺的内容，
  // 再拉详情并解析 YAML。解析失败 definition 保持 null（渲染层走 Empty 占位），
  // 加载/解析失败都只 message 提示不抛错打断。
  useEffect(() => {
    setLoading(true);
    // 清空旧数据，避免切换工艺时闪现上一个工艺的内容
    setDetail(null);
    setYamlText('');
    setIsSystem(false);
    setDefinition(null);
    setSelectedNodeId(null);

    const loadDetail = async () => {
      try {
        const result = await bundledApi.getProcess(processGuid);
        setDetail(result);
        setYamlText(result.definition);
        setIsSystem(result.is_system);
        const parsed = parseYaml(result.definition);
        if (parsed.parsed && typeof parsed.parsed === 'object') {
          setDefinition(parsed.parsed as ProcessDefinition);
        } else if (parsed.error) {
          // 解析失败：definition 保持 null，渲染层显示 Empty 占位
          message.error(`YAML 解析失败：${parsed.error.message}`);
        }
      } catch {
        message.error(`加载工艺「${processGuid}」失败`);
      } finally {
        setLoading(false);
      }
    };

    void loadDetail();
  }, [processGuid]);

  // ── 可视化操作回调（可视化 → YAML 路径）─────
  // 可视化区 / 属性面板修改 definition 后：
  // 1. setDefinition 更新 source of truth
  // 2. setIsDirty(true) 标记未保存
  // 3. setIsSyncing(true) 防 Monaco onChange 循环
  // 4. yamlDump 刷新 Monaco 文本
  // 5. setTimeout 0 清 flag（让 Monaco 先处理 onChange）
  const handleDefinitionChange = useCallback((newDefinition: ProcessDefinition) => {
    setDefinition(newDefinition);
    setIsDirty(true);
    // 防 Monaco onChange 循环：推送期间置 flag
    setIsSyncing(true);
    // 可视化 → YAML：dump 刷新 Monaco
    const newYaml = yamlDump(newDefinition);
    setYamlText(newYaml);
    // 推完后清 flag（setTimeout 0 让 Monaco 先处理本轮 onChange）
    setTimeout(() => setIsSyncing(false), 0);
  }, []);

  // ── Monaco 编辑回调（YAML → 可视化 路径）─────
  // Monaco onChange 触发：
  // 1. setYamlText 更新受控文本
  // 2. setIsDirty(true) 标记未保存
  // 3. if (isSyncing) return — flag 期间忽略（可视化触发的刷新）
  // 4. debounced 300ms parseYaml：成功更新 definition，失败只标红不破坏可视化
  const handleYamlChange = useCallback(
    (newYaml: string) => {
      setYamlText(newYaml);
      setIsDirty(true);
      // flag 期间忽略：这是可视化触发的刷新，避免循环
      if (isSyncing) return;
      // 清上一个 debounce timer，避免快速编辑触发多次解析
      if (debounceRef.current) clearTimeout(debounceRef.current);
      // debounced 300ms 后解析 YAML
      debounceRef.current = setTimeout(() => {
        const parsed = parseYaml(newYaml);
        if (parsed.parsed && typeof parsed.parsed === 'object') {
          // 解析成功：更新 definition，React Flow 重渲染
          // 防 React Flow onNodesChange 循环：推送期间置 flag
          setIsSyncing(true);
          setDefinition(parsed.parsed as ProcessDefinition);
          setTimeout(() => setIsSyncing(false), 0);
        }
        // 解析失败：Monaco 标红（ProcessYamlEditor 内部处理），不破坏可视化
      }, 300);
    },
    [isSyncing],
  );

  return {
    detail,
    loading,
    isSystem,
    definition,
    yamlText,
    setYamlText,
    selectedNodeId,
    setSelectedNodeId,
    isDirty,
    markClean,
    handleDefinitionChange,
    handleYamlChange,
  };
}
