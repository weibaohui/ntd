// ProcessEditor.tsx
// ---------------------------------------------------------------------------
// M4 里程碑：工艺编辑器主组件（M4 双栏布局版本）。
//
// 设计定位（对应 docs/design/029-M4-ReactFlow可视化编辑器-方案.md §3.1.11 + 方案 §2.8）：
// - M3 骨架扩展为双栏布局：左 React Flow 可视化区 + 右属性面板。
// - M4 只做"可视化操作 → ProcessDefinition 不可变更新"，不回写 Monaco（M5 sync flag）。
// - M5 会在顶部加保存/删除按钮 + 双向联动 sync flag。
// - M6 会加新建工艺元信息 Modal + 空工艺渲染。
//
// 数据流（M4 单向，可视化 → ProcessDefinition）：
//   路由 processName → bundledApi.getProcess → detail.definition / is_system
//   → parseYaml(result.definition) → setDefinition(parsed)
//   → ProcessVisualEditor 渲染 React Flow
//   → 用户拖连线 / 删节点 → onDefinitionChange(newDefinition)
//   → ProcessPropertyPanel 渲染属性表单
//   → 用户改字段 → onDefinitionChange(newDefinition)
//
// 注意：M4 不回写 Monaco（ProcessYamlEditor 的 value 仍是初始 yamlText）。
// M5 会加 isSyncing flag 实现 YAML ↔ 可视化双向联动。
//
// 非目标（留给后续里程碑）：
// - M5：保存按钮（PUT）、删除按钮（DELETE）、双向联动 sync flag、离开拦截
// - M6：新建工艺元信息 Modal、空工艺渲染
// ---------------------------------------------------------------------------

import {
  useEffect,
  useState,
  useCallback,
  useRef,
  type CSSProperties,
  type JSX,
} from 'react';
import { Alert, Button, Empty, Spin, Tabs, Modal, message } from 'antd';
import { CopyOutlined } from '@ant-design/icons';
import {
  bundledApi,
  type ProcessTemplateDetail,
} from '@/api/bundled';
import type { ProcessDefinition } from '@/types/process';
import { useTheme } from '@/hooks/useTheme';
import { ProcessYamlEditor } from './ProcessYamlEditor';
import { ProcessVisualEditor } from './ProcessVisualEditor';
import { ProcessPropertyPanel } from './ProcessPropertyPanel';
import { ProcessEditorToolbar } from './ProcessEditorToolbar';
import { parseYaml, yamlDump } from './processYamlValidator';

export interface ProcessEditorProps {
  // 工艺名（从路由参数取，作为 bundledApi.getProcess 的参数）
  processName: string;
}

// ProcessEditor 组件实现。
//
// 状态（M4 扩展集）：
// - detail：工艺详情（M3 已有）
// - loading：加载中标志（M3 已有）
// - yamlText：Monaco 编辑器内的 YAML 文本（M3 已有，M4 不再驱动）
// - isSystem：是否系统工艺（M3 已有）
// - definition：工艺定义对象（M4 新增，React Flow 的 source of truth）
// - selectedNodeId：当前选中的节点 YAML id（M4 新增，属性面板切换用）
export function ProcessEditor({ processName }: ProcessEditorProps): JSX.Element {
  // 从 ThemeProvider 获取当前主题模式
  const { themeMode } = useTheme();

  // ── M3 已有状态 ──────────────────────────────────
  const [detail, setDetail] = useState<ProcessTemplateDetail | null>(null);
  const [loading, setLoading] = useState(true);
  // yamlText 驱动 Monaco，受双向联动影响（可视化操作 → yamlDump 刷新）
  const [yamlText, setYamlText] = useState('');
  const [isSystem, setIsSystem] = useState(false);

  // ── M4 已有状态 ──────────────────────────────────
  // 工艺定义对象（React Flow 的 source of truth）
  const [definition, setDefinition] = useState<ProcessDefinition | null>(null);
  // 当前选中的节点 YAML id（null = 全局面板）
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);

  // ── M5 新增状态 ──────────────────────────────────
  // 双向联动防循环 flag：可视化→YAML 或 YAML→可视化 推送期间置 true，
  // 让对端的 onChange 在 flag 期间忽略，推完后清 flag（setTimeout 0）
  const [isSyncing, setIsSyncing] = useState(false);
  // 未保存修改标记，离开拦截用（用户改了任何字段就置 true，保存成功后置回 false）
  // M5 双向联动已 setIsDirty(true)，保存按钮/离开拦截在任务 #6/#7 接入读取
  const [isDirty, setIsDirty] = useState(false);
  void isDirty;
  // 保存中状态，控制保存按钮禁用 + loading
  const [isSaving, setIsSaving] = useState(false);
  // 删除中状态，控制删除按钮禁用 + loading
  const [isDeleting, setIsDeleting] = useState(false);
  // debounced parseYaml 的 timer ref，避免快速编辑触发多次解析
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // 当前激活的 Tab：'visual'（可视化双栏）| 'yaml'（Monaco YAML）
  const [activeTab, setActiveTab] = useState<'visual' | 'yaml'>('visual');

  // ── 加载工艺详情副作用 ──────────────────────────
  // 依赖 [processName]：工艺名变化时重新加载
  useEffect(() => {
    // 标记加载开始
    setLoading(true);
    // 清空旧数据，避免切换工艺时闪现上一个工艺的内容
    setDetail(null);
    setYamlText('');
    setIsSystem(false);
    setDefinition(null);
    setSelectedNodeId(null);

    // 定义异步加载函数
    const loadDetail = async () => {
      try {
        // 调用 M1 已有的 API 客户端获取工艺详情
        const result = await bundledApi.getProcess(processName);
        // 设置详情和 Monaco 文本
        setDetail(result);
        setYamlText(result.definition);
        setIsSystem(result.is_system);
        // M4 新增：解析 YAML 文本为 ProcessDefinition 对象
        // 用 parseYaml 纯函数解析，成功后 setDefinition
        const parsed = parseYaml(result.definition);
        if (parsed.parsed && typeof parsed.parsed === 'object') {
          setDefinition(parsed.parsed as ProcessDefinition);
        } else if (parsed.error) {
          // YAML 解析失败：提示错误，definition 保持 null
          // M5 会在这里加"YAML 错误时可视化区显示错误提示"逻辑
          message.error(`YAML 解析失败：${parsed.error.message}`);
        }
      } catch {
        // 加载失败时 message.error 提示
        message.error(`加载工艺「${processName}」失败`);
      } finally {
        // 无论成功失败都关闭 loading
        setLoading(false);
      }
    };

    // 触发异步加载
    void loadDetail();
  }, [processName]);

  // ── 复制系统工艺到用户层 ──────────────────────────
  // 成功后 message.success 提示用户重新打开编辑器
  // （M3 不自动刷新状态，避免复杂的状态重置逻辑；M5 会优化）
  const handleCopyToUser = async () => {
    try {
      await bundledApi.copyProcessToUser(processName);
      message.success(`已复制到用户层，请重新打开编辑器`);
    } catch {
      message.error('复制到用户层失败');
    }
  };

  // ── M4/M5：可视化操作回调（可视化 → YAML 路径）─────
  // 可视化区 / 属性面板修改 definition 后：
  // 1. setDefinition 更新 source of truth
  // 2. setIsDirty(true) 标记未保存
  // 3. setIsSyncing(true) 防 Monaco onChange 循环
  // 4. yamlDump 刷新 Monaco 文本
  // 5. setTimeout 0 清 flag（让 Monaco 先处理 onChange）
  const handleDefinitionChange = useCallback(
    (newDefinition: ProcessDefinition) => {
      setDefinition(newDefinition);
      setIsDirty(true);
      // 防 Monaco onChange 循环：推送期间置 flag
      setIsSyncing(true);
      // 可视化 → YAML：dump 刷新 Monaco
      const newYaml = yamlDump(newDefinition);
      setYamlText(newYaml);
      // 推完后清 flag（setTimeout 0 让 Monaco 先处理本轮 onChange）
      setTimeout(() => setIsSyncing(false), 0);
    },
    [],
  );

  // ── M5：Monaco 编辑回调（YAML → 可视化 路径）─────
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

  // ── M5：保存回调（PUT /api/v1/processes/{name}）─────────
  // 用 yamlText（Monaco 当前文本）而非 definition，因为用户可能在 YAML tab
  // 直接编辑未触发 debounced parseYaml 的中间态。yamlText 永远是最新文本。
  const handleSave = useCallback(async () => {
    setIsSaving(true);
    try {
      await bundledApi.putProcess(processName, yamlText);
      message.success('工艺已保存');
      // 保存成功：清未保存标记，不跳路由不清表单（保留当前 definition + 节点位置）
      setIsDirty(false);
    } catch (err) {
      // 兜底错误提示：后端 400（结构校验失败）/ 409（系统工艺）已在 message 反馈
      const msg = err instanceof Error ? err.message : String(err);
      message.error(`保存失败：${msg}`);
    } finally {
      setIsSaving(false);
    }
  }, [processName, yamlText]);

  // ── M5：删除回调（DELETE /api/v1/processes/{name}）─────
  // 弹 Modal.confirm 二次确认，确认后调 DELETE，成功跳路由回列表页
  const handleDelete = useCallback(() => {
    Modal.confirm({
      title: `确认删除工艺「${processName}」？`,
      content: '此操作不可恢复。',
      okText: '删除',
      okType: 'danger',
      cancelText: '取消',
      onOk: async () => {
        setIsDeleting(true);
        try {
          await bundledApi.deleteProcess(processName);
          message.success('工艺已删除');
          // 先置 isDirty=false 避免离开拦截误触发（hashchange 监听在任务 #7）
          setIsDirty(false);
          // 跳路由回列表页（hash 路由）
          window.location.hash = '#/processes';
        } catch (err) {
          const msg = err instanceof Error ? err.message : String(err);
          message.error(`删除失败：${msg}`);
          setIsDeleting(false);
        }
      },
    });
  }, [processName]);

  // ── M5：离开拦截（需求 §3.6）──────────────────────
  // 两层拦截，共享 isDirty 状态（用户改了任何字段就置 true，保存成功后置回 false）。
  //
  // 1. 路由内跳转（hash 路由）：监听 hashchange，isDirty 时弹 Modal.confirm 呝止，
  //    用户确认后跳目标路由，取消则用 history.replaceState 回退旧 hash（避免再触发 hashchange）。
  // 2. 刷新/关页签：window.beforeunload，isDirty 时 e.preventDefault() + returnValue=''，
  //    浏览器原生提示（Chrome/Firefox/Safari 统一行为）。
  //
  // 注意：项目无 react-router-dom（用 hash 路由 + 自研 useViewState），
  // 故 React Router 的 useBlocker 不可用，改用 window 层 hashchange 监听。
  useEffect(() => {
    // 路由内跳转拦截：hashchange 监听
    const handleHashChange = (e: HashChangeEvent) => {
      // 仅在 isDirty 时拦截；非 dirty 放行
      if (!isDirty) return;
      // 雹 Modal.confirm 呝止跳转
      Modal.confirm({
        title: '你有未保存的修改',
        content: '确认离开？未保存的修改将丢失。',
        okText: '离开',
        cancelText: '留下',
        onOk: () => {
          // 用户确认后允许跳转：先置 isDirty=false 避免目标路由二次拦截
          setIsDirty(false);
          // 跳目标 hash（e.newURL 已含完整 URL，取 # 后部分）
          const newHash = e.newURL.split('#')[1] ?? '';
          window.location.hash = newHash;
        },
        onCancel: () => {
          // 取消时不跳转：hashchange 已触发，需回退旧 hash
          // 用 history.replaceState 避免再触发 hashchange（直接设 location.hash 会再触发）
          history.replaceState(null, '', e.oldURL);
        },
      });
    };
    window.addEventListener('hashchange', handleHashChange);

    // 刷新/关页签拦截：beforeunload 监听
    const handleBeforeUnload = (e: BeforeUnloadEvent) => {
      if (!isDirty) return;
      // 标准行为：preventDefault + returnValue='' 触发浏览器原生提示
      e.preventDefault();
      e.returnValue = '';
    };
    window.addEventListener('beforeunload', handleBeforeUnload);

    // 清理：组件卸载或 isDirty 变化时移除监听
    return () => {
      window.removeEventListener('hashchange', handleHashChange);
      window.removeEventListener('beforeunload', handleBeforeUnload);
    };
  }, [isDirty]);

  // ── 渲染分支 ──────────────────────────────────────

  // 加载中：显示 Spin
  if (loading) {
    return (
      <div style={loadingContainerStyle}>
        <Spin tip="加载工艺..." />
      </div>
    );
  }

  // 加载完成但详情为空：工艺不存在或加载失败
  if (!detail) {
    return (
      <div style={loadingContainerStyle}>
        <Empty description={`工艺「${processName}」不存在或加载失败`} />
      </div>
    );
  }

  // 主渲染：Toolbar + Alert + Tabs（可视化/YAML）
  return (
    <div style={editorContainerStyle}>
      {/* M5：顶部工具栏（保存/删除 + 未保存红点） */}
      <ProcessEditorToolbar
        processName={processName}
        displayName={detail.display_name}
        isSystem={isSystem}
        isDirty={isDirty}
        isSaving={isSaving}
        isDeleting={isDeleting}
        onSave={handleSave}
        onDelete={handleDelete}
      />

      {/* 顶部 Alert：系统工艺黄色 + 复制链接，用户工艺绿色 */}
      {isSystem ? (
        <Alert
          type="warning"
          showIcon
          message="这是系统工艺，编辑后会被同步覆盖"
          description={
            <Button
              type="link"
              icon={<CopyOutlined />}
              onClick={handleCopyToUser}
            >
              复制到用户层后编辑
            </Button>
          }
          style={alertStyle}
        />
      ) : (
        <Alert
          type="success"
          showIcon
          message="编辑保存后写入 ~/.ntd/processes/（M5 将启用保存按钮）"
          style={alertStyle}
        />
      )}

      {/* M5：Tabs 切换可视化/YAML，支撑双向联动 */}
      <Tabs
        activeKey={activeTab}
        onChange={(key) => setActiveTab(key as 'visual' | 'yaml')}
        style={tabsStyle}
        items={[
          {
            key: 'visual',
            label: '可视化',
            children: (
              /* 双栏布局：左可视化 + 右属性面板 */
              <div style={splitViewStyle}>
                {/* 左：React Flow 可视化区 */}
                <div style={visualEditorStyle}>
                  {definition ? (
                    <ProcessVisualEditor
                      definition={definition}
                      onDefinitionChange={handleDefinitionChange}
                      selectedNodeId={selectedNodeId}
                      onSelectNode={setSelectedNodeId}
                      theme={themeMode}
                    />
                  ) : (
                    <Empty description="YAML 解析失败，无法显示可视化区" />
                  )}
                </div>

                {/* 右：属性面板 */}
                <div style={propertyPanelStyle}>
                  {definition ? (
                    <ProcessPropertyPanel
                      definition={definition}
                      selectedNodeId={selectedNodeId}
                      onDefinitionChange={handleDefinitionChange}
                    />
                  ) : (
                    <Empty description="YAML 解析失败，无法显示属性面板" />
                  )}
                </div>
              </div>
            ),
          },
          {
            key: 'yaml',
            label: 'YAML',
            children: (
              /* M5：Monaco YAML 编辑器，接入双向联动 */
              <div style={yamlEditorWrapperStyle}>
                <ProcessYamlEditor
                  value={yamlText}
                  onChange={handleYamlChange}
                  readOnly={isSystem}
                  theme={themeMode}
                />
              </div>
            ),
          },
        ]}
      />
    </div>
  );
}

// ── 样式常量 ──────────────────────────────────────────

// 加载/空状态容器：居中
const loadingContainerStyle: CSSProperties = {
  display: 'flex',
  justifyContent: 'center',
  alignItems: 'center',
  height: '100%',
  minHeight: 300,
};

// 编辑器主容器：纵向 flex，Alert 固定高度，双栏区填满剩余
const editorContainerStyle: CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  height: '100%',
  width: '100%',
};

// Alert 样式：底部留间距
const alertStyle: CSSProperties = {
  marginBottom: 12,
};

// 双栏布局：横向 flex
const splitViewStyle: CSSProperties = {
  display: 'flex',
  flexDirection: 'row',
  flex: 1,
  minHeight: 400,
};

// Tabs 容器：填满剩余高度
const tabsStyle: CSSProperties = {
  flex: 1,
  display: 'flex',
  flexDirection: 'column',
  minHeight: 400,
};

// YAML 编辑器包装：撑满 Tab 内容区
const yamlEditorWrapperStyle: CSSProperties = {
  height: '100%',
  minHeight: 400,
};

// 左：可视化区，flex 1（占剩余宽度）
const visualEditorStyle: CSSProperties = {
  flex: 1,
  minWidth: 400,
  // React Flow 需要明确高度
  height: '100%',
};

// 右：属性面板，固定宽度 360px
const propertyPanelStyle: CSSProperties = {
  width: 360,
  height: '100%',
  overflow: 'auto',
  // 浅灰背景，与可视化区区分
  background: '#f8fafc',
  // 左边框分隔
  borderLeft: '1px solid #e2e8f0',
};
