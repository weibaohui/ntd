// ProcessEditor.tsx
// ---------------------------------------------------------------------------
// M3 里程碑：工艺编辑器主组件（M3 骨架版本）。
//
// 设计定位（对应 docs/design/029-M3-Monaco-YAML编辑器-方案.md §3.1.3）：
// - M3 只实现"加载工艺 → 渲染 Alert → 渲染 Monaco 编辑器"骨架。
// - M4 会在右侧加 React Flow 可视化区。
// - M5 会在顶部加保存/删除按钮 + 双向联动 sync flag。
// - M6 会加新建工艺元信息 Modal。
//
// 数据流（M3 单向）：
//   路由 processName → bundledApi.getProcess → detail.definition / is_system
//   → setYamlText / setIsSystem → ProcessYamlEditor 渲染
//   → 用户编辑 → onChange → setYamlText（M5 会加保存按钮）
//
// 非目标（留给后续里程碑）：
// - M4：React Flow 可视化、属性面板
// - M5：保存按钮（PUT）、删除按钮（DELETE）、双向联动、离开拦截
// - M6：新建工艺元信息 Modal、空工艺渲染
// ---------------------------------------------------------------------------

import { useEffect, useState, type CSSProperties, type JSX } from 'react';
import { Alert, Button, Empty, Spin, message } from 'antd';
import { CopyOutlined } from '@ant-design/icons';
import {
  bundledApi,
  type ProcessTemplateDetail,
} from '@/api/bundled';
import { useTheme } from '@/hooks/useTheme';
import { ProcessYamlEditor } from './ProcessYamlEditor';

export interface ProcessEditorProps {
  // 工艺名（从路由参数取，作为 bundledApi.getProcess 的参数）
  processName: string;
}

// ProcessEditor 组件实现。
//
// 状态（M3 最小集）：
// - detail：工艺详情，含 definition YAML 文本和 is_system 标记
// - loading：加载中标志，控制 Spin 显示
// - yamlText：当前 Monaco 编辑器内的 YAML 文本（受控）
// - isSystem：是否系统工艺，决定 readOnly 和 Alert 颜色
//
// 副作用：
// - mount 或 processName 变化时加载工艺详情
// - 复制到用户层后 message.success 提示
export function ProcessEditor({ processName }: ProcessEditorProps): JSX.Element {
  // 从 ThemeProvider 获取当前主题模式，传给 Monaco 编辑器
  const { themeMode } = useTheme();

  // 工艺详情（含 definition YAML 文本）
  const [detail, setDetail] = useState<ProcessTemplateDetail | null>(null);
  // 加载中标志，控制 Spin 显示
  const [loading, setLoading] = useState(true);
  // 当前 YAML 文本（Monaco 编辑的内容，受控）
  const [yamlText, setYamlText] = useState('');
  // 是否系统工艺，决定 readOnly 和 Alert 颜色
  const [isSystem, setIsSystem] = useState(false);

  // 加载工艺详情副作用
  // 依赖 [processName]：工艺名变化时重新加载
  useEffect(() => {
    // 标记加载开始
    setLoading(true);
    // 清空旧数据，避免切换工艺时闪现上一个工艺的内容
    setDetail(null);
    setYamlText('');
    setIsSystem(false);

    // 定义异步加载函数；
    // 用独立函数避免 useEffect 直接返回 Promise（React 不允许）
    const loadDetail = async () => {
      try {
        // 调用 M1 已有的 API 客户端获取工艺详情
        const result = await bundledApi.getProcess(processName);
        // 设置详情和 Monaco 文本
        setDetail(result);
        setYamlText(result.definition);
        setIsSystem(result.is_system);
      } catch {
        // 加载失败时 message.error 提示；
        // detail 保持 null，下方会渲染 Empty
        message.error(`加载工艺「${processName}」失败`);
      } finally {
        // 无论成功失败都关闭 loading
        setLoading(false);
      }
    };

    // 触发异步加载
    void loadDetail();
  }, [processName]);

  // 复制系统工艺到用户层
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

  // ── 渲染分支 ──────────────────────────────────────────

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

  // 主渲染：Alert + Monaco 编辑器
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
          message="编辑保存后写入 ~/.ntd/processes/"
          style={alertStyle}
        />
      )}

      {/* Monaco YAML 编辑器：填满剩余高度 */}
      <div style={monacoWrapperStyle}>
        <ProcessYamlEditor
          value={yamlText}
          onChange={setYamlText}
          readOnly={isSystem}
          theme={themeMode}
        />
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

// 编辑器主容器：纵向 flex，Alert 固定高度，Monaco 填满剩余
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

// Monaco 包装器：flex 1，填满 Alert 之外的剩余高度
const monacoWrapperStyle: CSSProperties = {
  flex: 1,
  minHeight: 400,
};
