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
  type CSSProperties,
  type JSX,
} from 'react';
import { Alert, Button, Empty, Spin, message } from 'antd';
import { CopyOutlined } from '@ant-design/icons';
import {
  bundledApi,
  type ProcessTemplateDetail,
} from '@/api/bundled';
import type { ProcessDefinition } from '@/types/process';
import { useTheme } from '@/hooks/useTheme';
import { ProcessVisualEditor } from './ProcessVisualEditor';
import { ProcessPropertyPanel } from './ProcessPropertyPanel';
import { parseYaml } from './processYamlValidator';

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
  // yamlText 在 M3 驱动 Monaco，M4 改用 definition 驱动可视化；
  // 保留 setter 以便 M5 双向联动时回写。
  const [yamlText, setYamlText] = useState('');
  void yamlText;
  const [isSystem, setIsSystem] = useState(false);

  // ── M4 新增状态 ──────────────────────────────────
  // 工艺定义对象（React Flow 的 source of truth）
  const [definition, setDefinition] = useState<ProcessDefinition | null>(null);
  // 当前选中的节点 YAML id（null = 全局面板）
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);

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

  // ── M4 新增：可视化操作回调 ──────────────────────
  // 可视化区 / 属性面板修改 definition 后，通过此回调更新父组件状态
  // M5 会在这里加 yaml.dump 刷新 Monaco（双向联动）
  const handleDefinitionChange = useCallback(
    (newDefinition: ProcessDefinition) => {
      setDefinition(newDefinition);
      // M5 将添加：setYamlText(yaml.dump(newDefinition))
      // M5 将添加：setIsDirty(true)
    },
    [],
  );

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

  // 主渲染：Alert + 双栏布局（左可视化 + 右属性面板）
  return (
    <div style={editorContainerStyle}>
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

      {/* 双栏布局：左可视化 + 右属性面板 */}
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

      {/* M3 的 ProcessYamlEditor 在 M5 会用 Tabs 切换可视化/YAML */}
      {/* M4 暂时不渲染 Monaco，避免 yamlText 与 definition 脱节导致混乱 */}
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
