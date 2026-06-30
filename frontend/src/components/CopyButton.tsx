/**
 * 复制按钮组件
 *
 * 封装 clipboard.js，将 clipboard.js 直接绑定到真实按钮上，
 * 省去临时 DOM 创建/销毁的开销，遵循 clipboard.js 推荐的事件驱动模式。
 *
 * 用法：
 * ```tsx
 * <CopyButton text="要复制的内容" onCopy={() => message.success('已复制')}>
 *   复制
 * </CopyButton>
 * ```
 */
import { useEffect, useRef, useCallback, useState, forwardRef } from 'react';
import { Button } from 'antd';
import type { ButtonProps } from 'antd';
import ClipboardJS from 'clipboard';
import { CopyOutlined, CheckOutlined } from '@ant-design/icons';

export interface CopyButtonProps extends Omit<ButtonProps, 'onClick' | 'children'> {
  /** 要复制的文本 */
  text: string;
  /** 复制成功回调 */
  onCopy?: () => void;
  /** 按钮内容 */
  children?: React.ReactNode;
  /** 复制成功后是否短暂显示勾号（默认 true） */
  showFeedback?: boolean;
  /** 反馈持续时间（ms，默认 2000） */
  feedbackDuration?: number;
}

export const CopyButton = forwardRef<HTMLButtonElement | HTMLAnchorElement, CopyButtonProps>(function CopyButton({
  text,
  onCopy,
  children = '复制',
  showFeedback = true,
  feedbackDuration = 2000,
  icon = <CopyOutlined />,
  ...rest
}, ref) {
  const innerRef = useRef<HTMLButtonElement | HTMLAnchorElement>(null);
  const clipboardRef = useRef<ClipboardJS | null>(null);
  const onCopyRef = useRef(onCopy);
  const [copied, setCopied] = useState(false);

  // 始终保持最新的 onCopy 引用，避免内联函数导致 ClipboardJS 实例频繁重建
  onCopyRef.current = onCopy;

  const handleSuccess = useCallback(() => {
    setCopied(true);
    onCopyRef.current?.();
    if (showFeedback) {
      setTimeout(() => setCopied(false), feedbackDuration);
    }
  }, [showFeedback, feedbackDuration]);

  useEffect(() => {
    const el = innerRef.current;
    if (!el) return;

    // 使用 clipboard.js 数据属性方式绑定
    el.setAttribute('data-clipboard-text', text);

    const clipboard = new ClipboardJS(el as Element);
    clipboardRef.current = clipboard;

    clipboard.on('success', handleSuccess);
    clipboard.on('error', () => {
      // clipboard.js 失败时不处理，由上层 onCopy 决定是否提示
    });

    return () => {
      clipboard.destroy();
      clipboardRef.current = null;
    };
  }, [text, handleSuccess]);

  // text 变化时更新 data 属性
  useEffect(() => {
    if (innerRef.current) {
      innerRef.current.setAttribute('data-clipboard-text', text);
    }
  }, [text]);

  return (
    <Button
      ref={(node) => {
        innerRef.current = node;
        if (typeof ref === 'function') ref(node);
        else if (ref) ref.current = node;
      }}
      icon={copied ? <CheckOutlined /> : icon}
      {...rest}
    >
      {children}
    </Button>
  );
});
