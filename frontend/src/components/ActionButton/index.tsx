import { useState, useEffect, useCallback, useRef } from 'react';
import { Button, Drawer, Spin, Typography, Space, message, Input, Tag } from 'antd';
import { ThunderboltOutlined, EditOutlined } from '@ant-design/icons';
import { useIsMobile } from '@/hooks/useIsMobile';
import { ChatView } from '@/components/ChatView';
import { useActionExecution } from './useActionExecution';
import { ExecutorPickerPopover } from '@/components/common/ExecutorPickerPopover';
import { WorkspaceSwitcher } from '@/components/shell/WorkspaceSwitcher';
import { getLastExecutor, setLastExecutor } from '@/constants';
import type { ActionButtonProps } from './types';

const { Text, Paragraph } = Typography;
const { TextArea } = Input;

/**
 * 把模板里的 {{key}} 占位符替换为参数值。
 * 用 split/join 而非 RegExp/replaceAll：无需正则转义、不依赖 ES2021 target，
 * 天然规避 key/value 含元字符（如 . * +）时的误匹配。
 */
function substituteParams(template: string, params: Record<string, string>): string {
  let result = template;
  for (const [key, value] of Object.entries(params)) {
    result = result.split(`{{${key}}}`).join(value);
  }
  return result;
}

/**
 * 可复用的一键 AI 执行组件。
 *
 * 交互流程：
 * 1. 点击按钮 → 打开 Drawer
 * 2. 展示可编辑的 Prompt、执行器选择器、参数预览
 * 3. 用户可修改后点击「执行」
 * 4. 通过 WebSocket 监听执行完成
 * 5. 完成后展示完整 markdown 结果
 * 6. 用户选择「应用」或「拒绝」
 */
export function ActionButton({
  actionType,
  actionKey,
  prompt,
  params,
  onApply,
  workspaceId,
  children,
  buttonType = 'default',
  icon,
  disabled = false,
  panelTitle = '自动优化标题',
  panelDescription = '检查并确认以下内容后执行',
  executor,
  buttonSize = 'middle',
  showLabel = true,
  completedView,
}: ActionButtonProps) {
  const [open, setOpen] = useState(false);
  const [editablePrompt, setEditablePrompt] = useState(prompt);
  // editablePrompt 的 ref 镜像：effect 里要读最新值，避免闭包捕获到旧的 state
  const editablePromptRef = useRef(editablePrompt);
  editablePromptRef.current = editablePrompt;
  // 记录「上次自动生成的 prompt」，用于判断用户是否手动编辑过：
  // 当前 editablePrompt 仍等于上次生成值 → 视为未手改，参数变化可安全覆盖；
  // 一旦不等（用户改过 textarea）→ 后续参数变化不再覆盖，保留手动编辑。
  const lastGeneratedRef = useRef(editablePrompt);
  // 模板参数值：存储用户输入的参数，初始化时使用 params 的默认值
  const [paramValues, setParamValues] = useState<Record<string, string>>(params);
  // 初始化 selectedExecutor：优先从 localStorage 恢复上次选择，不存在时回退到 prop executor
  const [selectedExecutor, setSelectedExecutor] = useState<string | undefined>(
    () => getLastExecutor(executor)
  );
  // 工作空间 ID：初始值来自 prop，用户可在 Drawer 内切换
  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState<number | null | undefined>(
    workspaceId ?? undefined
  );
  const isMobile = useIsMobile();
  const { status, result, error, logs, execute, retry, reset } = useActionExecution(
    actionType,
    actionKey,
    prompt,
    params,
    selectedWorkspaceId ?? undefined,
    executor,
  );

  // 打开 Drawer 的初始化放在 handleOpen（点击瞬间执行），而非 useEffect：
  // effect 若依赖 params/prompt 引用，调用方传对象字面量时父组件每次重渲染都会
  // 产生新引用、反复触发 effect，覆盖用户正在编辑的 prompt。

  // 参数值变化时，实时把占位符替换进 prompt。
  // 仅当用户未手动编辑（当前值仍等于上次自动生成值）时才覆盖 editablePrompt，
  // 否则保留用户的手动修改；始终刷新 lastGeneratedRef 以便下次比较。
  useEffect(() => {
    const generated = substituteParams(prompt, paramValues);
    if (editablePromptRef.current === lastGeneratedRef.current) {
      setEditablePrompt(generated);
    }
    lastGeneratedRef.current = generated;
  }, [paramValues, prompt]);

  // 用户切换执行器时同时保存选择到 localStorage，
  // 确保本次关闭后下次打开 Drawer 能恢复成这个值。
  const handleExecutorChange = useCallback((value: string) => {
    setSelectedExecutor(value);
    setLastExecutor(value);
  }, []);

  // 打开瞬间一次性初始化：prompt（参数替换后）、参数值、执行器（localStorage 恢复）。
  // 普通函数每次渲染捕获最新 props，点击时用的是当前 prompt/params/executor。
  const handleOpen = () => {
    reset();
    const generated = substituteParams(prompt, params);
    setEditablePrompt(generated);
    lastGeneratedRef.current = generated;
    setParamValues(params);
    setSelectedExecutor(getLastExecutor(executor));
    setOpen(true);
  };

  const handleClose = () => {
    setOpen(false);
  };

  const handleExecute = () => {
    execute(editablePrompt, selectedExecutor);
  };

  const handleRetry = () => {
    retry(editablePrompt, selectedExecutor);
  };

  const handleApply = async () => {
    // onApply 仅在走默认完成视图时由调用方提供；提供 completedView 的场景不会走到这里
    if (!result || !onApply) return;
    try {
      await onApply(result);
      message.success('已应用');
      handleClose();
    } catch (err: any) {
      message.error(err?.message || '应用失败');
    }
  };

  // 从 params 中提取要展示的预览内容
  const paramsPreview = Object.entries(params)
    .map(([key, value]) => ({ key, value }));

  const renderContent = () => {
    if (status === 'idle') {
      return (
        <Space direction="vertical" size="middle" style={{ width: '100%' }}>
          {/* 描述 */}
          <Text type="secondary">{panelDescription}</Text>

          {/* Prompt 编辑区 */}
          <div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 6 }}>
              <EditOutlined style={{ color: 'var(--color-text-secondary)' }} />
              <Text strong style={{ fontSize: 13 }}>Prompt 模板</Text>
            </div>
            <TextArea
              value={editablePrompt}
              onChange={(e) => setEditablePrompt(e.target.value)}
              autoSize={{ minRows: 4, maxRows: 12 }}
              style={{ fontFamily: 'monospace', fontSize: 12 }}
            />
          </div>

          {/* 参数输入区（移至工作空间上方） */}
          {paramsPreview.length > 0 && (
            <div>
              <Text strong style={{ fontSize: 13, display: 'block', marginBottom: 6 }}>
                模板参数
              </Text>
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: 16 }}>
                {paramsPreview.map(({ key }) => (
                  <div key={key} style={{ flex: '1 1 200px' }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 4, marginBottom: 4 }}>
                      <Tag color="blue" style={{ fontSize: 12 }}>{`{{${key}}}`}</Tag>
                    </div>
                    <Input
                      value={paramValues[key] ?? ''}
                      onChange={(e) => setParamValues((prev) => ({ ...prev, [key]: e.target.value }))}
                      placeholder={`请输入 ${key}`}
                      size="small"
                    />
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* 工作空间 + 执行器 横排布局：左工作空间、右执行器 */}
          <div style={{ display: 'flex', gap: 16 }}>
            {/* 工作空间 */}
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 12, color: 'var(--color-text-tertiary)', marginBottom: 6 }}>
                工作空间
              </div>
              <WorkspaceSwitcher
                value={selectedWorkspaceId ?? null}
                onChange={setSelectedWorkspaceId}
              />
            </div>
            {/* 执行器 */}
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 12, color: 'var(--color-text-tertiary)', marginBottom: 6 }}>
                执行器
              </div>
              <ExecutorPickerPopover
                value={selectedExecutor}
                onChange={handleExecutorChange}
              />
            </div>
          </div>
        </Space>
      );
    }

    if (status === 'executing') {
      // 实时日志流：复用 ChatView 把 WS Output 事件 push 来的 logs 渲染成
      // 思考/工具/输出气泡，让用户看见 AI 在干什么，而非黑盒转圈。
      // 容器限定 60vh 高度，ChatView 内部 .chat-container flex:1 自行滚动并自动滚到底。
      return (
        <div style={{ display: 'flex', flexDirection: 'column', height: '60vh' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
            <Spin size="small" />
            <Text type="secondary">AI 正在处理中...</Text>
          </div>
          <ChatView logs={logs} isRunning />
        </div>
      );
    }

    if (status === 'failed') {
      return (
        <Space direction="vertical" size="middle" style={{ width: '100%' }}>
          <Text type="danger">{error || '执行失败'}</Text>
        </Space>
      );
    }

    // completed
    // 提供自定义完成视图时全权交给插槽（如建议列表）；否则走默认「结果原文」展示
    if (completedView) {
      return completedView({ result: result ?? '', close: handleClose, retry: handleRetry });
    }
    return (
      <Space direction="vertical" size="middle" style={{ width: '100%' }}>
        <Text type="secondary">AI 生成结果：</Text>
        <div
          style={{
            padding: 12,
            background: 'var(--color-success-bg, #f6ffed)',
            border: '1px solid var(--color-success-border, #b7eb8f)',
            borderRadius: 6,
            maxHeight: 400,
            overflow: 'auto',
          }}
        >
          <Paragraph
            style={{ whiteSpace: 'pre-wrap', margin: 0 }}
            ellipsis={{ expandable: true, symbol: '展开' }}
          >
            {result}
          </Paragraph>
        </div>
      </Space>
    );
  };

  const renderFooter = () => {
    if (status === 'idle') {
      return (
        <Space>
          <Button onClick={handleClose}>取消</Button>
          <Button type="primary" onClick={handleExecute}>
            执行
          </Button>
        </Space>
      );
    }

    if (status === 'executing') {
      return null;
    }

    if (status === 'failed') {
      return (
        <Space>
          <Button onClick={handleClose}>关闭</Button>
          <Button type="primary" onClick={handleRetry}>
            重试
          </Button>
        </Space>
      );
    }

    // completed
    // 自定义完成视图自带操作按钮（sticky 底栏），Drawer footer 置空避免重复
    if (completedView) {
      return null;
    }
    return (
      <Space>
        <Button onClick={handleClose}>拒绝</Button>
        <Button type="primary" onClick={handleApply}>
          应用
        </Button>
      </Space>
    );
  };

  return (
    <>
      <Button
        type={buttonType}
        size={buttonSize}
        icon={icon || <ThunderboltOutlined />}
        onClick={handleOpen}
        disabled={disabled}
      >
        {showLabel && (children || '优化标题')}
      </Button>

      <Drawer
        title={panelTitle}
        open={open}
        onClose={status !== 'executing' ? handleClose : undefined} // 执行中禁止关闭，其他时候允许通过 X 按钮关闭
        closable={status !== 'executing'}
        keyboard={false} // 禁止 Escape 关闭
        maskClosable={false} // 始终禁止点击遮罩关闭
        placement={isMobile ? 'bottom' : 'right'}
        width={isMobile ? '100%' : 520}
        height={isMobile ? '85vh' : undefined}
        footer={renderFooter()}
        destroyOnHidden
      >
        {renderContent()}
      </Drawer>
    </>
  );
}
