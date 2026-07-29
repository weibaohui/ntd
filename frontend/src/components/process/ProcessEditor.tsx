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
import { Alert, Button, Empty, Spin, Modal, message } from 'antd';
import { CopyOutlined, LeftOutlined, RightOutlined } from '@ant-design/icons';
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
  // 属性面板收缩状态：true = 收起为窄条（看大图时用），false = 正常 360px 面板
  const [propertyPanelCollapsed, setPropertyPanelCollapsed] = useState(false);

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

  // ── 返回工艺列表页 ─────────────────────────────────
  // 仅设置 location.hash 触发 hashchange；若 isDirty，离开拦截的 hashchange
  // 监听会弹「未保存修改」确认框，确认后才真正跳转，避免误丢改动。
  const handleBack = useCallback(() => {
    window.location.hash = '#/processes';
  }, []);

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
        onBack={handleBack}
      />

      {/* 顶部 Alert：系统工艺黄色 + 复制链接 */}
      {isSystem && (
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
      )}

      {/* M5：Tab 切换可视化/YAML，支撑双向联动 */}
      {/* 不用 Ant Design Tabs items API：其内部 .ant-tabs-tabpane 用 position:absolute + display:none 切换， */}
      {/* 导致 React Flow v12 的 ResizeObserver 拿到 0 尺寸，可视化区塌为 h:0 一片空白 */}
      {/* 改用手写两按钮 + display 切换：React Flow 始终挂载不卸载，父代明确 flex 链撑开 */}
      <div style={tabsStyle}>
        <div style={tabBarStyle}>
          <button
            type="button"
            style={activeTab === 'visual' ? tabButtonActiveStyle : tabButtonStyle}
            onClick={() => setActiveTab('visual')}
          >
            可视化
          </button>
          <button
            type="button"
            style={activeTab === 'yaml' ? tabButtonActiveStyle : tabButtonStyle}
            onClick={() => setActiveTab('yaml')}
          >
            YAML
          </button>
        </div>

        {/* 可视化 Tab：双栏布局（左可视化 + 右属性面板） */}
        {/* display 切换不卸载组件，React Flow 始终挂载；flex:1 撑满剩余高度 */}
        <div style={{ ...splitViewStyle, display: activeTab === 'visual' ? 'flex' : 'none' }}>
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
          <div style={propertyPanelStyle(propertyPanelCollapsed)}>
            {propertyPanelCollapsed ? (
              // 收起态：整条窄条都是一个展开按钮（用 AntD Button，可靠处理点击），
              // 纵向排版：顶部向左箭头、下方竖排标题。
              // 箭头方向与展开态的向右箭头形成「→ 收起 / ← 展开」的对称语义。
              <Button
                type="text"
                aria-label="展开属性面板"
                title="展开属性面板"
                style={expandStripButtonStyle}
                onClick={() => setPropertyPanelCollapsed(false)}
              >
                <div style={expandStripInnerStyle}>
                  {/* 顶部：向左箭头作为展开触发器（整条可点，箭头只是示意） */}
                  <LeftOutlined style={expandArrowStyle} />
                  {/* 底部：竖排标题，左靠齐（贴左边缘），让用户看清这是哪个面板 */}
                  <span style={expandTitleStyle}>
                    {getCollapsedPanelTitle(definition, selectedNodeId)}
                  </span>
                </div>
              </Button>
            ) : (
              // 展开态：顶部工具条（向右箭头收起）+ 面板内容
              <>
                <div style={panelToolbarStyle}>
                  {/* 工具条标题：与收起态窄条标题保持一致，左靠齐，
                      让用户一眼识别当前面板类型（工艺属性/阶段属性/环节属性）。 */}
                  <span style={panelToolbarTitleStyle}>
                    {getCollapsedPanelTitle(definition, selectedNodeId)}
                  </span>
                  <Button
                    type="text"
                    aria-label="收起属性面板"
                    title="收起属性面板"
                    style={collapseButtonStyle}
                    onClick={() => setPropertyPanelCollapsed(true)}
                  >
                    {/* 展开态用向右箭头：面板固定在右侧，点击后向右收缩成窄条，
                        箭头方向与面板退出的方向一致（常规语义）。 */}
                    <RightOutlined />
                  </Button>
                </div>
                <div style={panelBodyStyle}>
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
              </>
            )}
          </div>
        </div>

        {/* YAML Tab：Monaco 编辑器，接入双向联动 */}
        <div style={{ ...yamlEditorWrapperStyle, display: activeTab === 'yaml' ? 'block' : 'none' }}>
          <ProcessYamlEditor
            value={yamlText}
            onChange={handleYamlChange}
            readOnly={isSystem}
            theme={themeMode}
          />
        </div>
      </div>
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
// position:absolute + inset:0 绕过 Ant Design Tabs 内部 .ant-tabs-tabpane 的 position:absolute 链
// （tabpane 默认 absolute + 无明确高度，子代 height:100%/flex:1 全失效，React Flow 塌为 h:0）
// 撑满 Tabs 根（tabsStyle 有 position:relative + 明确高度）
const splitViewStyle: CSSProperties = {
  flexDirection: 'row',
  flex: 1,
  minHeight: 400,
};

// Tab 按钮栏：横向 flex，底部边框分隔（仿 Ant Design Tabs 视觉）
const tabBarStyle: CSSProperties = {
  display: 'flex',
  flexDirection: 'row',
  borderBottom: '1px solid #e2e8f0',
  padding: '0 4px',
};

// Tab 按钮：未激活态，浅色文字 + 透明背景
const tabButtonStyle: CSSProperties = {
  padding: '8px 16px',
  border: 'none',
  background: 'transparent',
  cursor: 'pointer',
  color: '#64748b',
  fontSize: 14,
};

// Tab 按钮：激活态，深色文字 + 底部蓝色高亮条（仿 Ant Design Tabs 激活态）
const tabButtonActiveStyle: CSSProperties = {
  ...tabButtonStyle,
  color: '#1677ff',
  fontWeight: 500,
  boxShadow: 'inset 0 -2px 0 #1677ff',
};

// Tabs 容器：填满剩余高度
// overflow:hidden 让 Tabs 内容区剪裁，nav 不溢出覆盖可视化区
// Ant Design Tabs 内容区默认无明确高度，需 flex:1 + minHeight 让内部 React Flow 撑开
// 父代 splitViewStyle 已 flex:1 撑开，这里 flex:1 + height:100% 雾路接通
const tabsStyle: CSSProperties = {
  flex: 1,
  display: 'flex',
  flexDirection: 'column',
  minHeight: 400,
  // 关键：让 Tabs 内容区（含 React Flow）正确剪裁，nav 不覆盖画布
  overflow: 'hidden',
};

// YAML 编辑器包装：撑满 Tab 内容区
// 同 splitViewStyle，用 absolute+inset:0 绕过 tabpane 链
const yamlEditorWrapperStyle: CSSProperties = {
  flex: 1,
  minHeight: 400,
};

// 左：可视化区，flex 1（占剩余宽度）
// 父代 splitViewStyle 已 absolute+inset:0 撑开，这里 flex:1 + height:100% 链路接通
const visualEditorStyle: CSSProperties = {
  flex: 1,
  minWidth: 400,
  height: '100%',
};

// 右：属性面板。展开固定 360px；收起为 32px 窄条（只留展开箭头）。
// 注意：不使用宽度过渡动画。过渡会让面板在 200ms 内逐步收窄，
// 期间窄条几何位置不稳定，点击容易落空（表现为「前几次点击失效」）。
// 直接切换宽度更可靠，点击区域始终稳定。
function propertyPanelStyle(collapsed: boolean): CSSProperties {
  return {
    width: collapsed ? 32 : 360,
    height: '100%',
    // 纵向 flex：展开时工具条在上、内容区撑满剩余
    display: 'flex',
    flexDirection: 'column',
    // 浅灰背景，与可视化区区分
    background: '#f8fafc',
    // 左边框分隔
    borderLeft: '1px solid #e2e8f0',
    // 收起态内容（窄条按钮）不允许溢出
    overflow: 'hidden',
  };
}

// 面板顶部工具条：左侧标题、右侧收起按钮，两端对齐。
// 标题与收起态窄条标题一致，保持展开/收起两种形态名称统一。
const panelToolbarStyle: CSSProperties = {
  display: 'flex',
  justifyContent: 'space-between',
  alignItems: 'center',
  padding: '8px 12px',
  borderBottom: '1px solid #e2e8f0',
  // 工具条不参与收缩，固定高度由内容决定
  flexShrink: 0,
};

// 工具条标题：与表单内原有大标题同级字号、加粗，但不再占表单垂直空间。
const panelToolbarTitleStyle: CSSProperties = {
  fontSize: 16,
  fontWeight: 600,
  color: '#334155',
  lineHeight: 1.4,
};

// 面板内容区：撑满工具条之外的剩余高度，滚动只发生在内容区
const panelBodyStyle: CSSProperties = {
  flex: 1,
  overflow: 'auto',
};

// 收起按钮（AntD Button type=text）：小号透明按钮，hover 由 AntD 处理。
// 覆盖默认阴影/最小高度，保持工具条极简、与标题两端对齐。
const collapseButtonStyle: CSSProperties = {
  background: 'transparent',
  border: 'none',
  boxShadow: 'none',
  color: '#64748b',
  fontSize: 12,
  cursor: 'pointer',
  height: 'auto',
  padding: '2px 6px',
  lineHeight: 1.4,
};

// 收起态的整条展开按钮（AntD Button type=text）：铺满 32px 窄条全高，
// 整条可点让用户不用瞄准小图标。覆盖 AntD 默认内边距/最小高度/阴影，
// 确保按钮占满窄条且不被默认样式挤压。
const expandStripButtonStyle: CSSProperties = {
  width: '100%',
  height: '100%',
  padding: 0,
  background: 'transparent',
  border: 'none',
  boxShadow: 'none',
  color: '#64748b',
  fontSize: 12,
  cursor: 'pointer',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
};

// 收起态内部纵向容器：箭头贴顶、竖排标题贴底，两端分布。
// 这样在 32px 窄条里也能清晰呈现「标题 + 箭头」的收缩栏形态。
// 内层容器不拦截鼠标，点击由外层 AntD Button 统一处理（Button 原生会正确
// 把内部图标/文字的点击冒泡到自身，无需 pointer-events hack）。
const expandStripInnerStyle: CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  alignItems: 'center',
  justifyContent: 'space-between',
  width: '100%',
  height: '100%',
  padding: '10px 0',
};

// 收缩栏箭头：小号、低调颜色，hover 由按钮整体承接交互。
const expandArrowStyle: CSSProperties = {
  fontSize: 12,
  color: '#64748b',
};

// 收缩栏标题：竖排（writing-mode vertical-rl）以便在中文字符下自然竖读，
// 贴左边缘、不换行。窄条宽度有限，竖排是唯一可读的呈现方式。
const expandTitleStyle: CSSProperties = {
  writingMode: 'vertical-rl',
  textOrientation: 'upright',
  letterSpacing: 2,
  fontSize: 13,
  color: '#334155',
  whiteSpace: 'nowrap',
};

// 收起态标题解析：与 ProcessPropertyPanel 的表单路由保持一致，
// 让收缩栏显示的名称就是展开后表单头部的名称（工艺属性/阶段属性/环节属性）。
// definition 为 null（YAML 未解析）时兜底为工艺属性。
function getCollapsedPanelTitle(
  definition: ProcessDefinition | null,
  selectedNodeId: string | null,
): string {
  // 未选中任何节点 → 全局面板，对应「工艺属性」
  if (selectedNodeId === null || definition === null) return '工艺属性';
  // 命中 phase → 「阶段属性」
  if (definition.phases?.some((p) => p.id === selectedNodeId)) return '阶段属性';
  // 命中任一 phase 下的 link → 「环节属性」
  for (const p of definition.phases ?? []) {
    if ((p.links ?? []).some((l) => l.id === selectedNodeId)) return '环节属性';
  }
  // 悬空引用（节点已删但 selectedNodeId 未清）→ 兜底工艺属性
  return '工艺属性';
}
