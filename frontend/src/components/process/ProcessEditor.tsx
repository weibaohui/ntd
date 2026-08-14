// ProcessEditor.tsx
// ---------------------------------------------------------------------------
// M4 里程碑：工艺编辑器主组件（096-W4-4-4 收口后为编排器）。
//
// 设计定位（对应 docs/design/029-M4-ReactFlow可视化编辑器-方案.md §3.1.11 + 方案 §2.8）：
// - 双栏布局：左 React Flow 可视化区 + 右属性面板；顶部 Toolbar + 系统 Alert；Tab 切可视化/YAML。
// - M5 双向联动：可视化 ↔ Monaco YAML 双向同步（isSyncing 防循环），保存/删除/离开拦截齐全。
//
// 096-W4-4-4 拆分后职责边界（原 704 行 → 编排器）：
// - 数据/双向联动/未保存标记 → useProcessEditorState
// - 保存/删除/返回/复制 → useProcessPersistence
// - 离开拦截（hashchange + beforeunload）→ useLeaveGuard
// - 可折叠属性面板 UI → CollapsiblePropertyPanel
// - 骨架样式 → processEditorStyles
// 本组件只保留：activeTab（纯视图态）+ 三 hook 编排 + 渲染分支组合。
//
// 数据流（M4 单向 + M5 双向联动）：
//   路由 processGuid → useProcessEditorState.bundledApi.getProcess → definition / yamlText
//   → 可视化区拖连线/改属性 → handleDefinitionChange → yamlDump 刷新 Monaco
//   → Monaco 编辑 → handleYamlChange（debounced）→ parseYaml 回写 definition
//   → 保存 → useProcessPersistence.handleSave（PUT yamlText）→ 回刷 Monaco + 清 dirty
// ---------------------------------------------------------------------------

import { useState, type JSX } from 'react';
import { Alert, Button, Empty, Spin } from 'antd';
import { CopyOutlined } from '@ant-design/icons';
import { useTheme } from '@/hooks/useTheme';
import { useProcessEditorState } from '@/hooks/useProcessEditorState';
import { useProcessPersistence } from '@/hooks/useProcessPersistence';
import { useLeaveGuard } from '@/hooks/useLeaveGuard';
import { ProcessYamlEditor } from './ProcessYamlEditor';
import { ProcessVisualEditor } from './ProcessVisualEditor';
import { ProcessEditorToolbar } from './ProcessEditorToolbar';
import { CollapsiblePropertyPanel } from './CollapsiblePropertyPanel';
import {
  loadingContainerStyle,
  editorContainerStyle,
  alertStyle,
  splitViewStyle,
  tabBarStyle,
  tabButtonStyle,
  tabButtonActiveStyle,
  tabsStyle,
  yamlEditorWrapperStyle,
  visualEditorStyle,
} from './processEditorStyles';

export interface ProcessEditorProps {
  // 工艺 guid（从路由参数取，作为 bundledApi.getProcess 等 API 的寻址参数）
  processGuid: string;
}

export function ProcessEditor({ processGuid }: ProcessEditorProps): JSX.Element {
  // 从 ThemeProvider 获取当前主题模式（传给 React Flow / Monaco）
  const { themeMode } = useTheme();

  // 当前激活的 Tab：'visual'（可视化双栏）| 'yaml'（Monaco YAML）。纯视图态，组件自管。
  const [activeTab, setActiveTab] = useState<'visual' | 'yaml'>('visual');

  // 编辑器数据 + 双向联动 + 未保存标记（加载/definition/yamlText/selectedNodeId/isDirty/handle*）。
  const editor = useProcessEditorState(processGuid);
  // 持久化动作（保存/删除/返回/复制），注入编辑器状态的回写出口。
  const persistence = useProcessPersistence(processGuid, {
    detail: editor.detail,
    yamlText: editor.yamlText,
    setYamlText: editor.setYamlText,
    markClean: editor.markClean,
  });
  // 离开拦截：isDirty 时弹确认/原生提示；确认离开前清标记防二次 hashchange。
  useLeaveGuard(editor.isDirty, editor.markClean);

  // ── 渲染分支 ──────────────────────────────────────

  // 加载中：显示 Spin
  if (editor.loading) {
    return (
      <div style={loadingContainerStyle}>
        <Spin tip="加载工艺..." />
      </div>
    );
  }

  // 加载完成但详情为空：工艺不存在或加载失败
  if (!editor.detail) {
    return (
      <div style={loadingContainerStyle}>
        <Empty description={`工艺「${processGuid}」不存在或加载失败`} />
      </div>
    );
  }

  // 主渲染：Toolbar + Alert + Tabs（可视化/YAML）
  return (
    <div style={editorContainerStyle}>
      {/* 顶部工具栏（保存/删除 + 未保存红点） */}
      <ProcessEditorToolbar
        processName={editor.detail.name}
        displayName={editor.detail.display_name}
        isSystem={editor.isSystem}
        isDirty={editor.isDirty}
        isSaving={persistence.isSaving}
        isDeleting={persistence.isDeleting}
        onSave={persistence.handleSave}
        onDelete={persistence.handleDelete}
        onBack={persistence.handleBack}
      />

      {/* 顶部 Alert：系统工艺黄色 + 复制链接 */}
      {editor.isSystem && (
        <Alert
          type="warning"
          showIcon
          message="这是系统工艺，编辑后会被同步覆盖"
          description={
            <Button
              type="link"
              icon={<CopyOutlined />}
              onClick={persistence.handleCopyToUser}
            >
              复制到用户层后编辑
            </Button>
          }
          style={alertStyle}
        />
      )}

      {/* Tab 切换可视化/YAML，支撑双向联动 */}
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
            {editor.definition ? (
              <ProcessVisualEditor
                definition={editor.definition}
                onDefinitionChange={editor.handleDefinitionChange}
                selectedNodeId={editor.selectedNodeId}
                onSelectNode={editor.setSelectedNodeId}
                theme={themeMode}
              />
            ) : (
              <Empty description="YAML 解析失败，无法显示可视化区" />
            )}
          </div>
          <CollapsiblePropertyPanel
            definition={editor.definition}
            selectedNodeId={editor.selectedNodeId}
            onDefinitionChange={editor.handleDefinitionChange}
          />
        </div>

        {/* YAML Tab：Monaco 编辑器，接入双向联动 */}
        <div style={{ ...yamlEditorWrapperStyle, display: activeTab === 'yaml' ? 'block' : 'none' }}>
          <ProcessYamlEditor
            value={editor.yamlText}
            onChange={editor.handleYamlChange}
            readOnly={editor.isSystem}
            theme={themeMode}
          />
        </div>
      </div>
    </div>
  );
}
